spec: task
name: "数据迁移与生产演练：导出导入/故障接管/回滚/验收压测（production-drill-migration）"
tags: [migration, cluster, drill, rollback, cron, acceptance]
estimate: 2w
---

- **task**: c5-production-drill-migration
- **status**: proposed
- **phase**: P5（生产演练；依赖 c1–c3 已落地）
- **ADR**: docs/adr/cluster-state-and-execution.md（Consequences：迁移与回滚策略）
- **验收矩阵**: K10（Cron firing 唯一）、K15（升级期存储失败）、K16（备份恢复）、K17（Pod 强杀接管）

## Intent

c1–c3 落地后，存量单实例部署的消息 JSONL、redb 记忆、审批、任务、
workspace 需要可验证地迁入 PG/对象存储；集群形态需要真实故障注入演练
证明恢复边界成立。禁止复制运行中的 redb 文件充当一致备份；禁止本地
文件与 PG 无事务长期双写；新集群接受写入后不能直接把流量切回旧文件
副本。

本任务交付：按 profile 的短暂停写迁移工具与核对流程、Cron durable
firing、故障接管演练（强杀/SIGTERM 超时/节点删除）、备份恢复演练、
灰度与回滚规程。退出条件：满足 SLO/验收清单；完成一次故障接管和一次
完整回滚演练。

## Decisions

1. **按 profile 短暂停写迁移**：选择小批 profile → 停止新输入与 Cron
   派发 → 等安全 checkpoint → 冻结写入 → 一致性导出（JSONL 含 meta/
   thread/控制记录/fork/rollback/去重标记；redb；审批；任务；
   workspace）→ 导入 PG/对象存储并建立 legacy scope 映射 → 核对计数/
   摘要/顺序/预算/产物 → 切换 profile 路由与 storage generation →
   集群侧成为唯一写者。
2. **一致导出只走支持的口径**：redb 必须停写关闭后复制或使用一致的
   导出接口；禁止直接复制打开中的文件。JSONL 导入不可按「每行都是
   Message」简化。
3. **legacy scope 映射**：保留旧 wire session ID，建立
   （旧实例、profile、规范化存储作用域）→ 新 Scope 映射；session ID
   不假设全局唯一。未完成任务/待审批/无法迁移的旧进程逐个归类：
   可从检查点恢复、等待旧环境排空、或标记人工处理。
4. **回滚用兼容版本前向处理**：需要回滚时冻结新写、导出集群增量并
   验证逆向兼容；旧版本能表示全部新状态时才逆向导入切回，否则保留
   新存储并回滚到仍能读新 schema 的兼容应用版本（expand/contract）。
5. **Cron durable firing**：`schedules`/`schedule_firings` 落库，
   `UNIQUE(schedule_id, scheduled_at)`；两个 Cron Controller 同时扫到
   同一时间点只产生一个 firing 与一个 run；misfire 按既定策略补跑。
6. **演练以真实多 Pod 故障注入为准**：不能仅凭 mock SQL 的单元测试
   宣称分布式恢复已验证；也不需要每次文档调整运行整个 workspace。
7. **指标与运行手册同交付**：容量测算方法、恢复边界、故障语义
   （DB/对象存储/Registry/MCP/可用区不可用时的行为）写入运行手册；
   恢复指标是验收目标，须实测记录。

## Boundaries

### Allowed Changes

- specs/task-c5-production-drill-migration.spec.md
- crates/octos-cli/src/commands/（迁移导出/导入/核对子命令）
- crates/octos-store/migrations/ 与迁移工具
- crates/octos-bus/src/（cron_service durable firing、渠道 cursor/去重）
- deploy/ 或 packaging/（灰度/回滚运行手册、演练脚本）
- crates/octos-cli/tests/、e2e/（故障注入与回滚演练测试）

### Forbidden

- 不实现插件工厂灰度发布（工厂 Binding 的发布/回滚归下一目标）
- 不做跨地域多主
- 不做不停机双写迁移（需要时另立独立复杂度的任务）
- 不删既有测试断言换绿；不 push/PR

