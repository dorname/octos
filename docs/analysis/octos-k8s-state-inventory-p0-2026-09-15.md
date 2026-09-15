# P0 状态/调用路径盘点：无状态化改造基线对照

盘点日期：2026-09-15。源码基线：`main @ 6cfc689e`，workspace `2.0.3-rc.11`。

本文是 P0「基线与契约」阶段的状态外置对照清单：左侧为当前源码中的状态
真相落点（逐一经源码路径核实），右侧为目标真相源（依据
[改造方案](octos-k8s-plugin-factory-plan-2026-09-14.md) §3.3 与
[ADR](../adr/cluster-state-and-execution.md) D1）。改造切片按 spec 推进：
c1 Scope → c2 持久化边界 → c3 可恢复执行 → c5 迁移演练；插件工厂（P4）
相关状态（包版本/Binding/Registry 引用）不在本清单的迁移范围，随工厂
目标另行盘点。

| 状态 | 当前真相落点（源码证据） | 目标真相源 | 迁移/一致性规则 | 归属 spec |
|---|---|---|---|---|
| 用户、租户、profile | `octos-store/src/user_store.rs`、`profiles.rs`（文件/单机语义） | PostgreSQL | tenant/profile 归属明确，配置不可变 revision | c2 |
| 登录会话、OTP、撤销 | `octos-store/src/admin_token_store.rs`、`login_allowlist.rs`、`api/otp.rs` | PostgreSQL / 受控 Redis TTL | OTP 校验/次数/消费原子化；撤销跨 Pod 生效 | c2 |
| Canonical 消息/线程/控制记录 | `runtime/session.rs`、`runtime/profile.rs` 的 JSONL 账本（含 meta/thread/fork/rollback/去重标记） | PostgreSQL | 保留 message/thread/turn ID、顺序、fork/rollback、原时间戳；导入不可按每行皆 Message 简化 | c2 → c5 迁移 |
| OUP 事件、snapshot、cursor | `api/ui_protocol_ledger.rs`、`api/events.rs`、`api/ui_protocol_transport.rs`（本地 broadcast 为唯一通道） | PG seq/outbox/replay + 老事件归档对象存储 | 按完整 Scope 单调 seq；持久化后推送；broadcast 降级为加速层 | c1（UPCR）→ c2 |
| ContextManager/compaction/retry state | `api/context_manager.rs`（进程内 + 本地落盘） | PG 元数据 + 大对象 | checkpoint 带 schema/runtime revision；不能只导入纯文本聊天 | c2 → c3 |
| Task/Goal/Supervisor/续执行/预算 | `task_supervisor.rs`、`agent_orchestrator.rs`、`supervisor_store.rs`、`commands/goal.rs`（内存注册表 + 本地账本混合） | PostgreSQL | 状态迁移与对应事件同事务；保留去重与证据摘要 | c3 |
| 审批、提问、人工 gate | `contracts/approvals.rs:17`（`tokio::sync::oneshot` 为唯一事实）、`contracts/questions.rs`、pipeline human gate | PostgreSQL durable waiting | pending/决策/超时/续执行落库；reply CAS；本地 oneshot 降级为 adapter | c2 |
| Cron、渠道 cursor、入站/出站去重 | `cron_service.rs`、gateway adapters（进程内调度） | PostgreSQL | `UNIQUE(schedule_id, scheduled_at)`；渠道 message ID 唯一约束 | c5 |
| 长期记忆与 Episode | `octos-memory/`（`memory_store.rs`、`episode.rs`、`hybrid_search.rs`，redb + MEMORY.md 文件语义） | PostgreSQL；向量检索 pgvector 可选 | episode 全量迁移优先；向量索引异步重建（带模型/维度/version）；HybridSearch 评分需固定样本对照，不得直接宣称 SQL 等价 | c2 → c5 |
| 文档/媒体/工具结果大对象 | 工作区文件系统、`skill-output/` | S3 兼容存储 + PG artifact 引用 | 先上传校验再提交可见引用；孤儿对象异步 GC | c2（引用）→ c3（产物） |
| 可变工作区与 peer 隔离目录 | `peers/`（含 `peers/recovery.rs`）、file mutations、preview | run/workspace 专属 runner + 版本化 snapshot | 外部持久化为恢复依据；本地路径不进入跨节点身份 | c3 |
| Skills/Tools 安装目录 | `plugins/`、SKILL.md 发现目录 | OCI 内容摘要包 + 本地只读缓存 | 安装与调用固定 digest；缓存可删重建 | **下一目标（工厂）** |
| Provider 凭据、MCP refresh token | 配置/keychain、`mcp_auth.rs` | KMS 加密凭据服务/Secret 引用 | 授权主体绑定；分布式 refresh CAS；不写模型上下文 | c2（引用语义）；MCP 适配归工厂目标 |
| 运行指标、日志、链路 | 本地日志 | 集中日志/Prometheus/Tracing | 业务状态不得依赖日志检索恢复 | c5（运行手册） |
| 插件包版本/Binding/Agent 定义 | 无（现状只有本地安装目录与 manifest） | PostgreSQL + OCI Registry | 不可变 digest、依赖 lock、发布/绑定 revision | **下一目标（工厂）** |

## 关键调用路径（改造切入点）

1. **入口**：`octos-cli serve/gateway/chat` → Axum API 与 OUP WS（
   `api/router.rs`、`api/ui_protocol_transport.rs`）→ `AppState` 持有
   profile 映射与 `SessionRuntimeCache`（`runtime/cache.rs:100`）。
   c1 在此处绑定 ExecutionContext。
