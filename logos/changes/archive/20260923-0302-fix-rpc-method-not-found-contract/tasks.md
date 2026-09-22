# 实现任务

## [delta] 规格变更
- [x] `core-04-serve-api-design.md` 新增附录:WS AppUI RPC 错误码契约(1附.5:未知方法 -32601 / 门控已知方法 -32004)+ Admin 连接 scope 语义与 M9 fixture Stage-5 线协议(1附.6)

## [code] 代码实现
- [x] 切片 1(core 后端):`route_rpc_command` 支持方法表未命中 → `RpcError::method_not_found`(TDD:CORE-RPC-01/02)+ OpenLogos reporter
- [x] 切片 2(诊断 + 修复):nightly protocol-e2e 15 失败三层根因全部修复——
  (a) admin 连接 scope:#40 ③ 把 admin 钉到不存在的 `admin` profile → `profile_is_known` 虚拟 profile 条款 + admin 超用户显式 scope(TDD:CORE-RPC-03/04/05);
  (b) Stage-5 割接抑制 legacy 帧:M9 fixture 改发原生 v2 envelope(Basic/Slow assistant_delta 回显 prompt、ToolEvents tool_start/progress/end、Basic 持久化会话行)(TDD:CORE-RPC-06/07/08);
  (c) e2e 套件迁移到 v2 线协议:m9-ws-client 增加 `waitForTurnTerminalEnvelope`/envelope 映射,6 个 spec 文件改写。本地全套件 36 passed / 1 skipped(nightly 原 15 failed)

## [deploy] 部署任务
- [x] musl 重建注入 k8s;smoke:本地 fixture 模式 serve + WS 请求 `session/zzz-not-real` 断言 -32601;门控方法(如未协商 auxiliary 时 `session/list`)断言 -32004;admin 裸 open + fixture turn 全事件 envelope 到达(全部通过:本地 8/8 断言 + k8s WS 复验 2/2,SMOKE-RPC-01..05)
