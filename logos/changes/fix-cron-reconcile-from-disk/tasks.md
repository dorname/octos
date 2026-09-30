# 实现任务

## [code] 代码实现

> 删后续自检：评分 0-7（2 个相关文件内单路径 bugfix，无契约变化），单切片即可自闭环。修复后 `toggle_via_service_adopts_external_writes_instead_of_erasing_them` 从 FAIL 转绿。
>
> 验证口径：复用既有 S06 cron 用例（无需新增 UT/ST ID）；修改后 `cargo test -p octos-bus` 与 `cargo test -p octos-cli --lib` 的 cron_panel 相关测试全绿，随后 `openlogos verify` 保持 Coverage 100%。

- [ ] 单切片：在 `crates/octos-bus/src/local_cron_store.rs` 新增包内重载方法（复用 `load_store_or_quarantine`），在 `crates/octos-bus/src/cron_service.rs::toggle_job_reconciling` 锁内先重载磁盘再查找/更新；同步修正 `crates/octos-cli/src/api/cron_panel.rs` 中 `toggle_via_service_adopts_external_writes_instead_of_erasing_them` 的断言；跑 `cargo test -p octos-bus`、`cargo test -p octos-cli --lib` 与 `openlogos verify` 验证（复用 S06 既有用例，无新增 UT/ST ID）
