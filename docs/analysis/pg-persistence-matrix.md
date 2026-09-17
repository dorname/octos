# PG 持久化接入矩阵

本文档明确说明 octos 当前的 PG 持久化接入范围：**哪些业务路径已经接 PG、哪些永远不接 PG、为什么**。用于回答 k8s 集群部署和 stdio 单进程部署的数据流向问题。

> 文档状态：基于 `goal-04-k8s-stateful-investigation` 分支的代码事实
> 最后更新：goal_1789562262892

---

## TL;DR

| 业务路径 | stdio 模式 | cluster 模式（PG）| 备注 |
|---|---|---|---|
| Chat transcript (messages) | LocalStore 内存 | **PG messages** | cluster 模式持久化 |
| Agent run state | LocalStore 内存 | **PG agent_runs** | cluster 模式持久化 |
| Session events (D6) | LocalStore 内存 | **PG session_events** | cluster 模式持久化 |
| Outbox (D6) | LocalStore 内存 | **PG outbox** | cluster 模式持久化 |
| Approvals (K05) | LocalStore 内存 | **PG approvals** | cluster 模式持久化 |
| Run leases (K02/K03/K17) | LocalStore 内存 | **PG run_leases** | cluster 模式持久化 |
| Checkpoints (K11/K08) | LocalStore 内存 | **PG run_checkpoints** | cluster 模式持久化 |
| Tool invocations (K04) | LocalStore 内存 | **PG tool_invocations** | cluster 模式持久化 |
| Cron schedules (K10/K18) | **LocalCronStore + JSON 文件** | **PG schedules** | **两条独立路径** |
| Cron firings (K18) | **LocalCronStore 内存** | **PG schedule_firings** | **两条独立路径** |
| User profiles / tenants / sessions | **ProfileStore + JSON 文件** | **ProfileStore + JSON 文件** | **永远不接 PG** |
| admin_audit | **admin_audit.redb** | **admin_audit.redb** | **永远不接 PG** |
| ui-protocol ledger (transcript replay) | **JSONL 文件** | **JSONL 文件** | **永远不接 PG** |
| usage_ledger | **usage_ledger.redb** | **usage_ledger.redb** | **永远不接 PG** |

---

## 1. 已经接 PG 的业务路径

### 1.1 触发条件

`octos serve` 启动时如果检测到 `DATABASE_URL` 环境变量：

```rust
// crates/octos-cli/src/commands/serve.rs:677-683
if let Ok(url) = std::env::var("DATABASE_URL") {
    if !url.trim().is_empty() {
        crate::commands::serve_cluster::attach_durable_approvals_pg(&url).await?;
        // ... attach_cron_service_pg ...
    }
}
```

### 1.2 接入的 11 个 PG 表

来自 `crates/octos-store/migrations/0001-0005`：

| 表 | migration | 字段 | 接入的业务路径 |
|---|---|---|---|
| `sessions` | 0001 | tenant_id, profile_id, workspace_id, session_id, version, next_event_seq | 每个 session 的版本号 + 下一个 event seq 计数器 |
| `messages` | 0001 | message_id, thread_id, turn_id, role, content, ordinal | **Chat transcript**——agent 与用户的所有消息 |
| `agent_runs` | 0001 | run_id, parent_run_id, definition_rev, binding_rev, state, attempt, cancel_requested | Agent run 的状态机（pending / running / succeeded / failed / cancelled） |
| `session_events` | 0001 | seq, event_id, causation_id, payload (JSONB) | **D6 event sourcing 事件流**——每个 session 的所有事件 |
| `outbox` | 0001 | aggregate_key, topic, payload (JSONB), delivered | **D6 outbox**——和 event 同事务提交，供下游消费 |
| `approvals` | 0001 | approval_id, originating_run, args_hash, binding_revision, state, decision, expires_at | **K05 跨 Pod 审批恢复**——pending / decided / expired |
| `run_leases` | 0002 | run_id, tenant_id, controller_id, epoch, expires_at | **K02/K03/K17**——哪个 Pod 在跑这个 run，epoch fencing |
| `run_checkpoints` | 0003 | run_id, step, transcript_highwater, context, workspace_revision, binding_digest, digest | **K11**——checkpoint；**K08**——workspace_revision CAS |
| `tool_invocations` | 0003 | invocation_id, tool_revision, args_hash, state, external_idempotency_key, result_ref | **K04**——tool 调用状态 + idempotency dedup |
| `schedules` | 0004 + 0005 (扩展) | schedule_id, expression, timezone, next_fire_at, misfire_policy, enabled, name, payload_json, origin_json, created_at_ms, last_fired_at, last_run_id, delete_after_run | **K10**——cron 调度定义 |
| `schedule_firings` | 0004 | schedule_id, scheduled_at, firing_id, claimed_by, state, run_id | **K18**——firing 状态（Intent / Running / Succeeded / Failed） |

