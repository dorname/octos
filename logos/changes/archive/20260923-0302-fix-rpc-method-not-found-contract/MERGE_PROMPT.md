# 合并指令

## 变更提案
- 提案名称：fix-rpc-method-not-found-contract
- 提案目录：logos/changes/fix-rpc-method-not-found-contract/

## 提案内容

# 变更提案：fix-rpc-method-not-found-contract

> module: core | created: 2026-09-22

## 变更原因

issue #12(nightly 17+ 连红)在 dorname 分支侧的修复(用户决策:只修 dorname,main 暂不动)。nightly protocol-e2e 最新运行(run 35739412497)15 失败 / 21 通过,失败分两类:

1. **错误码契约违背**:`m9-protocol-fault-injection.spec.ts:235` 期望未知方法(`session/zzz-not-real`)返回 JSON-RPC 标准 `-32601 method not found`,但 `route_rpc_command`(ui_protocol_transport.rs:18681)在查支持方法表时短路返回 `-32004 method_not_supported`。`UiCommand::from_rpc_request` 本会对未知方法返回 `method_not_found`(octos-core ui_protocol.rs:4366),但短路使其永远无法执行。两处均自 2026-04-30 存在,测试与服务端契约长期不一致。
2. **turn/completed 超时群**:turn/start happy path、turn/interrupt、tool events、web-client WS chat 等 14 个用例等不到 `turn/completed`/终态事件。需在 dorname 本地复现定位(main 与 dorname 差 14 个提交,部分可能已被 dorname 侧修复)。

check-windows matrix flake 已在 dorname 修复(前序工作),不在本提案范围。

## 变更类型

接口级(错误码契约矫正)+ 代码级

## 变更范围

- 影响的需求文档:无
- 影响的功能规格:`core-04-serve-api-design.md`(新增 WS RPC 错误码契约附录)
- 影响的业务场景:S05(REST API 服务与流式集成)
- 影响的 API:WS AppUI RPC 全部未知方法的错误响应码(-32004 → -32601,仅限"方法不在支持表"场景;能力门控已知方法保持 -32004)
- 影响的 DB 表:无
- 影响的编排测试:e2e/tests/m9-protocol-*.spec.ts、web-client.spec.ts(nightly protocol-e2e 套件)

## 部署影响

- 是否需要部署:是
- 部署原因:服务端行为变更需注入 k8s 验证;k8s 部署是 dorname 侧标准验证环境
- 影响环境:本地 k8s(docker-desktop ns octos)
- 是否涉及数据迁移:否
- 是否需要回滚预案:否(行为仅错误码语义矫正 + 可能的 turn 完成路径修复)
- 是否需要 smoke:是(本地起 serve + 跑 m9 协议套件子集验证 -32601;turn 超时部分视诊断结论确定)

## 变更概述

1. **契约矫正**:`route_rpc_command` 对不在 `ui_protocol_server_supported_methods()` 表中的方法返回 `RpcError::method_not_found`(-32601);`method_not_supported`(-32004)保留给"方法已知但被能力门控拒绝"的场景(strict opt-in gates / legacy header-present gates / autonomy / skill / voice 门控不变)。
2. **turn/completed 超时诊断与修复**:在 dorname 本地以 nightly 同参(fixture 模式)复现 protocol-e2e,定位 turn 不完成的根因并修复(若属代码缺陷);若 dorname 已不复现,则以本地绿证据收口。
3. TDD:先写失败测试(Rust UT 断言未知方法 → -32601;门控已知方法 → -32004 不变),再实现。


## 需要合并的 Delta 文件

### 1. deltas/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md

- Delta 文件：`logos/changes/fix-rpc-method-not-found-contract/deltas/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md`
- 目标目录：`logos/resources/prd/2-product-design/1-feature-specs/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

## 执行要求

1. 逐个 Delta 文件处理，每处理完一个报告修改摘要
2. 对于 ADDED 标记：在主文档的指定位置插入新内容
3. 对于 MODIFIED 标记：替换主文档中同名章节的内容
4. 对于 REMOVED 标记：从主文档中删除对应章节
5. 保持主文档的原有格式和风格
6. 如果主文档有"最后更新"时间戳，同步更新
7. 所有变更完成后，列出修改清单
8. 所有变更合并完成后，自动执行 git commit（告知用户，无需确认）：
   git add -A && git commit -m "docs(fix-rpc-method-not-found-contract): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive fix-rpc-method-not-found-contract`。
