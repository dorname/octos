# 实现任务

## [code] 代码实现

> 删后续自检：评分 0-7（单文件单测试断言修正，无契约变化），单切片即可自闭环。修复后 `embedded_client_uses_real_oup_negotiation_and_rpc_errors` 从 FAIL 转绿。
>
> 验证口径：复用该测试自身（无需新增 UT/ST ID）；修改后 `cargo test -p octos-cli --lib embedded_client_uses_real_oup_negotiation_and_rpc_errors` 通过，随后 `openlogos verify` 保持 Coverage 100%。

- [ ] 单切片：修正 `crates/octos-cli/src/commands/oup_client.rs` 中 `embedded_client_uses_real_oup_negotiation_and_rpc_errors` 的断言，将 `-32004` 改为 `-32601` 并补充注释说明与 #12 dispatch 语义一致；跑 `cargo test -p octos-cli --lib` 与 `openlogos verify` 验证（无新增 UT/ST ID）
