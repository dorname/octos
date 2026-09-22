# Delta: prd/3-technical-plan/3-deployment — core-01-deployment-plan.md

> target: logos/resources/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md

## ADDED — 十一、octos-web 反代路径白名单契约

## 十一、octos-web 反代路径白名单契约

前后端分离形态（`deploy/k8s/04-octos-web-standalone.yaml`）下，octos-web 的 nginx 是浏览器到后端的唯一入口。其 `default.conf` 的路由契约为**白名单制**——只有以下路径族反代到 `octos.octos.svc.cluster.local:8080`，其余一律 SPA fallback（`try_files ... /index.html`，前端 client routing 依赖此行为）：

| location | 匹配 | 后端路由 | 备注 |
|----------|------|----------|------|
| `/api/` | 前缀 | 全部主路由组（REST + `/api/ui-protocol/ws`） | WS upgrade + 3600s 读超时 |
| `= /health` | 精确 | `handlers::health`（router.rs:899） | 前端源码唯一直接调用的非 `/api` 路径 |
| `/v1/` | 前缀 | `/v1/session_ingress/ws/{session_id}`（router.rs:952） | session-ingress WS 族 |

约束：

1. 三个反代 block 共用同一组 proxy 参数（Host/X-Forwarded 头、WS upgrade 头、3600s 读写超时、`proxy_buffering off`）；
2. nginx location 优先级（精确 > 前缀）保证 `= /health` 不被 `location /` 捕获；
3. 后端路由表中**不存在**的路径（如 `/openapi.json`、`/docs`、`/v1/chat/completions`）不属于白名单——它们命中后端时的正确行为是 `404 application/json`（见 serve API 规格），nginx 不为不存在的路由开口。

### 验收条件（部署级）

##### 正常：健康检查穿透
- **GIVEN** octos-web pod 以新配置运行
- **WHEN** 经 web 端口请求 `GET /health`
- **THEN** 返回 `200 application/json` `{"status":"healthy",...}`，而非 index.html

##### 正常：/v1/ 不落 SPA
- **GIVEN** 同上
- **WHEN** 请求 `/v1/session_ingress/ws/x`（无 WS upgrade）
- **THEN** 返回后端真实响应（4xx），而非 index.html

##### 正常：SPA 与 /api 不回退
- **GIVEN** 同上
- **WHEN** 请求 `/chat`（前端路由）与 `/api/auth/me`
- **THEN** `/chat` 仍返回 index.html；`/api/auth/me` 仍返回后端 JSON（401/200），两者行为与变更前一致
