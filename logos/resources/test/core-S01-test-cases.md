# core-S01 ChatGPT 订阅登录与 Codex 后端对话 — 测试用例

> 场景:用户持 ChatGPT 订阅(Plus/Pro),通过 `octos auth login` 设备码登录后,凭证被分类为订阅 OAuth 并路由到 Codex 后端对话;token 到期自动刷新。
> 实现落点:`crates/octos-cli/src/auth/oauth.rs`、`crates/octos-cli/src/auth/store.rs`、`crates/octos-cli/src/config.rs`、`crates/octos-cli/src/commands/auth.rs`、`crates/octos-cli/src/commands/init.rs`、`crates/octos-llm/src/openai_responses.rs`、`crates/octos-llm/src/registry/openai.rs`。

## 单元测试

| ID | 描述 | 来源 | 前置条件 | 输入 | 预期输出 |
|---|---|---|---|---|---|
| UT-S01-01 | deviceauth 响应反序列化(device_auth_id/user_code/字符串 interval/expires_at/默认 verification_uri) | oauth.rs DeviceCodeResponse | 无 | 新版 deviceauth JSON | 字段全部正确,verification_uri 默认 codex/device |
| UT-S01-02 | `usercode` 别名兼容 + 数字 interval | oauth.rs serde alias | 无 | 含 `usercode` 的 JSON | user_code 正确解析 |
| UT-S01-03 | 二次交换响应反序列化 | oauth.rs CodeSuccessResponse | 无 | authorization_code+code_verifier JSON | 两字段正确 |
| UT-S01-04 | 从 OAuth JWT 提取 chatgpt_account_id/plan_type | oauth.rs chatgpt_account_id/chatgpt_plan_type | 无 | 含 auth claim 的 JWT | Some("acct-abc")/Some("plus") |
| UT-S01-05 | 不透明 API key 不误判为 JWT | 同上 | 无 | "sk-proj-abc123" | None/None |
| UT-S01-06 | JWT 缺 auth claim 时返回 None | 同上 | 无 | 仅 sub/iss 的 JWT | None |
| UT-S01-07 | 畸形 JWT payload 不 panic | 同上 | 无 | "header.not-base64!!!.sig" | None |
| UT-S01-08 | 刷新合并:轮换 refresh_token、重解析 account_id、过期清除 | oauth.rs merge_refreshed_credential | 旧凭证已过期 | 新 JWT + 新 refresh + expires_in 3600 | account_id 更新、未过期 |
| UT-S01-09 | 刷新响应省略 refresh_token 时保留旧值 | 同上 | 旧凭证有 refresh | refresh_token: None | 保留旧 refresh,access 更新 |
| UT-S01-10 | 订阅模型识别(gpt-5*/codex* 覆盖,gpt-4o 等排除) | openai_responses.rs is_chatgpt_subscription_model | 无 | 模型名 | 真/假判定正确 |
| UT-S01-11 | with_chatgpt_oauth 切到 Codex 后端(base_url/account/session) | openai_responses.rs with_chatgpt_oauth | 无 | account_id | base_url=CODEX_BACKEND_BASE,is_codex_backend()=true |
| UT-S01-12 | 默认构造不进入 Codex 模式 | 同上 | 无 | new() | is_codex_backend()=false |
| UT-S01-13 | Codex 请求体:store:false + instructions 承载系统消息 | openai_responses.rs build_request | Codex 模式 | 含 system 的消息列 | body.store=false,instructions 拼接正确 |
| UT-S01-14 | Codex 模式附带 reasoning summary 配置 | 同上 | Codex 模式 | 默认 config | body 含 reasoning 配置 |
| UT-S01-15 | Codex 模式请求头:originator/chatgpt-account-id/session_id | openai_responses.rs apply_headers | Codex 模式 | 请求构建器 | 三个头在场且值正确 |
| UT-S01-16 | 平台 API 不附带 Codex 头 | 同上 | 非 Codex | 请求构建器 | 无 originator/account/session 头 |
| UT-S01-17 | Codex 模式无 system 消息时 instructions 为空字符串(字段必须在场) | 同上 | Codex 模式 | 无 system 的消息列 | instructions="" |
| UT-S01-18 | 旧版凭证 JSON(无 account_id 字段)反序列化为 None,向后兼容 | store.rs AuthCredential serde | 无 | 无 account_id 的旧 JSON | account_id=None,其余字段正常 |
| UT-S01-19 | account_id 为 None 时序列化省略该字段 | store.rs serde skip_serializing_if | 无 | account_id=None 的凭证 | JSON 无 account_id 键 |
| UT-S01-20 | account_id 有值时序列化往返保留 | store.rs serde | 无 | account_id=Some 的凭证 | 往返后值不变 |
| UT-S01-21 | device_code 登录凭证分类为 ChatGptOAuth | config.rs resolve_credential | 存储 openai/device_code 凭证 | resolve | ChatGptOAuth{access_token,account_id} |
| UT-S01-22 | 浏览器 oauth 凭证分类为 ChatGptOAuth | 同上 | 存储 openai/oauth 凭证 | resolve | ChatGptOAuth |
| UT-S01-23 | paste_token 凭证分类为 ApiKey | 同上 | 存储 paste_token 凭证 | resolve | ApiKey("sk-real-key") |
| UT-S01-24 | 存储缺 account_id 时从 JWT 惰性解析回填 | 同上 | device_code 凭证无 account_id | access_token 为含 auth claim 的 JWT | account_id=Some("acct-jwt") |
| UT-S01-25 | 无存储凭证时 env_vars 中的 OPENAI_API_KEY 解析为 ApiKey | 同上 | auth_home 不存在 | env_vars 注入 key | ApiKey("sk-from-env-map") |
| UT-S01-26 | OAuth 过期且无 refresh_token 时报错并提示重新登录 | 同上 | 过期凭证无 refresh | resolve | Err 含重新登录提示 |
| UT-S01-27 | 自定义 provider 的 api_key_env 覆盖绕过 auth store | 同上 | 自定义 provider+env key | resolve | ApiKey(env 值),不读 store |
| UT-S01-28 | 60s 宽限期(REFRESH_LEEWAY_SECS)内将到期的凭证判定为需刷新 | 同上 | 凭证在宽限期内到期 | resolve | 判定为 expiring |
| UT-S01-29 | OpenAI OAuth 凭证 status 描述为订阅 | commands/auth.rs status 描述 | OAuth 凭证 | status 输出 | 含订阅描述 |
| UT-S01-30 | 过期且无 refresh 的凭证 status 标记为纯过期 | 同上 | 过期无 refresh 凭证 | status 输出 | 标记 expired(不带 auto-refresh) |
| UT-S01-31 | 将到期 OAuth 凭证 status 行标记 auto-refresh | 同上 | 将到期 OAuth 凭证 | status 输出 | 含 auto-refresh 提示 |
| UT-S01-32 | paste-token 不描述为订阅 | 同上 | paste_token 凭证 | status 输出 | 无订阅描述 |
| UT-S01-33 | 有效凭证 status 显示 active | 同上 | 未过期凭证 | status 输出 | 显示 active |
| UT-S01-34 | 订阅登录时 init 默认 OpenAI 模型为 gpt-5 | commands/init.rs 默认模型选择 | 订阅登录态 | init 默认模型 | gpt-5 |
| UT-S01-35 | 订阅凭证接受订阅模型(gpt-5.1-codex) | registry/openai.rs create | ChatGptOAuth | model=gpt-5.1-codex | 创建成功,model_id 正确 |
| UT-S01-36 | 订阅凭证拒绝平台专属模型,错误提示 subscription/platform | 同上 | ChatGptOAuth | model=gpt-4o | Err 含 gpt-4o/subscription/platform |
| UT-S01-37 | 订阅凭证拒绝纯推理模型(o3) | 同上 | ChatGptOAuth | model=o3 | Err 含 o3 |
| UT-S01-38 | 订阅凭证未指定模型时默认 gpt-5 | 同上 | ChatGptOAuth | model=None | model_id="gpt-5" |
| UT-S01-39 | ApiKey 凭证保留平台模型可用(行为不变) | 同上 | ApiKey | model=gpt-4o | 创建成功 |
| UT-S01-40 | 显式 base_url 覆盖优先于订阅路由(不强制 Codex 后端) | 同上 | ChatGptOAuth+base_url | model=gpt-4o,自定义 base_url | 创建成功,尊重覆盖 |

