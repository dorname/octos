# 变更提案：serve-auth-failfast

> module: core | created: 2026-09-21 | 依据：黑板条目 #8（新规范全流程实战）

## 变更原因
黑板 #6 批 3 issue #8 分类结案随帖上报的独立改进建议之二：serve turn 链路对 LLM 上游鉴权类失败（401/403）挂起而非快速失败——前端「Send not confirmed within 30s」症状根因之一。外环已采认。本条为新规范（通告三流程硬约束 + 通告四节点通知协议 + loop.md 第 4-6 条）的首轮全流程实战载体。

## 变更类型
代码级（行为修正：错误路径快速失败）+ 测试级（UT 断言）。

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：无（错误路径行为收紧，不改对外契约形状——turn/error 仍按既有 envelope 落态）
- 影响的业务场景：S16 集群域（serve/turn LLM 鉴权失败路径）
- 影响的 API：无新端点；turn/error envelope 的 code/message 在上游鉴权失败时带上游摘要
- 影响的 DB 表：无
- 影响的编排测试：crates/octos-llm（RetryProvider 鉴权不重试断言）、crates/octos-cli（turn 快速失败落态断言）

## 部署影响
- 是否需要部署：是（serve 行为变更）
- 部署原因：turn 链路错误处理路径修正
- 影响环境：本地 / 集群
- 是否涉及数据迁移：否
- 是否需要回滚预案：否（错误路径收紧，回退即恢复原行为）
- 是否需要 smoke：否（UT 覆盖）

## 变更概述
serve/turn 链路对 LLM 上游鉴权类失败（401/403）快速失败：
1. 现状已确认 `RetryProvider::is_retryable_error` 对 `LlmErrorKind::Authentication` 返回 false（不重试，退避仅 429/5xx/network/timeout/stream）——本提案**不重开**该面，而是补齐其上**显式断言**与 **turn 落态摘要**；
2. turn 链路在上游鉴权失败时，turn/error 的 message 带上游错误摘要（provider 标签 + HTTP 状态 + 上游 message 截断），前端拿到明确失败而非等 30s 看门狗；
3. 不改变 failover 语义（`should_failover` 对 Authentication 仍为 true——换 lane 找有效凭据是既有正确行为）。

## 本批覆盖的 UT/ST 用例 ID（输出代码前列出，挂 S16 域，接 UT-S16-27 起）
- **UT-S16-27** — RetryProvider 对模拟 401 provider 不重试（is_retryable_error=false 且 chat 单尝试即 Err）
- **UT-S16-28** — RetryProvider 对模拟 403 provider 不重试（同上，403 路径）
- **UT-S16-29** — turn/error envelope 在上游 401 时 message 含上游摘要（provider 标签 + 状态码 + 截断正文）
与 `logos/resources/test/core-S16-test-cases.md` 对齐（本提案 merge 时追加该三行登记）。
