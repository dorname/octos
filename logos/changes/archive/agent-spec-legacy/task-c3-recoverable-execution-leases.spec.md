spec: task
name: "可恢复执行：队列/租约/fencing/tool intent/检查点（recoverable-execution-leases）"
tags: [runtime, recovery, leases, fencing, checkpoints, cluster]
estimate: 4w
---

- **task**: c3-recoverable-execution-leases
- **status**: proposed
- **phase**: P3（可恢复执行；依赖 c1 的 Scope 与 c2 的持久化边界）
- **ADR**: docs/adr/cluster-state-and-execution.md（D3 检查点恢复、D4 副作用 Unknown、D5 租约 fencing）
- **验收矩阵**: K02（单 owner 领取）、K03（分区接管 fencing）、K04（副作用幂等/Unknown）、K09（子任务 join 一次）

## Intent

当前 TaskSupervisor / agent_orchestrator / supervisor_store 把业务状态机
与进程内执行 handle 混在一起：任务真相在内存注册表与本地账本里，审批
挂起依赖 oneshot，工具副作用没有 intent 记录。Worker 被杀后无法区分
「外部已成功但结果未落盘」与「未执行」，多副本领取也没有 epoch 防护。

本任务把执行改造成可恢复任务：持久化命令队列、run_leases（owner/epoch/
expires_at 事务内占有与递增）、run_checkpoints（transcript highwater、
context、workspace revision、pending invocation、artifact refs、digest）、
tool_invocations 幂等账本、取消与 join 语义。退出条件：强杀与网络分区
演练通过；副作用 Unknown 不误重试。

## Decisions

1. **业务状态机与执行 handle 分离**：`agent_runs`（run_id、parent_run、
   definition/binding revision、runtime/schema version、state、attempt、
   cancel_requested）与 `commands`（幂等唯一约束、not_before）落 PG；
   进程内 handle 仅作执行期优化，真相在库。
2. **租约与 fencing 在数据库事务内完成**：领取 = 同事务写 run_leases
   （owner_id、epoch 单调递增、expires_at）。任何业务状态写入携带当前
   epoch；旧 epoch 写被判失效拒绝。租约超期由 Scheduler/Recovery 扫描
   接管，接管方先递增 epoch 再恢复执行。
3. **检查点是恢复的唯一入口**：Worker 在定义好的边界（工具调用前后、
   审批挂起、turn 完成）提交 run_checkpoints；恢复方从最新有效
   checkpoint 重建 runtime，不做内存迁移。checkpoint 带 schema/runtime
   revision，digest 校验失败按损坏处理（不猜测修复）。
4. **工具副作用先记 intent 再执行**：tool_invocations 唯一约束
   （logical invocation ID + args_hash + tool revision）；有外部幂等键
   的副作用只生效一次；外部成功但结果未落盘的 Unknown 状态进入显式
   对账队列，不自动重发；无幂等能力的写操作在租约失效后不重试。
5. **取消/join 持久化**：cancel_requested 为库内标志，控制命令经独立
   通道可消费（不排在长 LLM 调用后）；并行子任务终态重复投递时父任务
   只 join/续执行一次，费用不重复结算。
6. **PG 队列起步**：第一阶段以 PostgreSQL 持久化任务队列 + outbox 实现，
   不引入独立消息中间件；Redis 仅限流/短期缓存。
7. **版本固定到 run**（ADR D9 的执行侧）：恢复时继续使用 checkpoint
   记录的 binding digest/权限快照，即使 Binding 已更新；撤销策略仍
   优先阻断新调用。

## Boundaries

### Allowed Changes

- specs/task-c3-recoverable-execution-leases.spec.md
- crates/octos-cli/src/（task_supervisor、agent_orchestrator、supervisor_store 拆分状态机/handle）
- crates/octos-agent/src/（Agent 循环检查点边界、tool intent 钩子）
- crates/octos-store/（agent_runs/run_leases/run_checkpoints/tool_invocations repository）
- crates/octos-pipeline/src/（checkpoint store async 化、节点运行状态）
- crates/octos-cli/tests/、crates/octos-store/tests/（含故障注入测试）

### Forbidden

- 不改 OUP wire 协议（需要时走 c1 的 UPCR 流程）
- 不实现 Cron durable firing（归 c5/后续切片；K10 不在本 spec）
- 不做 API 多副本部署清单与 server 提取（P2 归运维/部署切片）
- 不动插件工厂、MCP 适配层（下一目标）
- 不删既有测试断言换绿；不 push/PR

