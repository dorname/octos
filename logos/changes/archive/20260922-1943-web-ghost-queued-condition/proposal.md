# 变更提案：ghost 驻留判定条件修复（queued 不应依赖队列总数）

## 变更原因

真机验收发现：#57③（GhostBubble queued 属性抑制 30s 确认定时器）经 #60① 接线后存在**条件缺陷**——`dispatchGhostQueued` 仅在队列总数 `total > 1` 时置 `queued=true`。当**单条**消息被会话 FIFO 生命周期门驻留（上一 turn 未结算、消息未上 wire）时 `total == 1`，被判为 live，GhostBubble 的 30s 确认定时器照常点火，操作员持续看到「30s 超时误报」（operator 2026-09-21 复核："错了30s的问题依然存在"）。

后端侧已由外环 WS probe 实证排除：`turn/start` 回显客户端 turn_id（首 turn 与后续 turn 均 `ECHO_MATCH=True`），terminal 结算匹配在当前源码下成立，后端无需改动。

## 变更类型

代码级修复

## 变更范围

- 影响的需求文档：无（octos-web 为新纳入模块，暂无规格资源；行为契约以本提案「变更概述」为准）
- 影响的功能规格：无
- 影响的业务场景：octos-web 会话发送链路（FIFO 驻留 → ghost 展示）
- 影响的部署方案：octos-web dist 构建产物注入本地 docker-desktop `octos` ns 的 web pod（nginx webroot），注入方式不变
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无
- 影响的 smoke 测试：无

涉及源文件（预期，切片阶段可细化）：
- `octos-web/src/runtime/ui-protocol-send.ts` — `dispatchGhostQueued` 的 `queued` 判定（当前 `total > 1`）
- `octos-web/src/components/chat-thread.tsx` — `crew:ghost_queued` 监听与 GhostSpec.queued 传递
- `octos-web/src/components/GhostBubble.tsx` — `queued` prop 对 30s 定时器的门控语义

## 部署影响

- 是否需要部署：是
- 部署原因：修复作用于前端 bundle，需重新构建 dist 并注入 k8s web pod 后操作员方可真机复验
- 影响环境：本地（docker-desktop `octos` ns）
- 是否涉及数据迁移：否
- 是否需要回滚预案：是（回滚 = 重新注入上一版 bundle tar.gz，已有留存）
- 是否需要 smoke：否（前端 bundle 验收由操作员真机人工复验承担，未设自动化 smoke）

## 变更概述

行为契约：**只要消息被会话 turn 生命周期门驻留（尚未真正上 wire），GhostBubble 一律视为 queued，不得启动 30s 确认定时器**；队列总数仅是驻留的间接信号，不作为判定依据。实现上需将 `dispatchGhostQueued` 的 `queued` 判定从 `total > 1` 改为「该消息经 FIFO 驻留」这一直接事实（或等价的 lifecycle-parked 状态），并保持 `total > 1` 场景行为不回退。

不在本提案范围（另行提案）：
- #57②④ 加固批次（replaceSnapshot 活跃 turn 对账 fireComplete、切回会话 hydrate 对账）——已两次 NEAR-LIMIT 延期
- octos-web 模块规格资源（PRD/场景/测试）的补建——作为模块纳入后的独立推进事项
