# 变更提案：修复 oup_client 嵌入测试错误码断言

> module: core | created: 2026-09-30

## 变更原因

`crates/octos-cli/src/commands/oup_client.rs` 的单元测试 `embedded_client_uses_real_oup_negotiation_and_rpc_errors` 断言未知方法调用应返回包含 `-32004` 的错误。但 #12（042848b5，2026-09-23）已将 `api/ui_protocol_transport.rs::route_rpc_command` 中未知方法的错误码从 `METHOD_NOT_SUPPORTED (-32004)` 改为标准 JSON-RPC `METHOD_NOT_FOUND (-32601)`，且未同步该测试。因此该测试自 #12 起稳定失败（返回 `-32601`），属于预存断言过期问题。

## 变更类型

代码级

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无
- 影响的业务场景：无
- 影响的 API：无（只改测试断言，不改 dispatch 行为）
- 影响的 DB 表：无
- 影响的编排测试：无
- 影响代码：`crates/octos-cli/src/commands/oup_client.rs`

## 部署影响

- 是否需要部署：否
- 部署原因：仅测试断言修正
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

将测试断言从检查 `-32004` 改为检查 `-32601`，并补充注释说明该错误码对应 JSON-RPC `method_not_found`，与 #12 的 dispatch 语义保持一致。不修改生产代码行为。
