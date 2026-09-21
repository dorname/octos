# 实现任务

## [code] 代码实现
- [x] 重建 octos-web dist（含 #5 hydrate 修复，Node 22 串行 build）
- [x] 新增 git 清单 `deploy/k8s/04-octos-web-standalone.yaml`（Deployment+Service+nginx CM+init 注入）
- [x] live 部署：`kubectl apply`（octos ns）+ port-forward 5174
- [x] 验收：web 入口 HTTP 200 + 反代 /api/health 200 + 与 50080 PF 并存

## 验收
- web 入口（http://localhost:5174/）HTTP 200
- 反代 `/api/...`（同源）可达后端
- 登录/会话列表可用（经反代）
- 与后端 50080 PF 并存不冲突
- 存活边界在 ACK 写明
