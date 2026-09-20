# 实现任务

## [delta] 规格变更：S16 k8s 无状态化 + 部署方案

- [x] 产出 delta → `deltas/prd/1-product-requirements/core-01-requirements.md`（P11 + S16 场景总览/详述 + §5.1 约束修订）
- [x] 产出 delta → `deltas/prd/2-product-design/1-feature-specs/core-07-k8s-cluster-design.md`（S16 功能规格，全新）
- [x] 产出 delta → `deltas/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md`（集群模式 serve 交互增量）
- [x] 产出 delta → `deltas/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md`（集群部署架构视图）
- [x] 产出 delta → `deltas/prd/3-technical-plan/2-scenario-implementation/core-00-scenario-overview.md`（收录 S16）
- [x] 产出 delta → `deltas/prd/3-technical-plan/2-scenario-implementation/core-S16-k8s-cluster.md`（S16 时序/流程图，全新）
- [x] 产出 delta → `deltas/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md`（Phase 3-3 部署方案，全新）

## [code]

- [x] 修复 `deploy/` 构建说明：cluster/hostpath 必须 `--features api,postgres`（#2436）
- [x] 修复 `03-cluster-with-config.yaml` init：创建 `profiles/*/data/inbox`（#2437）
- [x] 修复 `config.rs` 中 `impl Config` 被提前闭合导致 musl 构建失败
- [x] 重编 musl binary（`api,postgres`）并滚动本地 k8s Deployment
- [x] 复验：`\dt` 11 表；inbox INFO tick；octoscode/WS `turn/start` 成功

## [deploy] 本地 k8s 验证（verify PASS 且人类确认后）

- [x] 按部署方案在 docker-desktop `octos` ns 完成 smoke 清单
  - SMOKE-S16-01/02/03/04 PASS（`scripts/smoke-s16-k8s.sh`）
  - `openlogos smoke` Gate 3.8 PASS（2026-09-20；磁盘满崩溃后已恢复 binary HTTP@Windows:18088 + Pod recreate）
  - 产物：`SMOKE_PASS` / `smoke-report.md` / `smoke-results.jsonl` / `core-S16-smoke-test-cases.md`
