# 变更提案：admin-key-fallback-test-hermeticity

> module: core | created: 2026-09-21

## 变更原因
黑板条目 #26（修复 #25 复验异议）。#25 外环在**带真实 `ANTHROPIC_API_KEY` 的 shell**
重跑，26/28——UT-S16-47 与 `skips_admin` 两用例被**进程 env 污染**：
`apply_admin_llm_key_fallback` 内部用 `std::env::var(key_var)` 判「消费 profile 是否
已有显式 key」，而测试把 `key_var` 设为真实名 `ANTHROPIC_API_KEY`，进程 env 里一旦
有真实 key，「缺失 / 占位」场景就被判成「显式 key 在」→ 不回退 → 断言失败。
这是**测试密闭性缺陷**，生产代码逻辑无误（生产环境进程 env 有 key 本就应判显式优先）。

## 变更类型
测试级（密闭性修复；**生产代码零改动**）。

## 变更范围
- 影响的功能规格：无（仅 `runtime::profile::tests` 的 5 个 fallback 用例）
- 影响的业务场景：无运行时行为变更
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：UT-S16-47..51（改）+ UT-S16-52（新增密闭性回归）

## 部署影响
- 是否需要部署：否（测试层修复，不动 binary）
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否（CI/复验跑测试即可）

## 变更概述
把 5 个 fallback 用例的 key env 名从真实 `ANTHROPIC_API_KEY` 改为**不可能存在于
进程 env 的专有变量名** `OCTOS_TEST_FB_KEY_23`（并 `std::env::remove_var` 防御性
清除 + 用同一变量名贯穿 config/admin_env/断言）。这样
`apply_admin_llm_key_fallback` 内的 `std::env::var(key_var)` 永远读不到进程环境
的真实 key，「缺失 / 占位 / 显式」场景完全由测试注入的 `env_vars` 决定，与外部环境
无关。

### 修法选型（理由）
- **专有变量名（采纳）**：`OCTOS_TEST_FB_KEY_23` 不会出现在任何真实部署的进程 env，
  密闭性由「名字不可能冲突」保证，无需锁、无需串行化，并行安全，最简单可靠。
- **测试内临时清除/覆盖真实 env + serial 锁（弃）**：`std::env::set_var/remove_var`
  是进程全局、并行测试间会互相污染（需 serial 锁），且在带真实 key 的 shell 里
  「测完恢复」易漏，脆弱。

## 验收硬标准
在**导出 `ANTHROPIC_API_KEY=sk-real`** 的 shell 里重跑 `profiles::` 与
`runtime::profile::` 全绿（外环将以此环境复验）。