## Acceptance Criteria

### Rule: lease-claim — 多 Worker 领取同一 Scope 只有一个有效 owner

Scenario: 并发领取仅一人成功（critical）
  Tags: critical, K02
  Test:
    Package: octos-store
    Filter: concurrent_lease_claim_yields_single_owner
  Given 同一 Scope 有待领取任务
  When 两个 Worker 并发 claim
  Then 恰好一个持有有效 lease 与 epoch；不同 Scope 的领取互不阻塞
  Level: integration
  Test Double: 真实 PG，双事务并发

Scenario: 租约超期后被接管且 epoch 递增
  Test:
    Package: octos-store
    Filter: expired_lease_takeover_increments_epoch
  Given owner A 的 lease 已过期
  When owner B 接管
  Then epoch 严格递增；A 的后续写入全部被判失效

### Rule: fencing — 分区旧主不可写

Scenario: 网络分区后旧 epoch 不可写业务状态（critical）
  Tags: critical, K03
  Test:
    Package: octos-cli
    Filter: partitioned_old_epoch_cannot_write_or_dispatch
  Given Worker A 与 DB 分区，Worker B 已取得新 epoch
  When A 分区恢复后尝试写状态/派发新操作
  Then 全部拒绝；A 的晚到结果按受控策略记录，不覆盖 B 的已确认状态
  Level: integration
  Test Double: 故障注入（延迟/丢弃 DB 调用）

### Rule: side-effect-idempotency — 外部副作用不误重试

Scenario: 工具外部成功后立即杀 Worker（critical）
  Tags: critical, K04
  Test:
    Package: octos-cli
    Filter: external_success_then_kill_does_not_duplicate_effect
  Given 有幂等键的工具外部已生效但结果未落盘时 Worker 被杀
  When 接管方恢复执行
  Then 副作用凭幂等键只生效一次；已确认结果被复用不重复计费

Scenario: 无幂等能力的 Unknown 不自动重发（critical）
  Tags: critical, K04
  Test:
    Package: octos-cli
    Filter: unknown_side_effect_goes_to_reconciliation_not_retry
  Given 无幂等能力的写工具处于 Unknown 状态
  When 接管方恢复
  Then 进入显式对账队列等待人工/策略处理，不自动重发

### Rule: checkpoint-resume — 从检查点重建

Scenario: 崩溃后从已提交检查点恢复（critical）
  Tags: critical
  Test:
    Package: octos-cli
    Filter: should_resume_from_committed_checkpoint
  Given 存在已持久化检查点（含 transcript highwater 与 pending invocation）
  When 原 Worker 终止且另一 Worker 取得新 epoch
  Then 已确认工具结果被复用，上下文按 highwater 重建，不重复执行

Scenario: checkpoint digest 损坏拒绝猜测修复
  Test:
    Package: octos-cli
    Filter: corrupted_checkpoint_digest_fails_closed
  Given checkpoint digest 校验失败
  When 恢复方加载
  Then 标记损坏并走明确终止/人工处理路径，不猜测内容继续

### Rule: cancel-and-join — 取消可达、join 一次

Scenario: 长 LLM 调用期间 interrupt 可消费
  Test:
    Package: octos-cli
    Filter: interrupt_consumed_during_long_llm_call
  Given run 正在长 LLM 调用
  When 设置 cancel_requested
  Then 取消在定义边界生效，不排在 LLM 调用之后无限等待

Scenario: 并行子任务终态重复投递只 join 一次（critical）
  Tags: critical, K09
  Test:
    Package: octos-cli
    Filter: duplicate_child_terminal_joins_parent_once
  Given 子任务终态被重复投递
  When 父任务处理
  Then join/续执行恰好一次，usage 不重复结算

### Rule: binding-pin — 恢复用旧版本快照

Scenario: Binding 更新后旧 run 崩溃接管（critical）
  Tags: critical, K11
  Test:
    Package: octos-cli
    Filter: resumed_run_uses_pinned_binding_snapshot
  Given Binding 已更新后旧 run 崩溃
  When 新 Worker 接管
  Then 恢复执行继续使用 checkpoint 记录的旧 digest/权限快照；
       已撤销的 Binding 仍优先阻断新调用