映射(实现函数 ↔ ID):oauth.rs `test_deserialize_device_code_response`=UT-S01-01,`test_deserialize_device_code_response_with_usercode_alias`=UT-S01-02,`test_deserialize_code_success_response`=UT-S01-03,`should_extract_account_id_and_plan_from_oauth_jwt`=UT-S01-04,`should_return_none_for_opaque_api_key`=UT-S01-05,`should_return_none_when_auth_claim_missing`=UT-S01-06,`should_return_none_for_malformed_jwt_payload`=UT-S01-07,`should_merge_refreshed_token_into_existing_credential`=UT-S01-08,`should_keep_old_refresh_token_when_response_omits_one`=UT-S01-09;openai_responses.rs `should_recognize_chatgpt_subscription_models`=UT-S01-10,`should_switch_to_codex_backend_with_chatgpt_oauth`=UT-S01-11,`should_not_be_codex_backend_on_default_construction`=UT-S01-12,`should_build_codex_request_with_store_false_and_instructions`=UT-S01-13,`should_add_reasoning_summary_in_codex_mode`=UT-S01-14,`should_apply_codex_headers_in_codex_mode`=UT-S01-15,`should_not_apply_codex_headers_for_platform_api`=UT-S01-16,`should_emit_empty_instructions_when_codex_request_has_no_system_message`=UT-S01-17;store.rs `should_default_account_id_to_none_for_legacy_credentials`=UT-S01-18,`should_omit_account_id_when_none`=UT-S01-19,`should_roundtrip_account_id_when_present`=UT-S01-20;config.rs `should_classify_device_code_credential_as_chatgpt_oauth`=UT-S01-21,`should_classify_browser_oauth_credential_as_chatgpt_oauth`=UT-S01-22,`should_classify_paste_token_as_api_key`=UT-S01-23,`should_lazily_parse_account_id_from_jwt_when_stored_field_missing`=UT-S01-24,`should_resolve_env_vars_map_as_api_key_when_no_stored_credential`=UT-S01-25,`should_error_with_relogin_hint_when_oauth_expired_without_refresh_token`=UT-S01-26,`should_bypass_auth_store_for_custom_api_key_env_override`=UT-S01-27,`should_detect_expiring_credentials_within_leeway`=UT-S01-28;commands/auth.rs `should_describe_subscription_for_openai_oauth_credential`=UT-S01-29,`should_mark_expired_without_refresh_as_plain_expired`=UT-S01-30,`should_mark_expiring_oauth_as_auto_refresh_in_status_line`=UT-S01-31,`should_not_describe_subscription_for_paste_token`=UT-S01-32,`should_show_active_for_valid_credential`=UT-S01-33;commands/init.rs `should_default_openai_model_to_gpt5_for_subscription_login`=UT-S01-34;registry/openai.rs `should_accept_subscription_model_with_chatgpt_oauth`=UT-S01-35,`should_reject_platform_only_model_with_chatgpt_oauth`=UT-S01-36,`should_reject_reasoning_only_model_with_chatgpt_oauth`=UT-S01-37,`should_default_to_gpt5_with_chatgpt_oauth_when_no_model`=UT-S01-38,`should_keep_platform_models_with_api_key`=UT-S01-39,`should_respect_base_url_override_even_with_chatgpt_oauth`=UT-S01-40。

## 场景测试

| ID | 描述 | 覆盖 Steps | 前置条件 | 操作序列 | 预期结果 |
|---|---|---|---|---|---|
| ST-S01-01 [manual] | 完整设备码登录 → Codex 对话端到端 | 登录→凭证落盘→分类→Codex 请求→刷新 | 真实 ChatGPT 订阅账号、可开浏览器/访问设备页 | 1. `octos auth login` 选 OpenAI 设备码流;2. 设备页输入 user_code;3. `octos auth status` 查看;4. `octos chat` 用 gpt-5 系模型发一句话;5. 等待 token 过期后再对话触发刷新 | 登录成功且 status 显示 plan/account;对话走 Codex 后端成功返回;刷新后无需重新登录 |

> ST-S01-01 需真实 OAuth 账号与浏览器交互,CI 无头环境不可自动断言,按规则标 [manual],不计入覆盖率分母。
