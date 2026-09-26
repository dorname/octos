# 变更提案：用户气泡与助手回复垂直错位修复（issue #20）

> module: core | created: 2026-09-26

## 变更原因

fork issue #20（`bug(chat): 用户消息与助手回复垂直错位（生成中 / 重进会话）`）真机复现两组症状：

- **生成中（streaming）**：助手回复气泡渲染在用户触发消息**上方**，用户气泡（ghost）垫底；时间戳显示用户更早，布局却颠倒。
- **重进会话后**：错位持续存在——用户气泡落在助手大块输出下方，二者间有大块空白（两个独立 thread bundle 的间距）。

根因分析（代码级核实）：

1. **链 A（streaming 主因）——ghost 无条件垫底**。canonical `user_message` 要等 turn 结束才发射；生成中时 turn thread 只有助手帧，`view.user === null` 渲染为空占位行（`projection-render-adapter.ts` placeholderOrigin）。`chat-thread.tsx` 的 ThreadList 先 `threads.map(...)` 后 `ghosts.map(...)`（约 1398-1420 行），在途 turn 的 ghost 永远挂在列表最底部 → 「助手在上、用户 ghost 在下」。turn 正常结束后 canonical user 到达、ghost settle 消失，布局自愈——这只是瞬时错位，但配合链 B 会固化。
2. **链 B（持久错位）——同 turn 的 user 与 assistant 拆成两个 thread 块**。`projection.ts:165-178` 严格按 `thread_id` 分组、按首次 admitted 排序，没有按 `turn_id` 归并。c21a539 的 cmid 跨 thread 去重是「先 admitted 者存活」（`projection-store.ts:284-290`）：若 durable user 行先经 hydrate admitted 到 thread A、live 回声（thread B = turn UUID）被丢，而助手帧都在 thread B，则 threadOrder = [..., B, A] → 用户块固定在助手块下方。

历史相关：与 #19（FIFO 卡死 / 历史丢失，已修复归档 20260924-0018）同一投影排序病灶面；#19 的 checkpoint 结算已消除 wedge 这个诱发条件，本提案收口渲染排序本身。

## 变更类型

代码级修复

## 变更范围

- 影响的需求文档：无（octos-web 模块暂无规格资源；行为契约以本提案「变更概述」为准）
- 影响的功能规格：无
- 影响的业务场景：octos-web 会话渲染链路（streaming ghost 锚定、投影 thread 归并排序）
- 影响的部署方案：`logos/resources/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md` 注入方式不变（web dist 注入 k8s web pod）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无
- 影响的 smoke 测试：无（沿用既有真机复验清单，不新增自动化 smoke）

涉及源文件（预期，切片阶段可细化）：
- `octos-web/src/store/projection.ts` — ThreadView 按 `turn_id` 归并（同 turn 的 user 与 assistant 不拆块）
- `octos-web/src/components/chat-thread.tsx` + `octos-web/src/components/GhostBubble.tsx` — 在途 turn 的 ghost 锚定到其 turn 的 thread 位置（填充空 user 占位），而非无条件垫底
- `octos-web/src/store/projection-render-adapter.ts` — 归并后的 Thread → 渲染适配（如需）

## 部署影响

- 是否需要部署：是
- 部署原因：修复作用于前端 bundle，需重新构建 dist 注入 k8s web pod 后操作员方可真机复验 issue #20 验收标准
- 影响环境：本地（docker-desktop `octos` ns）
- 是否涉及数据迁移：否
- 是否需要回滚预案：是（回滚 = 重新注入上一版 bundle tar.gz，`/tmp/octos-k8s-bin/octos-web-dist.tar.gz.bak-c21a539` 及后续留存）
- 是否需要 smoke：否（真机复验由操作员人工承担；回归靠 octos-web UT）

## 变更概述

行为契约：

1. **turn 归并（治链 B）**：投影渲染层按 `turn_id` 归并 ThreadView——同一 turn 的 `user_message` 与其 assistant/tool/terminal 帧永远渲染为同一个会话块，用户气泡在上、助手内容在下；即使它们的 envelope 分属不同 `thread_id`（turn UUID vs canonical conversation thread），也不得拆成两个独立块或按 admission 顺序错位。
2. **ghost 锚定（治链 A）**：在途 turn 的 ghost 渲染到其 turn 的 thread 位置——当该 turn 的 canonical thread 已出现助手帧且 user 为空占位时，ghost 内容填充该占位（用户在上、streaming 助手在下），不再无条件垫底；ghost settle / failure 语义不回退（#57③ #60 #63 的 queued 门控保持不变）。
3. **排序稳定性**：刷新 / hydrate / replaceSnapshot 后气泡顺序与时间戳一致；乐观用户帧 + snapshot 合并不重复、不落到线程尾部。
4. **回归测试**：覆盖「同 turn user/assistant 分 thread 到达 → 渲染仍同块且用户在前」「streaming 中 ghost 锚定在其 turn 上方」「hydrate 后排序稳定」三类用例。

不在本提案范围（另行提案）：
- `hydrateFailed` 标记的 UI 重试入口（沿用 #19 提案的范围外结论）
- 服务端 `TurnCompleted` 固定 `topic: None` 与 topic scope 过滤的不匹配（沿用 #19 提案的范围外结论）
- octos-web 模块规格资源补建

## UI/UX 变更声明

- `ui_impact`: false（恢复正确排序的行为修复，无新界面设计；与 fix-web-turn-terminal-fifo-wedge 同一先例）
- `design_system_mode`: fallback
- `design_system_fallback_reason`: 本次变更无 UI 原型交付（ui_impact=false）
