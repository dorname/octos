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

## [deploy] 本地 k8s 验证（verify PASS 且人类确认后）

- [ ] 按部署方案执行 `./deploy/scripts/deploy-k8s.sh cluster`（或 hostpath）并完成 smoke 清单