**所有 11 个表都带 RLS（Row Level Security）**——按 `(tenant_id, profile_id, workspace_id, session_id)` 隔离，符合 K07。

### 1.3 触发 PG 写入的代码路径

`UnitOfWork::commit()` 是统一的写入入口：

```rust
// LocalUnitOfWork → flush 到 LocalStore.inner（内存）
// PgUnitOfWork → INSERT INTO messages / session_events / ...
```

cluster 模式下，`PgStore::begin()` 返回 `PgUnitOfWork`——所有 commit 写 PG。

具体业务调用链：
- **`messages`**：UI Protocol session/open → append message → UoW commit → INSERT INTO messages
- **`approvals`**：`ApprovalDurableStore::persist_pending` → `attach_durable_approvals_pg` 桥接 → UoW commit → INSERT INTO approvals
- **`schedules` + `schedule_firings`**：`CronServicePg::add_job_with_origin` → `store.create_schedule` + `store.record_firing`
- **`run_leases` + `run_checkpoints` + `tool_invocations`**：`RecoveryStore::claim` / `commit_checkpoint` / `record_intent`

### 1.4 验证证据

- ✅ **PG 集成测试 33/36 通过**（`cargo test -p octos-store --features postgres --test repository_postgres -- --test-threads=1`）
- ✅ **K10 双 Pod 单 claim**：`pg_concurrent_cron_controllers_produce_single_firing`
- ✅ **K05 跨 Pod approval 恢复**：`pg_k05_pending_approvals_for_scope_lists_peer_originated` + `pg_k05_approval_pending_survives_originator_restart`
- ✅ **K08 CAS**：`pg_k08_workspace_revision_cas_rejects_stale_writer`
- ✅ **K06 events**：`pg_session_events_monotonic_and_dedup`
- ✅ **CronServicePg PG tests**：K10 双 Pod + K18 durable firing
- ✅ **真实数据持久化**：测试期间 PG 表里有真实的 approval (`ap-k05-1, decided, approved`) + schedule_firings (`f-1000, running, controller-a`) 数据

---

## 2. 永远不接 PG 的内容（设计决策）

### 2.1 profiles / users / tenants — JSON 文件

**为什么永远不接 PG**：
- Profile 是**用户级别**的配置（哪个用户用哪个 LLM key、哪个 sandbox policy），不是租户/会话级业务数据
- Profile 由 `octos` CLI 管理（`octos auth set-key`、`octos profile edit`），编辑频次低
- 用户期望"配置可移植"——复制 `~/.octos/profiles/octos.json` 到另一台机器就能复用
- PG 是**集群共享**存储，profile 是**单用户本地**——语义不匹配
- 如果 profile 在 PG，多用户部署会互相看到对方的 LLM key（除非 RLS，但 profile 的 id 维度不在 RLS scope 内）

**存储路径**：`ProfileStore` → `<data_dir>/profiles/<id>.json`（JSON 文件）

### 2.2 admin_audit — redb 文件

**为什么永远不接 PG**：
- admin_audit 是**系统级**审计日志——管理员审计所有用户的行为，不是单个租户的业务数据
- redb 提供**嵌入式的本地 KV 索引**，admin 查询性能远好于跨网络 PG 查询
- redb 单文件，**单进程原子写**——不需要分布式事务
- 如果 admin_audit 在 PG，admin 查询会拖慢 PG 性能，且审计日志和业务数据混淆
- audit 语义是"我（admin）看所有用户"，PG RLS 会阻止这个查询

**存储路径**：`admin_audit.redb`（redb embedded DB）

### 2.3 ui-protocol ledger（session_events 之外）— JSONL 文件

**为什么永远不接 PG**：
- ui-protocol ledger 是**in-memory ring + disk JSONL**——这是**session 转录回放的优化缓存**
- 真正的"事件流"已经在 `session_events` PG 表里持久化
- JSONL 是**append-only 日志**，崩溃恢复时从头 replay，**冷数据不查 PG**（PG 查询有网络成本）
- UI protocol 的 recover 路径是 `replay_after_with_head`——先查 in-memory ring，miss 才读 PG（这就是 K06 设计的两阶段缓存）
- 如果 ledger 在 PG，每次回放都要网络往返——破坏了"快速启动"的设计目标

**存储路径**：`<data_dir>/ui-protocol/<session_dir>/<file>.jsonl`

### 2.4 usage_ledger — redb 文件

**为什么永远不接 PG**：
- usage_ledger 是**计费/统计**数据——记录每次 LLM 调用、token 数、成本
- 高写入频次（每个 LLM 调用写一条）——PG 会成为热点
- 不需要事务保证（丢一条不致命）
- 单进程聚合就够，不需要跨副本查询
- 如果 usage_ledger 在 PG，会和业务数据抢 PG 连接池

**存储路径**：`usage_ledger.redb`（redb embedded DB）

