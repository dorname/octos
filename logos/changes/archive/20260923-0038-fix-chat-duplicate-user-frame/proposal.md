# 变更提案：fix-chat-duplicate-user-frame

> module: core | created: 2026-09-22

## 变更原因
Issue #18（fork dorname/octos）：chat 发送消息、assistant 回复完成后约 10~60s，主区**追加一个重复的 user 消息 frame**（ISO 时间戳格式；刷新页面后消失；服务端持久化数据无重复）。稳定复现 3/3。

WS 抓帧实证根因（双身份 + 无去重键）：

1. **live echo**：`turn/start` 提交后服务端经 commit observer 发出 live `user_message` v2 envelope——`thread_id = <客户端 turn UUID>`、seq 2、**无 `client_message_id`** → 渲染第一个 user frame（HH:MM 时间戳）。
2. **durable row**：同一消息持久化后 thread_id 被重盖章为另一个服务端线程 id（`derive_thread_id_for_new_write` 路径），`client_message_id` 同样为 `None`（`process_message_inner` 构建 user 行时硬编码 `None`）。
3. **snapshot 合并**：turn 完成后的一次 `session/hydrate`（gap 触发的 requestRehydrate）把 durable user 行带入快照——thread_id 不同、`client_message_id` 处处缺失 → projection-store 的 #54 cmid 去重无从触发 → 两个线程各渲染一个 user frame（durable 行带 `persisted_at` → ISO 时间戳）。

关键事实：前端把 `turn_id` 与 `clientMessageId` 钉为同一 UUID（`ui-protocol-send.ts:542`），但 `turn/start` 的 wire 参数只带 `turn_id`，服务端从未把它落成 `client_message_id`——**去重键在服务端被丢弃**。

## 变更类型
代码级（跨 core 后端 + octos-web 前端；wire 变更为**可选字段按既有 schema 填充**，`client_message_id` 在 envelope / hydrate message 契约中均已定义为 optional，纯增量、向后兼容）。

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：`core-04-serve-api-design.md`（附录增补 user_message cmid 贯通契约）
- 影响的业务场景：chat 会话页消息投影
- 影响的 API：WS `user_message` envelope 与 `session/hydrate` messages（仅填充已有 optional 字段）
- 影响的 DB 表：无（session JSONL 行内字段，非独立表）
- 影响的编排测试：无

## 部署影响
- 是否需要部署：是
- 部署原因：k8s octos pod（后端）与 web pod（前端 dist）都需更新
- 影响环境：本地（docker-desktop k8s `octos` ns）
- 是否涉及数据迁移：否（历史行 `client_message_id` 为空 → 保持现状渲染，仅新消息受益）
- 是否需要回滚预案：否
- 是否需要 smoke：是（playwright 真机：发送消息后 90s 内主区 user frame 数 == 1；刷新后仍 == 1）

## 变更概述
1. **服务端（octos-cli）**：WS `turn/start` 流程构建/持久化 user `Message` 时，以 `client_message_id = Some(turn_id)` 盖章（前端 turn_id 即 clientMessageId）。commit observer 既有逻辑会自动把它带上 live envelope；确认 `session/hydrate` messages 序列化包含该字段。
2. **前端（octos-web，防御性对称去重）**：`projection-store` 的 `ingestCanonical` 对 `user_message` 增加 cmid 去重——同一会话已收录过相同 `client_message_id` 的 user frame（任意 thread）时判为 duplicate，覆盖「snapshot 先到、live echo 后到」的反向时序。
3. **测试**：
   - Rust UT：turn/start 持久化 user 行带 `client_message_id == turn_id`；hydrate messages 携带 cmid；live envelope 携带 cmid（CORE-DUPFRAME-01..03）
   - vitest：projection-store 双向时序去重（live 先到 / snapshot 先到）（OW-DUPFRAME-01..02）
   - playwright smoke：发送 → 90s 观察 → user frame == 1；刷新后 == 1
