spec: task
name: "task-pg-persistence-matrix"
tags: [deploy, k8s, persistence, pg, documentation, bug-fix]
---

## Intent

Two deliverables for goal_1789562262892:

1. **PG 持久化接入矩阵文档**（`docs/analysis/pg-persistence-matrix.md`）：
   - 哪些业务路径**已经接 PG**
   - 哪些内容**永远不接 PG**，以及**原因**
   - 让运维/开发者能在一份文档里搞清楚"stdio vs cluster 模式下数据流到哪里"

2. **修复 k8s cluster 部署 manifest**（`deploy/k8s/03-cluster-with-config.yaml`）：
   - 让 `init-config.sh` 真正创建 ConfigMap/profile/PG 数据目录
   - 让 `initContainer` 在 octos serve 启动**之前**跑完初始化
   - 修掉之前的 hostPath / subPath / binary 路径 bug

提供统一的 `deploy/` 目录，包含 octos 的所有部署和安装文件
（k8s manifests、docker-compose、shell 脚本、详细文档）。消除
仓库根目录散落 yaml 文件、脚本硬编码路径等不利于新运维
上手的问题。

## Decisions

D1: The PG persistence matrix is the single source of truth for
    "where does my data go?" It covers 11 PG tables and 4 categories
    of "never PG" (profiles / audit / UI cache / cron LocalCronStore).

D2: The cluster-mode k8s manifest uses an initContainer pattern
    instead of baking init into the entrypoint. Rationale:
    - k8s lifecycle hooks (CrashLoopBackOff backoff, pod restart
      semantics) treat init-container failure separately from the
      main container.
    - The init container can run to completion BEFORE octos serve
      starts, so the serve process sees pre-built config.

D3: PG migrations run LAZILY (K06 design). `attach_durable_approvals_pg`
    and `attach_cron_service_pg` call `store.migrate()` on first DB op.
    The init script does NOT run `octos migrate` because that
    subcommand does not exist. The init script documents this behavior.

D4: The binary is mounted at `/opt/octos/octos` (a file, via subPath).
    The init script is mounted at `/usr/local/bin/init-config.sh`.
    `/tmp/octos-data` and `/workspace` are emptyDir volumes shared
    between initContainer and main container.

## Boundaries

### Allowed Changes

- `docs/analysis/pg-persistence-matrix.md` (new file).
- `deploy/k8s/03-cluster-with-config.yaml` (existing file, restructure).
- `deploy/scripts/deploy-k8s.sh` (existing file, comments).
- `specs/task-pg-persistence-matrix.spec.md` (new spec).
- Test functions under `crates/octos-bus/tests/` referencing the new spec.

### Forbidden

- Do NOT change the runtime code in `crates/` (no Rust changes).
- Do NOT add a `migrate` subcommand to octos CLI (out of scope; lazy
  migration is the documented design).
- Do NOT move `Dockerfile` or change its build steps.
- Do NOT change `attach_durable_approvals_pg` or `attach_cron_service_pg`
  signatures.

## Completion Criteria

Rule: matrix — PG persistence boundary is documented

Scenario: matrix doc covers all 11 PG tables
  Test:
    Package: octos-bus
    Filter: test_pg_matrix_doc_covers_all_tables
  Given `docs/analysis/pg-persistence-matrix.md`
  When the operator searches for each PG table name
  Then all of `sessions`, `messages`, `agent_runs`, `session_events`,
    `outbox`, `approvals`, `run_leases`, `run_checkpoints`,
    `tool_invocations`, `schedules`, `schedule_firings` are mentioned
    with field lists

Scenario: matrix doc explains "never PG" categories
  Test:
    Package: octos-bus
    Filter: test_pg_matrix_doc_never_pg_categories
  Given `docs/analysis/pg-persistence-matrix.md`
  When the operator reads the "永远不接 PG" section
  Then profiles/users, admin_audit, ui-protocol ledger, usage_ledger
    are all covered with reasons

Rule: manifest — k8s cluster-mode deploys

