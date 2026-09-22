# 实现任务

## [delta] 规格变更
- [x] 产出 delta：`core-01-deployment-plan.md` 增补 octos-web nginx 反代路径白名单契约（`/api/` + `= /health` + `/v1/`，其余 SPA fallback）

## [code] 代码实现
- [x] 单切片：修改 `deploy/k8s/04-octos-web-standalone.yaml` ConfigMap `octos-web-nginx` 的 `default.conf`——新增 `location = /health` 与 `location /v1/` 两个反代 block（参数与 `/api/` block 一致：Host/X-Forwarded 头、WS upgrade、3600s 超时、关闭缓冲）；不改变 `location /` 与 `location /api/`。验收：`nginx -t` 等价静态校验（location 语法 + 括号配对）

## [deploy] 部署任务
- [x] `kubectl apply` 新 ConfigMap 并滚动重建 octos-web pod，经 5174 真机 smoke：`/health` → JSON `{"status":"healthy"}`；`/v1/session_ingress/ws/x` → 非 SPA（400/401/426 等后端真实响应而非 index.html）；`/chat` → 仍返回 index.html（SPA 不回退）；`/api/auth/me` → 后端 JSON（不回退）