2. **会话**：`SessionActor` / `InProcessAgentOrchestrator` 串行语义保留
   （ADR D2 决策 6）；canonical 写路径切到 repository（c2）。
3. **执行**：`octos-agent` Agent 循环 / `TaskSupervisor` → LLM / 工具；
   c3 在工具调用前后、审批挂起、turn 完成处落 checkpoint。
4. **审批**：`contracts/approvals.rs` 的 oneshot 通道前加 durable
   waiting 层（c2），续执行命令经 CAS（K05）。
5. **事件**：`ui_protocol_ledger.rs` 事件先落 `session_events`+outbox
   再广播（c2，D6）。

## 实施进度基线（2026-09-15 滚动更新）

已完成并经测试验证的层（不代表集群能力已就绪）：

- **c1**：`octos-core::execution_scope`（Scope/Execution/bind_scope/幂等判定，
  7 测试）；`octos-cli::api::execution_context` 入口绑定（4 测试）；
  `turn/start` accept 接 `run_id`（UPCR-2026-030，915 transport 测试无回归）。
- **c2 存储层**：`octos-store::repository` 契约（StoreView/UnitOfWork，10 契约
  测试）+ local adapter；`repository::postgres` 后端 + tenant 键 schema +
  RLS 迁移（10 个**真实 PG** 集成测试，稳定）。K05 审批 reply CAS 与 K07
  跨租户隔离在存储层已验证。
- **c2 消费侧（审批 durable 贯通）**：`PendingApprovalStore` 可选 durable sink
  （`attach_durable`，durable CAS 优先、oneshot 降级为加速器）；serve 启动在
  `DATABASE_URL` 存在时经 `commands::serve_cluster` 接线 PG 后端（真实运行时
  验证：启动即在 PG `public` schema 建出 6 张 c2 表）。59 contracts 测试通过。
- **c3 存储层三件套**：`run_leases`（epoch fencing，K02 真并发单 owner /
  K03 接管 fencing，真实 PG 验证）、`run_checkpoints`（D3/K11 pinned binding，
  stale-epoch 写 fencing）、`tool_invocations`（K04 幂等账本 + Unknown 对账）。
- **c3 执行/恢复组件**：`octos-agent::tools::idempotent`（`IdempotentToolExecutor`
  K04 运行时语义，9 测试）+ `octos-agent::recovery`（`recover_run` digest/版本
  fail-closed + pinned binding，3 测试）；`octos-cli` 桥接
  `DurableSideEffectLedger`/`DurableCheckpointSource` 接通 durable 后端
  （端到端 K04 跨"重启"复用 + checkpoint-resume 接管恢复，2 测试）。
- **c3 接管 supervisor**：`ClusterRunSupervisor::tick_takeover`（serve_cluster）
  ——对候选 (scope, run_id) 调用 LeaseStore::claim 接管过期租约（K02/K03），
  经 CheckpointSource + recover_run 重建并产出 TakeoverDecision（K11 pinned
  binding + K15 失败 fail-closed），端到端测试覆盖 4 条 c3 验收。
- **c5 存储层 K10**：`schedules`/`schedule_firings` + PRIMARY KEY
  (scope, schedule_id, scheduled_at) 强制 K10 单 firing 单 claim，real-PG
  + local 双后端测试覆盖。

> Baseline commit：`80636697`（`k8s-stateless-p3-baseline` 标签显式标注
> plugin factory deferred）。本目标范围内全部 41 个本目标文件已 commit。

**编排器 K04 接入**：`ToolRegistry::wrap_with_idempotent_ledger` API 已
落地并测试通过——把 side-effect tool 用 `IdempotentToolExecutor` 包装，
由 ledger 驱动 K04 复用/对账语义。生产编排器侧接入仍属剩余工程主体。

**插件工厂显式保留（按目标"插件工厂保留到下一个目标执行"）**：
plan §7.2 P4（Build/Registry/Catalog/Binding/AgentDefinition/binary/MCP
adapters/发布回滚）+ K12–K14（manifest/signing/MCP）+ K18–K20（多 adapter/
限额/包撤销）均未开始；commit message 与 baseline tag 均显式标注
deferred。

未完成的接线（诚实边界，不得当作已完成）：

- **编排器接入**：`IdempotentToolExecutor`/`recover_run`/supervisor 已就绪，
  但 `InProcessAgentOrchestrator`（49013 行）尚未在工具边界实际包装
  side-effect tool、未在接管时调用 `recover_run_from`、未实现 K09 join-once
  接入 TakeoverDecision。这是把组件变为运行时行为的关键剩余工作。
- **c5 cron 完整集成**：`CronScheduleStore` 持久化已就绪，但 `octos-bus
  cron_service.rs` 仍走内存/JSON，未真正改用持久化调度（涉及 octos-bus 引入
  sqlx + 1000+ 行 cron_service 改造）。
- **D6 多副本事件回放**：ledger 已是 disk-commit 是 truth（单节点 D6 成立）；
  多副本 K06 跨副本 seq 回放需要 PG session_events + outbox 接入
  ui_protocol_ledger（大型跨层改动）。
- **c5 迁移/演练**：数据导出导入、Cron 调度集成、灰度回滚、备份恢复（K16）、
  真实多 Pod 故障注入（K17）、WS 断线重放（K06）、workspace 冲突（K08）均未
  开始。

## 不可作为验收证据的事项（诚实边界）

- 本清单是源码盘点，不表示任何集群能力已实现。
- 容量/恢复指标为验收目标（方案 §6.4），未经实测不得宣称达成。
- mock SQL 单元测试不得作为分布式恢复已验证的证据（方案 §7.4）；
  K02/K03/K04/K17 需真实多 Pod 故障注入。
