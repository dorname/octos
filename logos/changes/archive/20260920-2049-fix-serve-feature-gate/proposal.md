# 变更提案：fix-serve-feature-gate

> module: core | created: 2026-09-20

## 变更原因

外环推送前复验（fix-k8s-test-compile 期间的关联研判，黑板 2026-09-20 行动注记遗留 ①）发现的存量缺陷：octos-cli 的 `commands/mod.rs` 中 `pub use serve::ServeCommand` 与 `Command::Serve` 枚举变体均已加 `#[cfg(feature = "api")]`，但 **39 行的 `mod serve;` 模块声明本身未 gate**——api 关闭时 serve.rs 仍被编译，其 `use crate::api::{...}`（serve.rs:13）与 `use crate::monitor::{...}`（serve.rs:2006）引用两个 api-gated 模块导致 46 个编译错误。该缺陷由 607bd6c4 引入（已在 origin），workspace 全量命令下被 feature 统一化掩盖，但任何以 `default-features = false` 依赖 octos-cli 的消费者（如 octos-ffi）单独构建即炸。连带修复：octos-ffi 测试的 `AuthCredential` 构造缺 `account_id` 字段（chatgpt-oauth-codex/S01 系列加字段后 ffi 测试未跟上，lib test 编译破损——同为 verify 陈旧产物盲区受害者）。

## 变更类型

代码级（feature gate 补齐 + 测试构造体字段补齐；无生产路径语义变化）

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无
- 影响的业务场景：无
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无
- 代码：`crates/octos-cli/src/commands/mod.rs`（`mod serve;` 补 `#[cfg(feature = "api")]` + 注释说明根因）、`crates/octos-ffi/src/lib.rs`（测试 `cred()` 构造补 `account_id: None`）

## 部署影响

- 是否需要部署：否
- 部署原因：编译面修复，无运行时行为变化
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

给 `mod serve;` 声明补上与其 pub use / 枚举变体一致的 `#[cfg(feature = "api")]` gate（api 关闭时不再编译 serve.rs，46 个 E0432/E0609 类错误消失）；octos-ffi 测试构造体补 `account_id: None`（`Option<String>`，paste_token 场景语义正确）。验收：`cargo clippy -p octos-cli --lib --no-default-features` 通过、`cargo clippy -p octos-ffi --all-targets -- -D warnings` 通过、`cargo test -p octos-ffi` 全绿（59 用例）、workspace 全量 clippy 不回归。
