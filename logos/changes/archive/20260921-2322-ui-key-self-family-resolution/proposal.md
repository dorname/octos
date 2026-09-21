# 变更提案：ui-key-self-family-resolution

> module: core | created: 2026-09-21 | operator 裁决：仅治本 a′

## 变更原因
黑板条目 #35（洋葱第 6 层，#34 定案）。operator 真机 22:37:26「你好」→ **401**（pod
`z9h4h`,a296f9b0 上的最新结果）。复现：WS probe `session/open profile_id=admin →
turn errored`，错误逐字一致（minimax-token/MiniMax-M3 401 X-Api-Key 未携带）。

**根因**：UI 保存写 admin.json 时**命名自不一致**——路由 `api_key_env=ANTHROPIC_API_KEY`
（按 api_type=anthropic 取名），而 key 值存 `env_vars.MINIMAX_API_KEY`（族规范名）。
`apply_admin_llm_key_fallback` 首行排除 admin 作为回退目标（admin 是回退 SOURCE）→
**admin 自链解析不到 key**（路由名 ANTHROPIC_API_KEY 在 admin.env_vars 缺失，进程 env
又是 REPLACE_ME 占位）→ 401。cluster-worker 不受影响（#33 跨 profile 回退命中）。

## 变更类型
代码级（治本 a′：服务端自体 by-family key 解析，不改对外契约形状）。

## 变更范围
- 影响的功能规格：ProfileRuntime bootstrap 的 key 解析（`runtime/profile.rs`）
- 影响的业务场景：任何 profile（含 admin）路由声明的 env 名与自身 env_vars 存储名
  按族错位时，key 仍可解析（UI 保存命名不一致的防御）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：runtime::profile::（UT-S16-57 起）

## 部署影响
- 是否需要部署：是（operator 已授权本提案全链含部署注入；验收门见下）
- 影响环境：k8s 集群
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：是（验收门：WS probe session/open profile_id=admin → turn/start →
  turn_terminal outcome=completed 无 401）

## 变更概述（方案 a′)
ProfileRuntime bootstrap 的 key 解析处新增**自体 by-family 解析**：对**任何 profile
（含 admin）**，若路由声明的 `api_key_env`（`config.api_key_env` 或 provider 默认）在该
profile **自身 env_vars** 中缺失/占位（REPLACE_ME/空），但**同族候选名**（registry
`ENTRY.api_key_env` 规范名 + `key_env_aliases`，去重，路由名优先）在自身 env_vars 中
持有真实 key → 注入 `config.env_vars[路由名]`。

### 语义（严格保留）
1. **先于/独立于 #23 跨 profile 回退**，admin 不再被排除（自体解析对 admin 生效）；
2. **显式 key 优先不变**（自身路由名持有真实 key 则不注入）；
3. 成功注入 `tracing::warn!`（带命中候选名+族名，**不泄 key 值**）；
4. **只注入内存 Config**，不写 seed/override（#13/#15/#20 语义）；
5. **#23/#31 跨 profile 回退原样保留**（自身 env_vars 无 key 时仍走 admin 回退）；
6. 真机案例即 admin.json：路由 ANTHROPIC_API_KEY vs 存 MINIMAX_API_KEY（minimax-token 族）。

### 与 #31 的关系
#31 是**跨 profile**（消费 profile ← admin）by-family；本提案是**自体**（profile 自身
env_vars 内 by-family 错位）。自体解析先跑，admin 自身错位自此可解；跨 profile 回退
仍在（自身无 key 时兜底）。两者并存，语义正交。

## 验收门
部署后 WS probe `session/open profile_id=admin → turn/start → turn_terminal
outcome=completed` 无 401（今日 22:5x 同法复现为 errored）。

## 测试
core-S16-test-cases.md 登记 UT-S16-57 起（admin 自身名不匹配→注入 / admin 自身名匹配
→no-op / 自身无任何族候选→不注入错误照常下抛 / 占位跳过），reporter 写
test-results.jsonl，全部 hermetic（仅 test-only 环境名，禁读真实进程 env，#26 纪律）。
