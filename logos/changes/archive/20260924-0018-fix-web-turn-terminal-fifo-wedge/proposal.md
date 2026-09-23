# 变更提案：回合 terminal 结算与 hydrate 快照压缩导致的 FIFO 卡死 / 历史丢失修复（issue #19）

> module: core | created: 2026-09-23

## 变更原因

fork issue #19（`bug(chat): 回合已结束仍 queued，且切换会话历史逐条消失`）真机复现三组症状：

- **A. FIFO 卡死**：短回合（「你好」）助手已完整渲染，但发送下一条消息时横幅 `1 message queued — the previous turn is still running`，消息永久驻留队列（安全网为 15 分钟 timer；刷新或久等后恢复）。
- **B. 历史逐条消失**：卡队列状态下侧边栏反复切换会话，可见历史每切换一次少一条直至空白；空白时主区误显示 CONVERSATION STUDIO 欢迎页（会话元数据仍在）。
- **C. epoch 时间戳**：助手气泡偶发 `1970-01-01 08:00`。

根因分析（代码级，已核实）：

1. **症状 A/B 共同根因——hydrate 快照压缩与客户端 seq 连续性门的不兼容**。服务端 `compact_hydrate_projection_replay`（`crates/octos-cli/src/api/ui_protocol_transport.rs:25952-25993`）对「per-thread seq 不连续」或「序列化字节超预算（`MAX_TEXT_FRAME_BYTES/4`，按 thread_id BTreeMap 序逐个消耗）」的 thread **只保留 `TurnTerminal` 帧（保留其原始 seq，如 seq=9，前驱帧缺失）**。客户端 `ProjectionStore.ingestCanonical`（`octos-web/src/store/projection-store.ts:267-327`）要求 per-thread seq 严格连续，terminal 因前驱缺失被永久挂入 `pendingByThread`——这是**结构性 gap**：再次 hydrate 返回同样的压缩快照，永远补不上前驱帧（c21a539 的 backoff 只压住 hydrate 风暴，未治 gap 本身）。后果有二：
   - terminal 永不 admitted → `notifyEnvelopeAdmitted` 不触发 → `ui-protocol-send.ts` 的 FIFO 生命周期门等不到 `fireComplete` → **症状 A**（卡 queued 直到 15 分钟安全网）。
   - 客户端 hydrate 路径**优先使用 canonical `projection_snapshot/projection_envelopes`**（`hydrate-projection.ts:58-74`）而弃用 `messages` 转录行：被压缩 thread 的消息行全部从渲染中消失；对话增长使更多 thread 逐次超预算 → **症状 B**（每切换一次历史少一条直至空白）。服务端已返回 `projection_thread_sequences` checkpoints 但客户端未消费。
2. **症状 C 根因——渲染适配器时间戳 fallback 误用 seq**。`projection-render-adapter.ts` 多处（assistant segment、terminal error、orphan tool、background child）在 `persisted_at` 缺失时以 `seq`（1,2,3…）作 `new Date()` 输入，直接渲染为 1970-01-01。
3. **附带**：c21a539 向 `ui-protocol-runtime.ts` 提交了调试 `console.log`（456-467、556 行，`[dbg ...]` 字样，含未配平方括号），应清理。

历史相关：`logos/changes/archive/20260922-1943-web-ghost-queued-condition/proposal.md` 记录 **#57②④**（replaceSnapshot 活跃 turn 对账 fireComplete、切回会话 hydrate 对账）两次 NEAR-LIMIT 延期——与本提案同一病灶面，本次一并收口。

## 变更类型

代码级修复

## 变更范围

- 影响的需求文档：无（octos-web 模块暂无规格资源；行为契约以本提案「变更概述」为准）
- 影响的功能规格：无
- 影响的业务场景：octos-web 会话发送链路（FIFO 结算）、会话切换 hydrate 渲染链路
- 影响的部署方案：`logos/resources/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md` 注入方式不变（web dist 注入 k8s web pod；如服务端改动落地则 musl 重建注入，路径同 #12 修复）
- 影响的 API：`session/hydrate` 响应消费方式（wire 格式不变；`projection_thread_sequences` 由未消费变为消费）
- 影响的 DB 表：无
- 影响的编排测试：无（协议 e2e 现有用例不应回退）
- 影响的 smoke 测试：无（沿用既有 k8s WS 真机复验清单，不新增自动化 smoke）

涉及源文件（预期，切片阶段可细化）：
- `octos-web/src/store/projection-store.ts` — 快照内孤立 `TurnTerminal`（前驱 seq 缺失）的结算豁免 / checkpoint 对齐
- `octos-web/src/runtime/hydrate-projection.ts` — 消费 `projection_thread_sequences`；canonical 快照缺行 thread 回退 `messages` 转录重建
- `octos-web/src/store/projection-render-adapter.ts` — 时间戳 fallback 去 seq 化
- `octos-web/src/runtime/ui-protocol-runtime.ts` — 清理调试 console.log
- （视方案取舍）`crates/octos-cli/src/api/ui_protocol_transport.rs` — 压缩策略配合（如 compacted thread 附带 transcript 可重建行）

## 部署影响

- 是否需要部署：是
- 部署原因：修复作用于前端 bundle（及可能的服务端二进制），需重建注入 k8s 后操作员方可真机复验 issue #19 验收标准
- 影响环境：本地（docker-desktop `octos` ns）
- 是否涉及数据迁移：否
- 是否需要回滚预案：是（回滚 = 重新注入上一版 bundle/binary，已有留存）
- 是否需要 smoke：否（真机复验由操作员人工承担；回归靠 octos-web UT + 仓库 nightly 协议 e2e）

## 变更概述

行为契约：

1. **结算鲁棒性（症状 A）**：`turn_terminal` 到达客户端投影层后必须能结算会话 FIFO——即使它作为快照内孤立帧（前驱 seq 因服务端压缩缺失）出现。实现方向：客户端消费 hydrate 响应已有的 `projection_thread_sequences` checkpoints 对齐 per-thread expected seq（或对「快照内孤立 terminal」豁免连续性检查）；terminal 结算幂等，重复结算无副作用。已真实在途的 turn 仍须排队，terminal admitted 后按序放行（不回退既有 FIFO 语义）。
2. **历史完整性（症状 B）**：reset → hydrate → replaceSnapshot 循环不得丢弃任何先前已渲染的消息行。canonical 快照中被压缩（缺消息行）的 thread 必须回退用 `messages` 转录行重建（hydrate-projection 已具备该转换能力）；反复切换会话后历史保持完整，非空会话不得误显示空白欢迎页。
3. **时间戳（症状 C）**：消息时间戳缺省时回退到接收时刻或隐藏，禁止以 seq 序号渲染 epoch（1970-01-01）。
4. **附带清理**：删除 c21a539 引入的调试 console.log。

不在本提案范围（另行提案）：
- `hydrateFailed` 标记的 UI 重试入口（issue #10 注释要求的 retry affordance 未实现——属新增 UI，另案评估）
- 服务端 `TurnCompleted` 固定 `topic: None` 与 topic-scoped 连接 replay/forwarder 过滤的不匹配（`ui_protocol_transport.rs:19515-19541`、39805+；仅影响 topic scope 会话，另案）
- octos-web 模块规格资源（PRD/场景/测试 ID 注册表）补建

## UI/UX 变更声明

- `ui_impact`: false（纯行为修复，无界面视觉/交互变更；hydrateFailed 重试入口已明确划出范围）
- `design_system_mode`: fallback
- `design_system_fallback_reason`: 本次变更无 UI 原型交付（ui_impact=false）
