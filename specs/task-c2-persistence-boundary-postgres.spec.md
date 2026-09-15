spec: task
name: "持久化边界：async repository/UnitOfWork/PG schema/local adapter（persistence-boundary-postgres）"
tags: [persistence, postgres, repository, cluster, octos-store]
estimate: 3w
---

- **task**: c2-persistence-boundary-postgres
- **status**: proposed
- **phase**: P1（持久化边界）
- **ADR**: docs/adr/cluster-state-and-execution.md（D1 PG 真相源、D6 事件账本、D8 local adapter）
- **验收矩阵**: K01（幂等落库）、K05（审批跨 Pod 恢复）、K07（RLS/租户隔离）、K15 前置（失败不发布未落盘成功）

## Intent

当前 octos-store 只有文件/单机语义的 store 模块（user、token、audit、
usage 等），会话 canonical 走 JSONL，记忆走 redb，审批走 oneshot——没有
async repository 抽象、没有跨模块事务、没有 PostgreSQL 后端。仅把
`octos serve` 放入多副本 Deployment 会遇到 redb 打开冲突与会话并发写入。

本任务定义 async repositories + Unit of Work 抽象，实现 PostgreSQL 后端
与 local adapter 双后端，把会话 canonical（消息/线程/控制记录）、
审批/提问、事件与 outbox、登录会话/OTP/撤销纳入同一事务边界。退出条件：
单副本 PG 模式功能对照 local 模式通过；重启后消息/上下文/审批一致。

## Decisions

1. **Unit of Work 是唯一事务边界**：一次业务转换涉及消息 + 运行状态 +
   事件 + outbox 时，必须由共享 UoW 在同一事务提交。禁止给每个旧模块
   单独加 `save_to_postgres()`，禁止无事务长期双写。事务失败不可把
   内存结果当作已成功；缓存提交后更新。
2. **async 接口为目标形态**：repository 方法全部 async。过渡期确需保留
   同步接口的调用点，使用受控阻塞线程桥接并限制并发，禁止在 Tokio 主
   执行线程阻塞等待数据库。
3. **PG schema 以 Scope 复合键建模**（依赖 c1）：`sessions`、`messages`/
   `session_control_records`、`commands`（请求幂等唯一约束）、
   `approvals`/`questions`（pending/决策/超时/续执行，不存 oneshot 作为
   唯一事实）、`session_events`（`UNIQUE(Scope, seq)` 与 event_id 去重）、
   `outbox`（同事务生成投递项）。所有租户业务表含 tenant_id，关键外键
   采用含 tenant_id 的复合约束；应用角色不使用 superuser/BYPASSRLS，
   可启用 RLS，连接池每事务设置并复位 tenant 上下文。
4. **审批 durable waiting + reply CAS**：pending 审批/提问落库，含
   originating run/step、args_hash、binding revision、state、decision、
   expires_at、resume command；回复以 CAS 写入，重复/跨租户/参数变化
   被拒绝。兼容本地 oneshot adapter（单机模式行为不变）。
5. **事件账本 PG 化**：`session_events` 按完整 Scope 分配单调 seq；
   持久化后才推送；本地 broadcast 降级为加速层，丢失时按 seq 回放或
   显式 resync。老事件归档对象存储的 key 含 tenant。
6. **local adapter 保留**：单机 `octos chat`/`gateway` 继续走文件后端，
   不要求 PostgreSQL；双后端共享 repository 契约测试。
7. **迁移工具与 expand/contract**：schema 迁移采用 expand/contract；
   本任务交付 schema 与迁移工具，存量数据迁移演练归 c5。
8. **秘密不进入业务库明文**：Provider 凭据/OAuth refresh token 走 KMS
   加密引用；OTP 校验/次数/消费原子化（可用受控 Redis TTL），撤销
   跨 Pod 生效。

## Boundaries

### Allowed Changes

- specs/task-c2-persistence-boundary-postgres.spec.md
- crates/octos-store/（async repository traits + UoW + PG 后端 + 迁移）
- crates/octos-store-postgres/（如需独立 crate，按 ADR §7.5 先模块化再拆）
- crates/octos-bus/src/（session/context_manager 持久化走 repository）
- crates/octos-cli/src/contracts/（approvals/questions durable 化 + 本地 oneshot adapter）
- crates/octos-cli/src/api/（otp/login session 走 repository）
- crates/octos-cli/tests/、crates/octos-store/tests/（契约与集成测试）
- migrations/ 或 crates/octos-store/migrations/（schema 迁移）

