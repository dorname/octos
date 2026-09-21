# 变更提案：k8s-appui-allowed-origins-manifest

> module: deploy | created: 2026-09-21

## 变更原因
黑板条目 #16（治本）。#14 rollout 后 operator 真机重现 WS 域白名单错误
（"Unable to establish the UI Protocol connection…this page origin is allowed"）——
`OCTOS_APPUI_ALLOWED_ORIGINS` 当初是 `kubectl set env` 打在 live Deployment 上的，
**未进 git manifest**，每次滚动都可能丢（本次实案）。外环 live 修复实证：
值里从未含独立 web 反代域 `5174`（原值仅 5173/50080），5174 发的 WS 握手
被 origin gate 拒绝。治本要求把该 env **显式写进 git Deployment 清单**并补语义文档。

## 变更类型
配置级（deploy manifest + 文档），无代码逻辑变更。

## 变更范围
- 影响的需求文档：无（不改对外功能契约）
- 影响的功能规格：无
- 影响的业务场景：k8s 集群部署下浏览器 AppUI/WebSocket 的 origin 白名单来源
  （live-only env → git manifest 声明，滚动后保留）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无（纯清单+文档，无 UT——**理由**：变更仅为 Deployment env
  值字符串追加 + Markdown 文档文字，无可执行代码路径、无可测逻辑单元；
  正确性由 YAML 语法校验 + 外环真机 rollout 验收覆盖，非单测范畴）

## 部署影响
- 是否需要部署：是（由外环在 #14 重做部署时引用本清单；本提案不自行部署）
- 部署原因：env 值变更需 rollout 才生效；但部署动作属外环编排（#14 重做），不在本提案执行范围
- 影响环境：k8s 集群（测试/生产同清单）
- 是否涉及数据迁移：否
- 是否需要回滚预案：否（env 追加为纯增量，不删既有值）
- 是否需要 smoke：是（外环部署后按 #14 四条标准 + 5174 WS 握手验收）

## 变更概述
1. `deploy/k8s/03-cluster-with-config.yaml` 的 serve Deployment：
   `OCTOS_APPUI_ALLOWED_ORIGINS` 值追加 `http://127.0.0.1:5174,http://localhost:5174`
   （独立 web 反代域）；注释注明 PF 端口与 5174 须显式列出（serve 只自动放行
   容器绑定端口 8080 的 loopback）。git 清单无 5173 占位（5173 是 #14 live 时
   `kubectl set env` 的临时值，不入 git），故仅加 5174。
2. `deploy/docs/K8S_DEPLOY_PROVEN.md`：`kubectl set env` 示例同步加 5174；
   新增 env 语义段——空/未设 = 仅放行不带 `Origin` 头的通道（curl/服务器间/CLI），
   浏览器 WS **必带 `Origin` 头**、不在白名单即 403（前端表现为通用 UI Protocol
   连接失败）；并注明"勿只 `kubectl set env` 打 live——滚动即丢，#14 实案"。

## 实现状态
实现层已完成并 commit（`083b0e52`，分支 dorname-k8s-dev），YAML 语法校验 OK
（8 docs）。本提案仅补 openlogos 流程节点（change/merge），实现不重做。
