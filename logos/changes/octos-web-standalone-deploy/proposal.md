# 变更提案：octos-web-standalone-deploy

> module: core | created: 2026-09-21 | 依据：黑板条目 #10（独立部署 octos-web 前后端分离）

## 变更原因
operator 裁决：后端（k8s serve）与 web 独立，`/app/` 内嵌路径废弃（503 属预期）。当前 web 为外环临时 vite dev（5173，会话级，随外环存亡），需部署常驻独立 web。形态自选（A=k8s 内 nginx 托管+反代 / B=本地常驻），本提案选 **形态 A**（理由见下）。新增 git 清单文件（k8s manifest）须走 change 流程，本提案即其载体。

## 变更类型
部署级（新增 k8s 清单 + live 资源）+ 文档级（存活边界说明）。

## 形态选择与理由
**选形态 A（k8s 内 nginx 托管 + `/api`/WS 反代至 `svc/octos:8080`）**，理由：
1. **常驻性**：Deployment 由 k8s 托管，不随外环会话存亡（对比形态 B 的 nohup 本地进程，机器重启即丢）。
2. **同源免白名单**：nginx 反代 `/api` 与 WS 到 `svc/octos:8080`，浏览器视角全同源（`http://<pf>/api/...`），不依赖 `OCTOS_APPUI_ALLOWED_ORIGINS` 白名单维护（形态 B 跨端口需另配 origin）。
3. **与后端 50080 PF 并存不冲突**：独立 Service + 独立 port-forward（避开 50080），后端 PF 不动。
4. **live 资源边界清晰**：dist 经 ConfigMap/emptyDir 或镜像注入，存活边界在 ACK 写明（ConfigMap 有 1MB 上限，dist ~4MB 超限，故用 emptyDir + init 注入或 nginx 镜像内 COPY——见下）。

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：无（纯部署拓扑）
- 影响的业务场景：S16 集群域（octos-web 独立部署）
- 影响的 API：无新端点；nginx 反代 `/api/*` → `svc/octos:8080`、`/api/ui-protocol/ws`（WS upgrade）同源
- 影响的 DB 表：无
- 影响的编排测试：deploy_directory_layout（新增 octos-web Deployment/Service 断言，如适用）

## 部署影响
- 是否需要部署：是（本提案即部署）
- 部署原因：常驻独立 web
- 影响环境：本地 docker-desktop octos ns
- 是否涉及数据迁移：否
- 是否需要回滚预案：否（`kubectl delete -f` 即回滚）
- 是否需要 smoke：是（web 入口 HTTP 200 + 反代 /health 200）

## 变更概述
部署 octos-web 静态产物（dist，已重建含 #5 hydrate 修复）为 k8s 内常驻服务：
1. dist 经 init container + HTTP fetch（复用 octos binary 同款宿主 HTTP 模式，:18088 实况）注入 emptyDir，nginx 托管；
2. nginx 反代 `/api/*`（含 `/api/ui-protocol/ws` WS upgrade）→ `svc/octos:8080`，浏览器全同源；
3. 独立 Deployment（replicas=1）+ Service（ClusterIP，port 80）入 octos ns；
4. port-forward 暴露（避开 50080 与 5173，选 5174）；
5. 新增清单文件 `deploy/k8s/04-octos-web-standalone.yaml`（git 跟踪，本 change 流程产物）。

## 存活边界（ACK 将写明）
- k8s 资源（Deployment/Service/ConfigMap）常驻于 octos ns，随集群存亡；
- dist 注入依赖宿主 HTTP 服务（:18088，会话级）——**pod 重建需宿主 HTTP 在线**，否则 init 失败（与 octos binary 同边界，文档已述）；
- port-forward 为会话级（kubectl 进程存亡），非 k8s 资源。
