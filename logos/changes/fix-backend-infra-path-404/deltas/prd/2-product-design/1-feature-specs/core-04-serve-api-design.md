# Delta: prd/2-product-design/1-feature-specs — core-04-serve-api-design.md

> target: logos/resources/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md

## ADDED — 1附.3 未注册基础设施路径 404 契约

### 1附.3 未注册基础设施路径 404 契约

`octos serve` 的 SPA fallback 只对**前端路由**负责。以下基础设施路径族在未注册到路由表时，必须返回 `404 application/json`（`{"error":"not_found","path":"<request-path>"}`），绝不 `307` 重定向进 SPA——API 客户端（Playwright `apiRequestContext`、reqwest 等）会被重定向上限或 HTML body 打断：

- 前缀族（段边界匹配）：`api`、`webhook`、`internal`、`v1`
- 精确路径：`health`、`openapi.json`、`docs`（及 `docs/` 前缀）

段边界约束：`/v1beta`、`/apiculture`、`/healthcare` 等兄弟路径**不**命中白名单，维持原 SPA 逻辑。

#### 验收条件（交互级）

##### 正常：未注册 API 形态路径 404 JSON
- **GIVEN** serve 运行中
- **WHEN** 客户端 GET/POST `/v1/chat/completions`、`/openapi.json`、`/docs`（均未注册）
- **THEN** 均返回 404 `application/json`，body 含 `"error":"not_found"` 与请求路径；无 307/Location 头

##### 正常：兄弟路径不误判
- **GIVEN** 同上
- **WHEN** 请求 `/v1beta` 或 `/apiculture`
- **THEN** 不返回 404 JSON；按原 SPA 逻辑处理

##### 正常：已注册路由不回退
- **GIVEN** 同上
- **WHEN** 请求 `/health`（已注册）与 `/app/`（SPA）
- **THEN** `/health` 仍由 handler 返回 200 JSON；`/app/` 仍返回 SPA HTML——白名单只影响未注册路径
