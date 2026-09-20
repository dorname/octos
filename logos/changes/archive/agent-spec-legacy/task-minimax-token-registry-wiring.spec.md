spec: task
name: "task-minimax-token-registry-wiring"
tags: [llm, bugfix, critical]
---

## Intent

Fix the chat-blocking bug where every `session/open` RPC on the deployed
server fails with `-32603 failed to bootstrap ProfileRuntime for profile
'admin': failed to create LLM provider for profile 'admin': unknown
provider: minimax-token`, the WS connection closes, and the SPA surfaces
the generic "Unable to establish the UI Protocol connection" banner.

Root cause: `crates/octos-llm/src/registry/minimax_token.rs` (the MiniMax
Token Plan family, Anthropic-compatible endpoint) exists in the tree since
f4095784 but is NEVER registered — `registry/mod.rs` neither declares
`mod minimax_token;` nor lists `minimax_token::ENTRY` in `ALL`, so
`registry::lookup("minimax-token")` returns None and profile bootstrap
fails for any profile whose `llm.primary.family_id` is `minimax-token`.

修复 deployed server 上 session/open 必败的问题：minimax_token.rs 家族
文件存在但从未在 registry/mod.rs 注册（缺 `mod minimax_token;` 和 ALL
列表条目），导致 lookup 返回 None、profile bootstrap 失败、前端 WS
被断开。把该家族正式接入注册表。

## Decisions

D1: Register the family where it already lives: add `mod minimax_token;`
    and `minimax_token::ENTRY` to `ALL` in
    `crates/octos-llm/src/registry/mod.rs`, placed immediately after
    `minimax::ENTRY`. No changes to `minimax_token.rs` itself — its
    ENTRY shape already matches `ProviderEntry`.

D2: `model_catalog.json` is NOT changed: the family works without a
    catalog default row because the operator profile always carries an
    explicit `model_id` (MiniMax-M3); `ENTRY.default_model()` returning
    None only affects keyless auto-default flows, which are out of scope.

D3: Registration must not disturb name resolution order: `minimax-token`
    has empty `detect_patterns`, so `detect_provider("MiniMax-M3")` keeps
    routing to the native `minimax` family; only an explicit family_id
    selects `minimax-token`.

D4: 已确定的技术选择：
    - 只改 `registry/mod.rs` 两处（mod 声明 + ALL 条目）
    - 不改 `minimax_token.rs` 与 `model_catalog.json`
    - `detect_patterns` 保持为空，不影响 MiniMax-* 的自动探测归属

## Boundaries

### Allowed Changes

- specs/task-minimax-token-registry-wiring.spec.md
- crates/octos-llm/src/registry/mod.rs

### Forbidden

- Do NOT modify `minimax_token.rs` (its ENTRY is already correct).
- Do NOT modify `model_catalog.json` (no default row needed — D2).
- Do NOT reorder existing `ALL` entries or change any other family's
  registration, aliases, or detect_patterns.
- Do NOT change `detect_provider` semantics for `MiniMax-*` model names —
  they must keep resolving to the native `minimax` family.
- 禁止：改其他家族的注册/别名/探测规则，改 catalog，改 provider 文件。

## Completion Criteria

Rule: registry-wiring — minimax-token resolves through the registry like every other family

Scenario: Explicit lookup by canonical name resolves
  标签: critical
  Test:
    Package: octos-llm
    Filter: should_resolve_minimax_token_when_lookup_by_name
    Targets: octos_llm::registry::lookup
  Given the octos-llm registry built from `registry/mod.rs`
  When `lookup("minimax-token")` is called
  Then it returns Some entry whose `name` is exactly `minimax-token`
  And `all_names()` contains `minimax-token`

Scenario: Alias lookup resolves to the same family
  Test:
    Package: octos-llm
    Filter: should_resolve_minimax_token_when_lookup_by_alias
    Targets: octos_llm::registry::lookup alias 解析
  Given the octos-llm registry
  When `lookup("minimax-anthropic")` is called
  Then it returns Some entry whose `name` is `minimax-token`

Scenario: Provider factory builds an Anthropic-compatible provider with explicit model
  Test:
    Package: octos-llm
    Filter: should_create_provider_when_key_and_model_given
    Targets: minimax_token ENTRY.create
  Given the registered `minimax-token` entry
  When `create` is called with an API key, model `MiniMax-M3`, and no base_url override
  Then it returns Ok and the provider reports label `minimax-token`

Scenario: MiniMax model auto-detection still routes to native minimax (error path)
  Test:
    Package: octos-llm
    Filter: should_route_minimax_models_to_native_family_when_detecting
    Targets: detect_provider 路由不被新家族劫持
  Given the registry with `minimax-token` registered
  When `detect_provider("MiniMax-M3")` is called
  Then it returns `minimax`, not `minimax-token`, because the new family
    declares no detect_patterns
