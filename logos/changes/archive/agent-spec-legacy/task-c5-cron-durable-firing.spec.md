spec: task
name: "Cron 持久化 Firing 与跨 Pod 接管（cron-durable-firing）"
tags: [cron, persistence, cluster, drill, take-over]
estimate: 1w
---

- **task**: c5-cron-durable-firing
- **status**: proposed
- **phase**: P5（生产演练；依赖 c2/c3 已落地、octos-store::CronScheduleStore 已实现）
- **ADR**: docs/adr/cluster-state-and-execution.md（D9 Cron firing 唯一来源 + 接管语义）
- **验收矩阵**: K10（Cron firing 唯一来源；durable）、K17（Pod 强杀后由幸存 pod 接管 cron）

## Intent

`octos-bus::cron_service` 当前以 in-memory + 本地 JSON 文件持久化（`persist_store_locked`）实现。
该形态在三 Pod 集群下出现「双源 firing」「重启丢未触发任务」「Cron
列表不一致」三种故障，违反 D9 「firing 必须只有单一来源」的契约。

本任务交付：
1. 把 `cron_service` 的「job 状态、fire 时间、租约、抑制」全部迁到
   `octos-store::CronScheduleStore` PG backend（在 `feature = "postgres"` 下启用）。
2. fire 路径写入 `cron_fires`（owned table，已在 `0004_c5_cron_durable.sql`
   中定义 schema）作为权威 firing 记录；in-process scheduler 仅消费该表。
3. 跨 Pod lease：cron job 调度权由 lease 控制，过期即由 `tick_takeover`
   风格的 cron-takeover 接管。

退出条件：
- RED→GREEN 集成测试：两个独立 `CronService`（两个 store 连接到同一
  PG schema）验证「一个 firing，两个 pod 仅一方能 ack / 仅一方能 advance
  lease」（K10 + K17）。
- `cargo test -p octos-bus --features postgres` 全绿。
- `cargo clippy -p octos-bus --features postgres --all-targets -- -D warnings` 0 错误。

## Decisions

1. **`CronService` 持有一个 trait 对象 `Box<dyn CronScheduleStore>`**：
   本地开发默认 `LocalCronStore`（保留现有 JSON 文件行为，作为降级
   backend），`postgres` feature 启用时 `PgCronStore` 适配到 PG。
2. **`fire_once` 路径写 `cron_fires` 表**（owner: scope，含 `job_id` /
   `fire_at_ms` / `claimed_by` / `claimed_until_ms`）。多次 fire 通过唯一
   `(job_id, fire_at_ms)` 约束保证幂等。
3. **Lease 字段加进 `cron_jobs` 表**（`lease_owner`, `lease_epoch`,
   `lease_expires_ms`，owner: scope）。takeover 由 `serve_cluster` 已在
   c3 中实现的 `tick_takeover` 模式驱动 cron 专用 `tick_takeover_cron`。
4. **in-memory scheduler 不再是权威**：仅作为本地「拉取 → 触发 →
   标记 firing」的薄壳。

## Constraints

1. RED: `octos-bus/tests/cron_postgres.rs`（新文件），三个测试。
2. GREEN：实现 `PgCronStore` 适配 `CronScheduleStore`（在
   `octos-store` crate 下），加 `cron_fires` / `cron_jobs` 列。
3. `octos-bus::cron_service::fire_once` 改为先 `store.claim_fire(...)`
   再派发 inbound；claim 失败则跳过（其他 pod 已 claim）。
4. `serve_cluster::tick_takeover_cron`（postgres feature-gated）。
5. 移除 `persist_store_locked` 在非 fallback 路径上的调用；保留
   `LocalCronStore` 作为本地开发降级路径。

## Acceptance Criteria

### Rule: cron-firing-uniqueness — 同一 fire 时刻只有一 Pod ack

Scenario: 两个 cron 实例同时 fire 同一 job（critical）
  Tags: critical, K10, K17
  Test:
    Package: octos-bus
    Filter: pg_cron_durable_fires_only_one_pod_acks
  Given 两个 CronService 实例连接到同一 PG schema，job fire 时间 = now
  When 两实例同时调用 fire_once
  Then 恰好一个 firing 记录（唯一约束决定 winner）；另一实例 skip

Scenario: 接管 Pod 接管过期 cron lease
  Tags: critical, K17
  Test:
    Package: octos-bus
    Filter: pg_cron_takeover_after_lease_expires
  Given pod_a 持有 cron lease 但已被 SIGKILL
  When 幸存 pod_b 调用 tick_takeover_cron
  Then pod_b 拿到 lease（epoch+1）并接管下一个 fire 时间点

Scenario: 同一 fire 时刻插入两次被唯一约束拒绝
  Tags: K10
  Test:
    Package: octos-bus
    Filter: pg_cron_idempotent_fire_at_unique_constraint
  Given 一个 cron_fires 行（job_id=J, fire_at_ms=T）已存在
  When 再次以同 (J, T) 插入 cron_fires
  Then 数据库因唯一约束拒绝；error code 23505

## Out of Scope

- in-memory scheduler 移除会破坏单实例本地开发流；保留
  `LocalCronStore` + feature flag 兜底。
- `cron_fires` 表高频写入需做 index：`CREATE INDEX ... ON cron_fires (job_id, fire_at_ms DESC)`。
- 跨时钟漂移：lease 时间用 `SystemTime::now()`，与 cluster 一致；不再依赖本地时钟。