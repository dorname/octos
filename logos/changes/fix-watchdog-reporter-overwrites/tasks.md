# 实现任务

## [code] 代码实现

> 删后续自检：评分 0-7（单文件单路径修复，无契约变化），单切片即可自闭环。修改后 `cargo test -p octos-cli` 不再清空 `test-results.jsonl`，全量 `openlogos verify` 仍可保持 Coverage 100%。
>
> 验证口径：复用既有 S17 watchdog 用例（无需新增 UT/ST ID）；修改后用 `cargo test -p octos-cli --lib` 触发并确认 jsonl 仍保留其他场景记录，随后 `openlogos verify` 全绿。

- [ ] 单切片：修改 `crates/octos-cli/src/commands/watchdog/tests.rs` 的 reporter 初始化逻辑，由无条件 `fs::write(path, b"")` 改为「目录不存在则创建，账本文件不存在则创建空文件，已存在则保留追加」；补同文件 UT 确保 reporter 正常写入；跑 `cargo test -p octos-cli --lib` 与 `openlogos verify` 验证（复用 S17 既有用例，无新增 UT/ST ID）
