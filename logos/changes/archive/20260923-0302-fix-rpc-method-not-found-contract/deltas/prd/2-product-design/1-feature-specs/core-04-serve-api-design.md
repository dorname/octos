# Delta: prd/2-product-design/1-feature-specs — core-04-serve-api-design.md

> target: logos/resources/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md

## ADDED — 1附.5 WS AppUI RPC 错误码契约（未知方法 vs 能力门控）

### 1附.5 WS AppUI RPC 错误码契约（未知方法 vs 能力门控）

AppUI WS RPC(`route_rpc_command` 分发）对方法级拒绝必须区分两种语义，错误码不可混用：

1. **未知方法 → `-32601 method not found`**:method 不在服务端支持方法表（`ui_protocol_server_supported_methods()`）中，即"本服务器根本不认识此方法"。响应 error message 必须回显方法名。这是 JSON-RPC 2.0 标准码，客户端据此判定协议/版本不匹配。
2. **已知方法被能力门控 → `-32004 method not supported`**:method 在支持表中，但连接未协商对应能力（strict opt-in gates、legacy header-present gates、autonomy / skill / voice 门控），即"服务器认识此方法，但本会话无权使用"。客户端据此回落 REST 或提示开启能力。

禁止用 `-32004` 回应未知方法：客户端会把"协议不匹配"误判为"能力未开"，走入错误的回落路径。

#### 验收条件（交互级）

##### 正常：未知方法返回 -32601
- **GIVEN** AppUI WS 连接已建立（任意能力协商状态）
- **WHEN** 客户端请求服务端支持表中不存在的方法（如 `session/zzz-not-real`)
- **THEN** 返回 error code `-32601`,message 中含该方法名

##### 正常：门控已知方法返回 -32004
- **GIVEN** AppUI WS 连接未协商 `auxiliary_rest_to_ws_v1` 能力
- **WHEN** 客户端请求支持表中存在但被 strict opt-in 门控的方法（如 `session/list`)
- **THEN** 返回 error code `-32004`（非 -32601)

##### 异常：能力协商后门控解除
- **GIVEN** 同一连接重新协商并携带对应能力
- **WHEN** 再次请求该方法
- **THEN** 正常路由执行，不再返回 -32004

## ADDED — 1附.6 Admin 连接 scope 语义与 M9 fixture 的 Stage-5 线协议

### 1附.6 Admin 连接 scope 语义与 M9 fixture 的 Stage-5 线协议

**Admin 连接 scope(#40 ③ 的补全）**:admin token 的 WS 连接 scope 钉在虚拟 profile `admin` 上（该 id 永不存在于 profile store——auth 层为 `AuthIdentity::Admin` 合成）:

1. `admin` 视为**已知虚拟 profile**:`session/open` 不得因 store 无 `admin` 行而报 `profile_unresolved`；其会话由 admin 管理器合法持有，turn 回落到 serve 引导运行时（单机 `--provider` 配置）。
2. Admin 是 profile **超用户**（与 REST `is_authorized_for_profile` 对齐）：连接显式请求其他 profile scope 时不得触发 authenticated-scope-mismatch 的 1008 关闭；被请求的 profile 成为 active profile。非 admin 的 user 连接跨 profile 行为不变（仍拒绝 + 1008)。

**M9 fixture 的 Stage-5 线协议**(`OCTOS_M9_PROTOCOL_FIXTURES=1` 专用测试面）:Stage-5 割接后 legacy `message/delta`/`tool/*`/`turn/completed` 帧对所有连接抑制，fixture 必须只发**规范 v2 envelope**:

1. Basic/Slow:assistant 内容以 `assistant_delta` envelope 投递；Basic **回显 prompt**（确定性内容，兼测 CJK/字节边界传输保真）。
2. ToolEvents：以原生 `tool_start`/`tool_progress`/`tool_end` envelope 投递，tool_call_id 相关。
3. Basic 持久化 user prompt + assistant 回显到 standalone 会话管理器（thread 盖章，走规范 commit 路径）——`session/messages_page` 对 fixture 会话可读。
4. turn 终态以 `turn_terminal` envelope(`outcome: completed|interrupted|errored`）投递。

#### 验收条件（交互级）

##### 正常：admin 连接裸 session/open 成功
- **GIVEN** serve 以 admin token 启动且 profile store 为空（CI 形态）
- **WHEN** 客户端不带 profile_id 调用 `session/open`
- **THEN** 返回 opened envelope(active_profile_id=admin)，不报 profile_unresolved

##### 正常：admin 连接显式 profile scope 不被 1008 关闭
- **GIVEN** 同上连接
- **WHEN** `session/open` 带显式 profile_id
- **THEN** 不触发 1008 auth_expired 关闭；被请求 profile 成为 active

##### 正常：fixture turn 全事件以 envelope 到达
- **GIVEN** fixture 模式 serve
- **WHEN** 客户端 `turn/start`(Basic/ToolEvents)
- **THEN** 收到 `turn/started` → `assistant_delta`/`tool_*` envelope → `turn_terminal` envelope；刷新重放（replay）同样可见；`session/messages_page` 返回持久化的 user/assistant 行