### Forbidden

- 不实现队列/租约/fencing/检查点恢复（归 c3）
- 不改 OUP wire 协议字段（归 c1 的 UPCR 流程）
- 不做插件目录/Binding 表（归下一目标的工厂目标）
- 不删既有测试断言换绿；不在 Tokio 主线程阻塞等 DB；不 push/PR

## Acceptance Criteria

### Rule: uow-atomicity — 业务转换单事务提交

Scenario: 消息+运行状态+事件+outbox 同事务（critical）
  Tags: critical
  Test:
    Package: octos-store
    Filter: uow_commits_message_runstate_event_outbox_atomically
  Given 一次 turn 完成需要写消息、run 终态、事件与 outbox
  When 在事件写入前注入故障
  Then 四个聚合全部不可见（无部分提交）；故障恢复后无 phantom completion

Scenario: 事务失败不发布内存结果
  Test:
    Package: octos-store
    Filter: failed_transaction_does_not_publish_in_memory_success
  Given 提交事务在落盘阶段失败
  When 调用方观察到错误
  Then 缓存与广播均未更新；重放不产生已确认假象

### Rule: dual-backend-parity — local adapter 与 PG 后端契约一致

Scenario: 双后端通过同一 repository 契约测试（critical）
  Tags: critical
  Test:
    Package: octos-store
    Filter: repository_contract_suite_passes_on_local_and_postgres
  Given repository 契约测试套件
  When 分别对 local 文件后端与 PG 后端执行
  Then 两后端全部通过；单机模式无需 PG 即可启动

Scenario: 重启后消息/上下文/审批一致（critical）
  Tags: critical
  Test:
    Package: octos-cli
    Filter: restart_preserves_messages_context_and_pending_approvals
  Given PG 模式下存在消息、压缩上下文与一个 pending 审批
  When 进程重启
  Then 消息顺序、上下文 highwater 与审批 pending 状态完整恢复

### Rule: approval-durability — 审批跨重启与副本恢复

Scenario: 审批请求后杀掉原进程，从另一实例批准（critical）
  Tags: critical, K05
  Test:
    Package: octos-cli
    Filter: approval_survives_originator_restart_and_cas_reply
  Given 实例 A 持久化 pending 审批后终止
  When 实例 B 收到批准决定
  Then CAS 写入一次决策；重复回复、跨租户回复、参数变化的回复均被拒绝
  Level: integration
  Test Double: 双进程共享同一 PG

### Rule: tenant-isolation — 租户隔离在存储层成立

Scenario: 跨租户查询不命中（critical）
  Tags: critical, K07
  Test:
    Package: octos-store
    Filter: cross_tenant_queries_return_no_rows
  Given 两租户存在相同 wire session 的数据
  When 以租户 A 上下文执行全部 repository 查询
  Then 不返回租户 B 的任何行；对象存储 key 与缓存键同样隔离

### Rule: event-ledger — 事件序号与 outbox

Scenario: 事件按 Scope 单调 seq 且去重
  Test:
    Package: octos-store
    Filter: session_events_monotonic_seq_and_idempotent_event_id
  Given 同 Scope 并发提交事件与重复 event_id
  When 事务提交
  Then seq 无缺口无重复；重复 event_id 被去重；outbox 与事件同事务可见

### Rule: migration-safety — expand/contract 与无同步阻塞

Scenario: schema 迁移可前滚与回退
  Test:
    Package: octos-store
    Filter: migrations_expand_and_contract_cleanly
  Given 初始 schema
  When 前滚再按 contract 回退
  Then 数据保留完整，旧应用版本在 expand 阶段仍可读写

Scenario: repository 不在 Tokio 主线程阻塞
  Test:
    Package: octos-store
    Filter: repositories_do_not_block_tokio_runtime
  Given 受控阻塞桥接的过渡期同步调用点
  When 在压测下观察运行时
  Then 主执行线程无阻塞等待数据库；桥接并发受上限约束
