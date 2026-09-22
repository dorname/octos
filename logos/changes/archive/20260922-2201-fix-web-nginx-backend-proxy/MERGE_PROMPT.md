# 合并指令

## 变更提案
- 提案名称：fix-web-nginx-backend-proxy
- 提案目录：logos/changes/fix-web-nginx-backend-proxy/

## 提案内容

# 变更提案：fix-web-nginx-backend-proxy

> module: core | created: 2026-09-22

## 变更原因
Issue #15（fork dorname/octos）：octos-web 的 nginx（`deploy/k8s/04-octos-web-standalone.yaml` ConfigMap `octos-web-nginx`）只 proxy `/api/*`，其余路径全部被 `try_files ... /index.html` SPA fallback 吞掉。已核实的影响面：

- 前端源码**实际调用**的非 `/api` 后端路径只有 `/health`（`octos-web/src/lib/` 全局仅此一处）——经 5174 访问返回 `200 text/html`（index.html），应为后端 JSON `{"status":"healthy",...}`；
- 后端真实存在但不挂 `/api` 前缀的路由族：`/health`（router.rs:899）与 `/v1/session_ingress/ws/{session_id}`（router.rs:952，WS）——后者经 nginx 完全不可达；
- issue 中提到的 `/openapi.json`、`/docs`、`/v1/chat/completions` 经核实**后端路由表中不存在**，不属于本提案范围（那些路径的正确行为是后端 404 JSON，由 issue #17 独立提案处理）。

## 变更类型
设计级（部署契约修正：前后端分离部署的反向代理路径白名单）。

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：`logos/resources/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md` 的 octos-web 反代契约
- 影响的业务场景：S05（REST API 服务与流式集成）的前后端分离部署形态
- 影响的 API：无（后端路由不变，仅 nginx 放行已有路由）
- 影响的 DB 表：无
- 影响的编排测试：无

## 部署影响
- 是否需要部署：是
- 部署原因：修复对象就是 k8s 部署清单；apply 后需滚动重建 octos-web pod 使新 nginx 配置生效
- 影响环境：本地（docker-desktop k8s `octos` ns）
- 是否涉及数据迁移：否
- 是否需要回滚预案：否（回滚 = 还原 ConfigMap）
- 是否需要 smoke：是（经 5174 验证 `/health` 返回 JSON、`/v1/` 不再落 SPA、SPA 前端路由不受影响、`/api/` 行为不回退）

## 变更概述
在 `default.conf` 新增两个 location block，与 `/api/` 共用相同的反代参数（含 WS upgrade 头与长读超时）：

1. `location = /health` — 精确匹配健康检查，proxy 到 `octos.octos.svc.cluster.local:8080`；
2. `location /v1/` — 前缀匹配 session-ingress WS 路由族，proxy 到同一后端。

其余路径维持 SPA fallback 不变（前端 client routing 依赖它）；`/api/` block 不变。nginx location 优先级（精确 > 前缀 > 正则 fallback）保证 `/health` 不会被 `location /` 捕获。


## 需要合并的 Delta 文件

### 1. deltas/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md

- Delta 文件：`logos/changes/fix-web-nginx-backend-proxy/deltas/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md`
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
   git add -A && git commit -m "docs(fix-web-nginx-backend-proxy): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive fix-web-nginx-backend-proxy`。
