# 实现任务

## [delta] 规格变更

无（测试/回归面修复，无规格影响）

## [code] 代码实现

- [x] 三处 `CreateParams` 测试初始化补 `credential: None`（minimax_token.rs ×2、registry/mod.rs ×1）
- [x] `registry/mod.rs` manual_contains 整改（`iter().any` → `contains`）
- [x] `ui_protocol_tests.rs` search_is_some（`find().is_none()` → `!contains`）+ unit_cmp（unit 断言改为直接 await）
- [x] `config.rs` field_reassign_with_default（初始化器内联 `api_key_env`）
- [x] `pg_persistence_matrix.rs` 消费测试改用 `NEVER_PG_CATEGORIES` 常量
- [x] `cargo fmt --all -- --check` 通过（fmt 违例已由 49f152c4 先行修复）
- [x] `cargo clippy --workspace --all-targets -- -D warnings` 通过（cargo clean 后 CI 等价全量终裁 exit 0）
- [x] 靶向测试通过（octos-bus 10/10；octos-llm 716+；octos-cli lib 3840/3846，6 失败均为 root 环境噪声——root 无视权限 mock + /root home 触发 root_escape，CI 非 root 不复现）
- [x] 附带整改：`test_k8s_manifest_binary_path_consistent` 与 `test_init_script_documents_lazy_migration` 断言过时（#2436 架构演进后从未真正跑过）——分别更新为 init-wget/emptyDir 落位断言与 attach 时机断言（后者更名 `..._pg_migration_timing`），commit 0b7ad0c9

## 关联研判（不在本提案修复面）

- octos-cli 无 `api` feature 时 lib 编不过（serve.rs 引用 gated 模块无 cfg gate；607bd6c4 引入，存量）——待终裁结果出来后单独提案或上报