Scenario: manifest uses initContainer for setup
  Test:
    Package: octos-bus
    Filter: test_k8s_manifest_uses_init_container
  Given `deploy/k8s/03-cluster-with-config.yaml`
  When the operator inspects the octos Deployment
  Then there is at least one `initContainers` entry
  And the init container's command is `/usr/local/bin/init-config.sh`

Scenario: manifest does not reference non-existent migrate subcommand
  Test:
    Package: octos-bus
    Filter: test_k8s_manifest_no_migrate_subcommand
  Given `deploy/k8s/03-cluster-with-config.yaml`
  When the operator greps for `octos migrate`
  Then no such line exists (the init script does NOT call migrate)

Scenario: binary mount path is consistent
  Test:
    Package: octos-bus
    Filter: test_k8s_manifest_binary_path_consistent
  Given `deploy/k8s/03-cluster-with-config.yaml`
  When the operator inspects volume mounts and container command
  Then `command: ["/opt/octos/octos"]` matches the binary mountPath
    `/opt/octos/octos`

Rule: yaml — manifest is syntactically valid

Scenario: manifest passes YAML parse
  Test:
    Package: octos-bus
    Filter: test_k8s_manifest_yaml_parse
  Given `deploy/k8s/03-cluster-with-config.yaml`
  When the operator runs `python3 -c "import yaml; yaml.safe_load(open(...))"`
  Then it returns 8 documents (Namespace, Deployment pg, Service pg,
    ConfigMap, Secret, ConfigMap init, Deployment octos, Service octos)
    with no parse errors

场景: matrix 文档覆盖所有 11 个 PG 表
  测试:
    包: octos-bus
    过滤: test_pg_matrix_doc_covers_all_tables
  假设 `docs/analysis/pg-persistence-matrix.md`
  当 运维搜索每个 PG 表名
  那么 所有 11 个表都被提到并列出字段

场景: matrix 文档解释"永远不接 PG"类别
  测试:
    包: octos-bus
    过滤: test_pg_matrix_doc_never_pg_categories
  假设 `docs/analysis/pg-persistence-matrix.md`
  当 运维阅读"永远不接 PG"章节
  那么 profiles/users、admin_audit、ui-protocol ledger、usage_ledger
    都被覆盖并给出原因

场景: manifest 使用 initContainer
  测试:
    包: octos-bus
    过滤: test_k8s_manifest_uses_init_container
  假设 `deploy/k8s/03-cluster-with-config.yaml`
  当 运维检查 octos Deployment
  那么 至少有一个 `initContainers` 条目
  并且 init 容器的 command 是 `/usr/local/bin/init-config.sh`

场景: manifest 不引用不存在的 migrate subcommand
  测试:
    包: octos-bus
    过滤: test_k8s_manifest_no_migrate_subcommand
  假设 `deploy/k8s/03-cluster-with-config.yaml`
  当 运维 grep `octos migrate`
  那么 不存在这样的行（init 脚本不调 migrate）

场景: binary 挂载路径一致
  测试:
    包: octos-bus
    过滤: test_k8s_manifest_binary_path_consistent
  假设 `deploy/k8s/03-cluster-with-config.yaml`
  当 运维检查 volume mounts 和 container command
  那么 `command: ["/opt/octos/octos"]` 与 binary mountPath `/opt/octos/octos` 匹配

场景: manifest 通过 YAML 解析
  测试:
    包: octos-bus
    过滤: test_k8s_manifest_yaml_parse
  假设 `deploy/k8s/03-cluster-with-config.yaml`
  当 运维运行 YAML 解析
  那么 解析成功，无错误

## Out of Scope

- Implementing the `migrate` subcommand in octos CLI.
- Changing the lazy-migration design (K06) to eager migrations.
- Multi-region / multi-cluster replication.
- Adding a real image registry push automation.
- 不在本任务范围内：
  - 在 octos CLI 实现 migrate subcommand
  - 改 K06 的惰性 migration 设计为 eager
  - 多区域/多 cluster 复制
  - 添加真实 image registry 推送自动化