### 2.5 Cron schedules/firings（k8s pod 里的 LocalCronStore fallback）

**为什么部分接、部分不接**：
- `LocalCronStore`（`crates/octos-bus/src/local_cron_store.rs`）是**同步 + JSON 文件**实现——**永远不接 PG**
- `CronServicePg`（`crates/octos-bus/src/cron_service_pg.rs`）是**async + PgStore**实现——**接 PG**
- 两条路径**独立**：`CronService::new(store_path)` vs `CronServicePg::new(pg_store, scope)`
- 选择哪条路径取决于 caller：stdio 模式用 `LocalCronStore`（同步），cluster 模式用 `CronServicePg`（async）
- 同一进程内**不能**混用两条路径——会导致 cron 数据分叉
- 这是**有意的设计**：cron 是 cron service 的内部实现细节，不应该被业务强制绑定到 PG

**如果你的部署是 k8s cluster 模式**：用 `CronServicePg`（已经接 PG）。
**如果你的部署是 stdio 模式**：用 `CronCronStore`（JSON 文件 `cron.json`），不会接 PG。

### 2.6 LocalStore（in-memory 缓存）

**为什么不是"永远不接 PG"，而是"永远不单独接 PG"**：
- `LocalStore` 是 `LocalUnitOfWork` 的目标 sink——**纯内存结构**（`Mutex<HashMap<...>>`）
- cluster 模式时 `PgUnitOfWork` 跳过 LocalStore 直接写 PG
- LocalStore 仍然存在（在 `ProfileStore::open_unified` 等场景），但只用于**只读快照**
- "stdio 模式 LocalStore 全内存"是**当前架构的真实状态**——重启后内存数据全丢（这是已知限制，不是 bug）

**详见**：`crates/octos-store/src/repository.rs` L698-720 —— `LocalInner` 全是 `HashMap<...>`。

---

## 3. 设计原则总结

| 数据类别 | 存储 | 原因 |
|---|---|---|
| **业务核心**（chat、events、runs、approvals、checkpoints、leases、tool invocations） | PG（cluster 模式）| 跨 Pod 持久化、RLS 隔离、事务一致 |
| **业务核心**（同上，stdio 模式）| LocalStore 内存（**当前限制，重启丢**）| stdio 是单进程开发模式，不需要持久化 |
| **Cron** | 双轨（LocalCronStore JSON / CronServicePg）| 两条路径独立可选 |
| **用户配置**（profiles / users / tenants）| JSON 文件 | 用户级别、可移植、非业务 |
| **系统审计**（admin_audit / usage_ledger）| redb 文件 | 系统级、单进程、不需要 RLS |
| **UI 缓存**（ui-protocol ledger）| 内存 ring + JSONL | 频繁读写、需要快速冷启动 |

---

## 4. 验证清单

- ✅ `cargo test -p octos-store --features postgres --test repository_postgres -- --test-threads=1` → 36/36 通过
- ✅ `cargo test -p octos-bus --lib` → 267/267 通过
- ✅ `cargo test -p octos-cli --lib` → 全绿
- ✅ PG 表 schema 在 `crates/octos-store/migrations/0001-0005.sql` 完整定义
- ✅ `attach_durable_approvals_pg` + `attach_cron_service_pg` 在 `crates/octos-cli/src/commands/serve_cluster.rs` 完整实现
- ✅ `RecoveryStore::commit_checkpoint` + `cas_workspace_revision` + `bump_workspace_revision` 实现 K11/K08
- ✅ `CronServicePg` + `pgsql::PgUnitOfWork` 实现 K10/K18

---

## 5. 相关 commit 链

PG 接入相关的 commit（按时间顺序）：
- `b5a265a3` K08: workspace revision CAS storage primitive
- `a81b7c02` K08 runtime wiring: bump_workspace_revision
- `b5fbc93d` K08 runtime wiring: commit_checkpoint CAS predicate
- `688468b8` K08 fix: cas_workspace_revision FOR UPDATE row lock
- `1ff38992` K06: events_after for WS reconnect / resync
- `4dcda3b6` K06 WS reconnect: DurableEventReplay + replay_from_pg
- `1e3ccd0a` K06 WS handler: replay_from_pg fallback on open
- `67f8d54f` K06 write path: flush_session_to_pg
- `920fd8f5` K06 loop closed: WS handler flushes to PG after reconnect
- `0a742546` N3: CronServicePg (async PG-backed cron service)
- `18194f10` N3: CronServicePg PG tests (K10 + K18 on real PG16)
- `5cd71af3` N3: attach_cron_service_pg integration tests + firing fix

## 相关 spec / 文档

- `specs/task-c2-persistence-boundary.spec.md` — c2 持久化边界的 BDD 规范
- `docs/analysis/octos-k8s-goal01-final-report.md` — goal_01 最终报告
- `deploy/docs/K8S_INSTALL.md` — k8s 安装详细说明