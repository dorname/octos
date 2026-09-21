# 变更提案：session-switch-identity-binding

> module: core | created: 2026-09-21 | 依据：黑板条目 #11（issue #10 会话切换身份-内容绑定失败）

## 变更原因
operator 定性（issue #10 评论）：「切回后空」与「标题-内容错配」是**同一问题**的两面——切换会话后，会话身份（侧栏选中/顶栏标题）与主区消息内容未正确绑定。1970-01-01 epoch 时间戳为错配渲染的附属症状。环境：独立 web @5174（dist 含 #5 修复 ec44326）+k8s 后端 bfde941e——#5 修复后仍存在，属切换路径的**竞态/静默失败**面，#5 修的是 hydrate 丢弃面，两者互补。

## 变更类型
代码级（octos-web 前端仓）+ 测试级（vitest 回归，覆盖竞态与失败面）。

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：无（错误/竞态路径收紧，不改对外契约）
- 影响的业务场景：S16 集群域（会话切换身份-内容绑定）
- 影响的 API：无新端点；turn/error 与 hydrate 渲染路径
- 影响的 DB 表：无
- 影响的编排测试：octos-web vitest（ui-protocol-runtime / hydrate-projection / projection-render-adapter）

## 定位链（复用 #5 经验）
sidebar onClick → switchSession → startBridgeForSession → session/open → runHydrateFor → hydrateSession RPC → 快照渲染。三面：
1. **切换竞态**：快速切换（A→B→回 A）时，旧请求后到是否覆盖新会话数据——现有 generation 全局守卫 + beginSnapshot 同 key 重入守卫已覆盖大部分，需回归测试钉死；
2. **静默失败**：hydrate RPC 失败被吞成空引导态（hydrate=null → return，replaceSnapshot 不调，resetProjectionScope 已清空 → 空，无报错/重试）——#8 上报过的可观测性缺口与本条直接相关；
3. **epoch 时间戳**：user envelope（hydrate）的 data.meta 不带 persisted_at（user 分支只 text+files），渲染 user timestamp 用 `user?.seq ?? 0`——seq 是 envelope 序号（1,2,3…）非时间，undefined 时 =0 → epoch；assistant 用 `timestamp(meta?.persisted_at, segment.seq)` 兜底到 seq 同样非时间。

## 修法（三面统一）
1. **静默失败**：hydrate 失败不再静默——`runHydrateFor` 在 hydrate=null 时发出可观测信号（warning 已存在 console，升级为 store 级 hydrateFailed 标记 + 渲染层显示"加载失败，点重试"而非空引导态）；并提供 retry 入口（重新触发 runHydrateFor）。
2. **竞态**：回归测试钉死现有守卫（generation + beginSnapshot），确认 A→B→A 不错配；如发现守卫缺口则补。
3. **epoch**：hydrate user 分支的 envelope data.meta 补 persisted_at（与 assistant 一致）；渲染层 timestamp 兜底策略修正——persisted_at 缺省/非法时**不渲染 epoch/序号**，改用相邻消息时间或省略（明确兜底策略，不得渲染 epoch）。

## 本批覆盖的 UT 用例 ID（挂 S16 域接 UT-S16-33 起）
- **UT-S16-33** — 切换竞态回归：A→B→A 快速切换，旧 A hydrate 晚到不覆盖新 A 快照（generation + beginSnapshot 守卫）
- **UT-S16-34** — 静默失败修复：hydrate RPC 失败时 store 标记 hydrateFailed（非空引导态），渲染层可据以显示重试
- **UT-S16-35** — epoch 修复：hydrate user envelope meta 含 persisted_at；渲染 timestamp 兜底不渲染 epoch/序号
与 `logos/resources/test/core-S16-test-cases.md` 对齐（merge 时追加）。

## 部署影响
- 是否需要部署：是（前端 dist 重建 + 重新注入）
- 部署原因：前端行为变更
- 影响环境：本地 docker-desktop octos ns（@5174）
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：是（真机 5174 复现验证，自建测试会话）
