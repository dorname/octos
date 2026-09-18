# 合并指令

## 变更提案
- 提案名称：chatgpt-oauth-codex
- 提案目录：logos/changes/chatgpt-oauth-codex/

## 提案内容

# 变更提案：chatgpt-oauth-codex

> module: core | created: 2026-09-18

## 变更原因
ChatGPT 订阅 OAuth token(browser PKCE / device code 登录)不携带 `api.*` scope,直连 `api.openai.com` 对每个模型都 403("Missing scopes: api.model.read")——它们只对 Codex 后端(`chatgpt.com/backend-api/codex`)有效。同时 OpenAI 设备码授权接口已切换为 JSON 版 deviceauth API(`device_auth_id`/`user_code`,二次交换 `authorization_code`+`code_verifier`),旧的 form 表单轮询已不可用。本变更让订阅登录真正可用:登录链路适配新 deviceauth API,凭证按种类路由到 Codex 后端。

## 变更类型
代码级

## 变更范围
- 影响的需求文档:无(存量功能修复+接线,无新需求文档)
- 影响的功能规格:无(认证/提供商路由属技术实现)
- 影响的业务场景:S01 ChatGPT 订阅登录与 Codex 后端对话(本变更新增场景,见 deltas/test/)
- 影响的 API:无(不改动 serve REST 面;仅 ui_protocol_transport/admin 各 +3 行接线)
- 影响的 DB 表:无
- 影响的编排测试:无
- 影响的代码:
  - `octos-cli/src/auth/oauth.rs`:device code 流适配新 deviceauth JSON API;JWT claim 解析(`chatgpt_account_id`/`chatgpt_plan_type`);独立 OS 线程上的阻塞式 token 刷新;刷新合并凭证
  - `octos-cli/src/auth/store.rs`:`AuthCredential` 新增 `account_id`
  - `octos-cli/src/config.rs`:`ResolvedCredential` 枚举(ApiKey/ChatGptOAuth)+ `resolve_credential()`(过期前 60s 自动刷新),凭证按种类分类
  - `octos-llm/src/openai_responses.rs`:Codex 后端模式(`with_chatgpt_oauth`、`apply_headers` 带 originator/chatgpt-account-id/session_id、`store:false`、`instructions`)
  - `octos-llm/src/registry/openai.rs`、`registry/mod.rs`、`registry/local.rs`:openai family 的 api_type/backend 选择接线
  - `octos-cli/src/commands/{auth,chat,init}.rs`、`gateway/profile_factory.rs`、`api/{admin,ui_protocol_transport}.rs`:凭证解析切换到 `resolve_credential`

## 部署影响
- 是否需要部署:否
- 部署原因:本地 CLI/库行为变更,无服务部署物
- 影响环境:无
- 是否涉及数据迁移:否(auth.json 新增可选字段 `account_id`,旧凭证向后兼容)
- 是否需要回滚预案:否(revert 即可)
- 是否需要 smoke:否

## UI/UX 变更声明

```yaml
ui_impact: false
design_system_mode: generated
design_system_fallback_reason: ""
pages: []
```

## 变更概述
让 `octos auth login`(OpenAI)走新版 deviceauth JSON API 完成设备码登录,从 JWT 中解析 ChatGPT 账户/计划信息存入凭证;凭证解析层把订阅 OAuth token 分类为 `ResolvedCredential::ChatGptOAuth` 并路由到 Codex 后端(`store:false`、stream-only、`instructions` 承载系统消息、Codex 路由 headers),到期前自动刷新。API key 路径行为不变。


## 需要合并的 Delta 文件

### 1. deltas/test/core-S01-test-cases.md

- Delta 文件：`logos/changes/chatgpt-oauth-codex/deltas/test/core-S01-test-cases.md`
- 目标目录：`logos/resources/test/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

## 执行要求

1. 逐个 Delta 文件处理，每处理完一个报告修改摘要
2. 对于 ADDED 标记：在主文档的指定位置插入新内容
3. 对于 MODIFIED 标记：替换主文档中同名章节的内容
4. 对于 REMOVED 标记：从主文档中删除对应章节
5. 保持主文档的原有格式和风格
6. 如果主文档有"最后更新"时间戳，同步更新
7. 所有变更完成后，列出修改清单
8. 所有变更合并完成后，自动执行 git commit（告知用户，无需确认）：
   git add -A && git commit -m "docs(chatgpt-oauth-codex): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive chatgpt-oauth-codex`。
