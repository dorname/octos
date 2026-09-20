# 实现任务

## [delta] 规格变更

无（编译面修复，无规格影响）

## [code] 代码实现

- [x] `commands/mod.rs` 的 `mod serve;` 补 `#[cfg(feature = "api")]`（与既有 pub use / Command::Serve 变体 gate 对齐）+ 根因注释
- [x] `octos-ffi/src/lib.rs` 测试 `cred()` 构造补 `account_id: None`
- [x] 验证：`cargo clippy -p octos-cli --lib --no-default-features` 通过（36.9s Finished）
- [x] 验证：`cargo clippy -p octos-ffi --all-targets -- -D warnings` 通过
- [x] 验证：`cargo test -p octos-ffi` 全绿（42+17 passed）
- [ ] 验证：`cargo clippy --workspace --all-targets -- -D warnings` 不回归（后台终裁进行中）
- [ ] commit + 提案闭环（merge no-op / verify / archive）