## Acceptance Criteria

### Rule: cron-dedup — Cron firing 集群唯一

Scenario: 两个 Controller 同时扫到同一时间点（critical）
  Tags: critical, K10
  Test:
    Package: octos-store
    Filter: concurrent_cron_controllers_produce_single_firing
  PG parity:
    Package: octos-store
    Filter: pg_concurrent_cron_controllers_produce_single_firing
    Schema: real PG (k10 cron PG schema)
  K18 cross-pod:
    Package: octos-store
    Filter: pg_k18_cron_durable_fires_only_one_pod_acks
  Given durable schedule 到达 fire 时间
  When 两个 Cron Controller 并发扫描
  Then 恰好一个 firing 记录与一个 run；另一实例因唯一约束让位

Scenario: misfire 按策略补跑
  Test:
    Package: octos-store
    Filter: cron_misfire_policy_backfills_once
  Given Controller 停机错过 fire 时间
  When 恢复后扫描
  Then 按既定 misfire 策略补跑一次，不重复不遗漏

### Rule: migration-fidelity — 迁移完整可核对

Scenario: 迁移核对计数/摘要/顺序/预算/产物（critical）
  Tags: critical
  Test:
    Package: octos-cli
    Filter: migration_audit_matches_counts_digests_order_budget
  Given 一个含消息/控制记录/fork/审批/任务/workspace 的 profile
  When 停写导出并导入 PG/对象存储
  Then 计数、内容摘要、canonical 顺序、预算账目与产物引用全部一致；
       legacy scope 映射可回查旧 wire session

Scenario: 导入拒绝简化 JSONL
  Test:
    Package: octos-cli
    Filter: importer_rejects_message_only_jsonl_simplification
  Given 含 meta/控制记录/rollback 行的 JSONL
  When 导入
  Then 控制语义完整保留；把每行当 Message 的输入被拒或明确报错

Scenario: 回滚路径双分支（critical）
  Tags: critical
  Test:
    Package: octos-cli
    Filter: rollback_prefers_forward_compatible_app_version
  Given 新集群已接受写入且需回滚
  When 旧版本不能表示全部新状态
  Then 保留新存储并回滚到兼容应用版本；能表示时逆向导入切回且
       已接受写入全部保留

### Rule: storage-failure-during-upgrade — 升级期存储失败语义

Scenario: DB/对象存储失败不产生 phantom completion（critical）
  Tags: critical, K15
  Test:
    Package: octos-cli
    Filter: storage_failure_mid_upgrade_no_phantom_completion
  Given 升级过程中数据库或对象存储不可用
  When 写入失败
  Then 不发布未落盘的成功；恢复后可续执行；orphan 对象异步 GC

### Rule: backup-restore — 备份恢复可用

Scenario: 从备份恢复 PG+对象存储后可打开历史（critical）
  Tags: critical, K16
  Test:
    Package: octos-cli
    Filter: restored_backup_opens_history_and_artifacts
  Given 从备份恢复的 PG 与对象存储
  When 打开历史会话、恢复样本 run、下载产物
  Then 全部可用；引用完整性校验通过
  Level: integration

### Rule: pod-kill-recovery — 任意 Pod 强杀满足恢复边界

Scenario: 强杀/SIGTERM 超时/节点删除（critical）
  Tags: critical, K17
  Test:
    Package: octos-store
    Filter: pg_k17_pod_failover_drill_takeover_recovery_audit
  Given 运行中的 API/Worker/Scheduler Pod（PG 后端）
  When pod A SIGKILL、pod B 调用 tick_takeover
  Then lease + checkpoint 接管；pinned binding (K11) 保留；audit 跨 pod 一致
  Level: integration
  Test Double: 真实 PG16 docker (real cluster topology)

  K09 跨任务 join-once:
    Package: octos-cli
    Filter: duplicate_child_terminal_joins_parent_once

  K16 端到端 dump→restore:
    Package: octos-store
    Filter: pg_k16_dump_restore_round_trip_on_real_pg
