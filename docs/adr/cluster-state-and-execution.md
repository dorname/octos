# Cluster State and Execution — Kubernetes 无状态化跨层架构决策

- Date: 2026-09-15 (proposal)
- Status: **Proposed**。本 ADR 是 P0 基线与契约阶段的跨层决策记录，落盘
  后进入设计审查；实施切片以 `spec: task` 驱动（见 §任务 specs 索引）。
- Source analysis:
  [docs/analysis/octos-k8s-plugin-factory-plan-2026-09-14.md](../analysis/octos-k8s-plugin-factory-plan-2026-09-14.md)
  （源码基线 `main @ 6cfc689e`，workspace `2.0.3-rc.11`）。
- Base revision: `main @ 6cfc689e`。

## Context

Octos 当前的业务状态真相分散在：本机 JSONL/JSON 账本、redb 记忆库、进程内
`SessionRuntimeCache` / `TaskSupervisor` 注册表、oneshot 审批通道、工作区
文件系统。把 `octos serve` 原样放进多副本 Deployment 会遇到 redb 打开
冲突、会话并发写入、审批无法跨 Pod 恢复、后台任务失联四类已确认问题。

本 ADR 只决策**无状态化**（P1 持久化边界 / P2 API 多副本 / P3 可恢复执行 /
P5 生产演练）。插件工厂（P4）的目录、构建、发布、Binding 生命周期在本 ADR
中只确立一条边界决策（D9 固定到 run 的版本快照），工厂自身的架构 ADR 与
任务 spec 保留到后续目标执行，不在本轮入库。

## Decision summary

### D1 — PostgreSQL 是业务状态唯一真相源

会话 canonical 记录、任务/Goal/Supervisor 状态机、审批、Cron、事件与
outbox、预算与用量，全部落 PostgreSQL。本机文件（JSONL、redb）降级为
local adapter 下的本地模式后端，或迁移来源，不再是集群模式的真相。
涉及消息 + 运行状态 + 事件 + outbox 的业务转换必须由共享 Unit of Work
在同一事务提交；禁止无事务的长期双写。

### D2 — 完整 Scope 是多租户隔离的单位

```text
Scope = tenant_id + profile_id + workspace_id + session_id
Execution = Scope + thread_id + run_id + attempt_id
```

`workspace_id` 为服务端分配的逻辑身份；wire session ID 进入服务端后绑定
到 Scope，沿查询、缓存、广播、审批、工具、对象存储路径与审计全过程携带。
所有租户业务表含 `tenant_id`，关键外键采用含 `tenant_id` 的复合约束；
可启用 PostgreSQL RLS 作为应用授权补充（应用角色不使用 superuser /
BYPASSRLS）。

### D3 — 恢复语义是「持久化检查点 + 重建执行」，不是内存迁移

Worker 可以持有内存、连接与本地缓存，但必须可重建。Pod 销毁不丢失已确认
业务状态，请求不要求回到原 Pod。运行恢复依赖 `run_checkpoints`（含
transcript highwater、context、workspace revision、pending invocation、
digest）与 `tool_invocations` 账本，而非任何进程内状态。

### D4 — 外部副作用 Unknown 必须显式对账，不得静默重试

工具执行先持久化 intent（`tool_invocations`：logical invocation ID、
tool revision、args_hash、state、external idempotency key、result ref）。
有幂等键的副作用只生效一次；外部成功但结果未落盘的 Unknown 状态进入
显式对账流程，不自动重发。无幂等能力的写操作在租约失效后不重试。

### D5 — 租约 + epoch fencing 由数据库事务保证

`run_leases`（owner_id、epoch、expires_at）的占有与递增在数据库事务内
完成。旧 epoch 持有者不可写业务状态、不可派发新操作；网络分区后被接管的
scope，其晚到结果按受控策略处理（K03 验收）。

### D6 — 事件账本：PG 单调序号 + outbox，本地 broadcast 降级为加速层

`session_events` 按完整 Scope 分配单调 `seq`（`UNIQUE(Scope, seq)` 与
event_id 去重），同事务生成 outbox 投递项。WebSocket 推送建立在持久化
回放之上：断线/切 API/通知丢失后按 seq 回放，无缺口或显式 resync；
慢客户端不拖垮 Worker（K06）。

### D7 — API/WS Edge、Agent Worker、Scheduler 为独立扩缩容的逻辑部署单位

第一阶段是逻辑边界，不强制拆仓库：同一 Cargo workspace 产出 API、Worker、
Scheduler 二进制（角色化启动），各自有独立的扩缩容依据与本地状态允许
范围。`octos-server` 当前为 Stage 1 scaffold，server 提取在 P2 完成。

### D8 — 本地 chat/gateway 保留 local adapter 后端

存储抽象（async repositories + Unit of Work）同时提供 PG 后端与 local
文件后端；单机 `octos chat` / `gateway` 不要求 PostgreSQL。抽象边界在
repository 层，不给每个旧模块单独加 `save_to_postgres()`。

### D9 — 版本固定到 run（与插件工厂的唯一边界决策）

`agent_runs` 记录 definition/binding revision 与 runtime/schema version；
Binding 更新后，旧 run 崩溃接管时继续使用旧 digest/权限快照，撤销策略
仍优先（K11）。工厂目录/发布/回滚的控制面设计不在本 ADR 范围。

## Consequences

- OUP 新增字段（accepted/run_id、恢复状态）需走 UPCR 流程与兼容测试，
  不能只改服务端。
- 迁移以 profile 为单位的短暂停写迁移；禁止复制运行中的 redb 文件充当
  一致备份。回滚优先使用仍能读取新 PG schema 的兼容应用版本，数据库
  采用 expand/contract 迁移。
- 原有同步接口向 async 适配时，过渡期受控阻塞线程桥接并限制并发，
  禁止在 Tokio 主执行线程阻塞等待数据库。

## 任务 specs 索引（本轮入库）

| Spec | 覆盖阶段 | 主要验收编号 |
|---|---|---|
| `specs/task-c1-cluster-scope-execution-context.spec.md` | P1 统一 Scope 与 ExecutionContext | K01, K07 |
| `specs/task-c2-persistence-boundary-postgres.spec.md` | P1 持久化边界（repository/UoW/PG schema/local adapter） | K01, K05, K07 |
| `specs/task-c3-recoverable-execution-leases.spec.md` | P3 可恢复执行（queue/lease/fencing/tool intent/checkpoint） | K02, K03, K04, K09 |
| `specs/task-c5-production-drill-migration.spec.md` | P5 数据迁移、故障接管与回滚演练 | K10, K15, K16, K17 |

插件工厂（P4：`octos-factory`、Binding 控制面、K11–K14/K18–K20 的工厂
部分）**不在本轮**：其 spec 与 ADR 保留到下一个目标执行；本 ADR 的 D9
是本轮对工厂方向确立的唯一约束。
