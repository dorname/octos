# Delta: prd/2-product-design/1-feature-specs — core-04-serve-api-design.md

> target: logos/resources/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md

## ADDED — 一附、S05 集群模式扩展（S16）

### 1附.1 集群模式启动

当配置/环境启用 PostgreSQL 集群后端时，`octos serve` 进入集群角色（见 `serve_cluster` / 部署清单）：

1. 校验 `DATABASE_URL` 与迁移版本
2. 挂载 UI Protocol / admin 路由（与单机相同对外契约）
3. 会话事件、审批、租约写入 PG；WS 支持 `replay_from_pg`
4. Cron 使用 `CronServicePg`（若启用定时任务）

#### 验收条件（交互级增量）

##### 正常：集群 serve 探活
- **GIVEN** cluster 形态已部署
- **WHEN** 客户端访问 `GET /api/version` 与仪表盘
- **THEN** 行为与单机 serve 一致（200 + 页面），后端状态在 PG

##### 异常：缺 DATABASE_URL
- **GIVEN** 集群标志已开但无可用 PG
- **WHEN** 启动
- **THEN** 失败并提示修复路径，不静默回落 JSONL 真相源
