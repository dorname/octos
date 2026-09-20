# 合并指令

## 变更提案
- 提案名称：k8s-cluster-deploy
- 提案目录：logos/changes/k8s-cluster-deploy/

## 提案内容

# 变更提案：k8s-cluster-deploy

> module: core | created: 2026-09-20

## 变更原因

dorname 分支已完成 OpenLogos 基线（S01–S15），但缺少 **Kubernetes 多副本无状态化部署** 的需求/设计/部署方案与流程图。原 `goal-04-k8s-stateful-investigation`（k8s-stateless）已在代码与 `deploy/` 清单侧落地 PostgreSQL 真相源、Scope 绑定、租约恢复、Cron PG 与三种 k8s 部署形态；本变更把该能力正式接入 OpenLogos 文档链路，并在 `dorname-k8s-dev` 上持续演进。关联 ADR：`docs/adr/cluster-state-and-execution.md`；分析基线：`docs/analysis/octos-k8s-plugin-factory-plan-2026-09-14.md`。

## 变更类型

需求级（新增场景 S16 + 部署方案 + 架构/设计增量；代码侧已由 k8s 分支合并进 `dorname-k8s-dev`，本提案以规格对齐为主）

## 变更范围

- 影响的需求文档：`logos/resources/prd/1-product-requirements/core-01-requirements.md`（新增痛点 P11、场景 S16、修订 §5.1 单机模型约束）
- 影响的功能规格：新增 `core-07-k8s-cluster-design.md`；修订 `core-04-serve-api-design.md`（集群模式 serve 交互）
- 影响的业务场景：**新增 S16**（K8s 多副本无状态化部署与故障恢复）；索引更新 `core-00-scenario-overview.md`
- 影响的部署方案：**新增** `prd/3-technical-plan/3-deployment/core-01-deployment-plan.md`（补齐 Phase 3-3）
- 影响的架构/流程图：修订 `core-01-architecture-overview.md`（集群部署视图）；新增 `core-S16-k8s-cluster.md` 时序图
- 影响的 API：无新增公开 REST 契约（集群行为复用 serve UI Protocol / admin；Scope 绑定见 UPCR-030）
- 影响的 DB 表：声明 PG 迁移族 `0001_c2`–`0005_c5`（sessions/events/approvals/leases/checkpoints/cron）为集群模式真相源（skip_phases 含 database，以迁移 SQL + ADR 为权威）
- 影响的编排测试：无（core skip_phases 含 scenario）
- 影响的 smoke 测试：部署方案中定义 SMOKE 输入（后续 test-writer 可落盘）

## 部署影响

- 是否需要部署：是
- 部署原因：S16 以 k8s/docker-desktop 本地集群与后续 staging 验证为验收路径；`deploy/k8s/*` 与 `deploy/scripts/deploy-k8s.sh` 已入库
- 影响环境：本地（docker-desktop/kind）/ 测试
- 是否涉及数据迁移：是（集群模式启用 PG 迁移；本地 JSONL/redb 保留为 local adapter）
- 是否需要回滚预案：是（回退到单副本 local adapter / 上一版 Deployment）
- 是否需要 smoke：是

## 变更概述

在 OpenLogos 文档基线中引入 **S16：K8s 多副本无状态化部署与故障恢复**：明确 PostgreSQL 为业务状态唯一真相源，Scope=`tenant+profile+workspace+session` 为隔离单位，API/Worker/Scheduler 逻辑角色可水平扩缩，Pod 销毁后靠租约+检查点重建执行。同步补齐 Phase 3-3 部署方案（baseline/hostpath/cluster 三种清单），并更新架构图与 S16 时序图，使 dorname 上的 OpenLogos 流程能继续驱动 k8s 迭代，而不再依赖已归档的 agent-spec 路径。


## 需要合并的 Delta 文件

### 1. deltas/prd/1-product-requirements/core-01-requirements.md

- Delta 文件：`logos/changes/k8s-cluster-deploy/deltas/prd/1-product-requirements/core-01-requirements.md`
- 目标目录：`logos/resources/prd/1-product-requirements/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 2. deltas/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md

- Delta 文件：`logos/changes/k8s-cluster-deploy/deltas/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md`
- 目标目录：`logos/resources/prd/2-product-design/1-feature-specs/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 3. deltas/prd/2-product-design/1-feature-specs/core-07-k8s-cluster-design.md

- Delta 文件：`logos/changes/k8s-cluster-deploy/deltas/prd/2-product-design/1-feature-specs/core-07-k8s-cluster-design.md`
- 目标目录：`logos/resources/prd/2-product-design/1-feature-specs/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 4. deltas/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md

- Delta 文件：`logos/changes/k8s-cluster-deploy/deltas/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md`
- 目标目录：`logos/resources/prd/3-technical-plan/1-architecture/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 5. deltas/prd/3-technical-plan/2-scenario-implementation/core-00-scenario-overview.md

- Delta 文件：`logos/changes/k8s-cluster-deploy/deltas/prd/3-technical-plan/2-scenario-implementation/core-00-scenario-overview.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 6. deltas/prd/3-technical-plan/2-scenario-implementation/core-S16-k8s-cluster.md

- Delta 文件：`logos/changes/k8s-cluster-deploy/deltas/prd/3-technical-plan/2-scenario-implementation/core-S16-k8s-cluster.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 7. deltas/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md

- Delta 文件：`logos/changes/k8s-cluster-deploy/deltas/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md`
- 目标目录：`logos/resources/prd/3-technical-plan/3-deployment/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

## 执行要求

1. 逐个 Delta 文件处理，每处理完一个报告修改摘要
2. 对于 ADDED 标记：在主文档的指定位置插入新内容
3. 对于 MODIFIED 标记：替换主文档中同名章节的内容
4. 对于 REMOVED 标记：从主文档中删除对应章节
5. 保持主文档的原有格式和风格
6. 如果主文档有"最后更新"时间戳，同步更新
7. 所有变更完成后，列出修改清单
8. 所有变更合并完成后，自动执行 git commit（告知用户，无需确认）：
   git add -A && git commit -m "docs(k8s-cluster-deploy): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive k8s-cluster-deploy`。
