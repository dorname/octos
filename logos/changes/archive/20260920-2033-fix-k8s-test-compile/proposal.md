# 变更提案：fix-k8s-test-compile

> module: core | created: 2026-09-20

## 变更原因

外环（外环(claude)）对 k8s 系列提交（fcacde31 等）执行推送前独立复验（命令逐字取自 .github/workflows/ci.yml check job），连续拦截三类真实缺陷——此前的 openlogos verify Gate 3.6「40/40 PASS」读取的是 S16 smoke 时段的陈旧 test-results.jsonl，未覆盖这些测试目标，构成 R2 层面的验证盲区：

1. `cargo fmt --all -- --check` 失败：7 文件 rustfmt 违例（已先行代修，commit 49f152c4，纯机械重排）。
2. `cargo clippy --workspace --all-targets -- -D warnings` 失败：fcacde31 给 `registry::CreateParams` 新增 `credential: Option<CredentialKind>` 字段，但 3 处测试初始化（minimax_token.rs ×2、mod.rs ×1）未跟上 → lib test 编译破损（E0063）；编译错误清除后测试代码 lint 逐层冒出（manual_contains / search_is_some / unit_cmp / field_reassign_with_default）。
3. octos-bus 测试 `pg_persistence_matrix.rs` 的 `NEVER_PG_CATEGORIES` 常量零消费（dead_code）——消费测试内联了 4 项字面量（漏 users/tenants），与常量分叉。

另发现存量缺陷（**不在本提案修复面**，单独研判）：octos-cli 无 `api` feature 时 lib 编不过（serve.rs:13/2006 引用 `#[cfg(feature = "api")]` 的 `api`/`monitor` 模块无 cfg gate，由 607bd6c4 引入、已在 origin）；workspace 全量命令下被 feature 统一化掩盖。dorname-k8s-dev 分支从未跑过 CI，无绿基线。

## 变更类型

代码级（测试代码编译破损修复 + lint 整改；无生产路径语义变化）

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无
- 影响的业务场景：无
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无
- 代码：`crates/octos-llm/src/registry/minimax_token.rs`（×2 补 `credential: None`）、`crates/octos-llm/src/registry/mod.rs`（×1 补 `credential: None` + manual_contains）、`crates/octos-cli/src/api/ui_protocol_tests.rs`（search_is_some + unit_cmp）、`crates/octos-cli/src/config.rs`（field_reassign_with_default）、`crates/octos-bus/tests/pg_persistence_matrix.rs`（消费测试改用 NEVER_PG_CATEGORIES 常量）

## 部署影响

- 是否需要部署：否
- 部署原因：纯测试/回归面修复，不触碰生产路径
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

修复 k8s 系列提交遗留的测试编译破损与 clippy 违例：三处 `CreateParams` 测试初始化补 `credential: None`（api-key 场景语义正确——`None` 即 plain API key，minimax-token 工厂测的 X-Api-Key 路径不受影响）；四处测试代码按 clippy 建议机械整改；`test_pg_matrix_doc_never_pg_categories` 改用 `NEVER_PG_CATEGORIES` 常量（消除 dead_code + 补齐 users/tenants 断言）。验收：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、靶向测试（octos-bus/octos-cli --features api/octos-llm）全绿。
