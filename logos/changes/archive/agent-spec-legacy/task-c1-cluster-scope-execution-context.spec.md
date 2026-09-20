spec: task
name: "统一 Scope 与可信 ExecutionContext（cluster-scope-execution-context）"
tags: [runtime, cluster, tenancy, scope, octos-core]
estimate: 2w
---

- **task**: c1-cluster-scope-execution-context
- **status**: proposed
- **phase**: P1（持久化边界前置：统一作用域是所有 repository 的键设计前提）
- **ADR**: docs/adr/cluster-state-and-execution.md（D2 完整 Scope）
- **验收矩阵**: K01（幂等提交）、K07（跨租户隔离）

## Intent

当前运行时的身份标识是 wire session ID 加 profile 目录约定：客户端提交的
session ID 直接进入文件路径、广播 channel 名、缓存键和审批记录，没有
租户概念，也没有服务端分配的 workspace 身份。两个部署共用相同 wire
session ID 与相同相对 cwd 时，存储、广播、artifact 与审批会互相串用。

本任务在 `octos-core` 与 OUP transport/scope 层引入不可歧义的内部作用域
`Scope = tenant_id + profile_id + workspace_id + session_id` 与
`Execution = Scope + thread_id + run_id + attempt_id`，并让可信
ExecutionContext 沿全部事件路径（查询、缓存、广播、审批、工具、对象
存储路径、审计）传递。wire session ID 在服务端入口一次性绑定到 Scope，
之后不再作为任何持久化或隔离键直接出现。

## Decisions

1. **Scope 是不可变值对象**（octos-core）：四个分量均为服务端校验过的
   标识；`tenant_id`/`profile_id` 来自认证身份推导，`workspace_id` 是
   服务端分配的逻辑身份（拒绝把客户端 cwd 字符串当身份），`session_id`
   保留旧 wire 值但仅作分量之一。cwd 只是 runner 内的映射位置。
2. **Execution 派生自 Scope**：`thread_id`/`run_id`/`attempt_id` 由
   服务端生成；一次 turn/start 创建或复用 run 时绑定 Execution。子任务
   /peer 使用独立 run/子作用域，合并走持久化 join（本任务只定义类型与
   传递，join 语义归 c3）。
3. **可信 ExecutionContext 为唯一授权载体**：API/WS 入口认证后构造
   ExecutionContext 并注入请求处理；内部模块不得从全局状态或客户端
   payload 重新推导 tenant/profile。控制命令（interrupt、approval、
   steer）携带同一 Execution 的独立可消费通道标识。
4. **OUP 变更走 UPCR**：turn/start accepted 响应新增 run_id/scope 字段、
   scope 绑定语义变化，均须先补 UPCR 文档与兼容测试，再改服务端
   （scripts/check-ui-protocol-upcr.sh 覆盖规则适用）。
5. **Scope 键贯穿存储设计**：c2 的 PG schema 中所有租户业务表以
   tenant_id 起首的复合键建模；本任务先落地 Rust 类型、入口绑定与
   事件路径传递，数据库建模在 c2 完成。
6. **默认单 owner 语义保留**：一个 Scope 同时只有一个主执行 owner
   （延续 SessionActor 串行输入语义）；并行 thread 用独立 run，不在
   本任务引入多写者。

## Boundaries

### Allowed Changes

- specs/task-c1-cluster-scope-execution-context.spec.md
- crates/octos-core/（Scope/Execution/ExecutionContext 类型与校验）
- crates/octos-cli/src/api/（入口认证后绑定 ExecutionContext）
- crates/octos-cli/src/runtime/（runtime cache 键切换到 Scope）
- crates/octos-bus/src/（session/channel 路径携带 Scope）
- api/（UPCR 文档：accepted 响应 run_id/scope 字段）
- crates/octos-cli/tests/、crates/octos-core/tests/（本任务新测试）

### Forbidden

- 不引入 PostgreSQL 依赖、不改任何持久化格式（归 c2）
- 不实现租约、队列或恢复语义（归 c3）
- 不改插件 manifest、MCP、工厂任何代码（下一目标）
- 不删既有测试断言换绿；不改权限/凭据；不 push/PR

## Acceptance Criteria

### Rule: scope-binding — wire 身份在入口一次性绑定到 Scope

Scenario: 同一 wire session 在两个租户下解析为不同 Scope（critical）
  Tags: critical, K07
  Test:
    Package: octos-cli
    Filter: scope_binding_distinguishes_tenants_with_same_wire_session
  Given 两个租户各自以相同 wire session ID 与相同相对 cwd 建立连接
  When 入口认证并绑定 Scope
  Then 两个 ExecutionContext 的 Scope 不同（tenant_id 不同），后续缓存键、
       广播 channel 与审批记录互不命中

Scenario: 客户端提交的 workspace 字符串不被当作身份
  Test:
    Package: octos-cli
    Filter: scope_binding_rejects_client_supplied_workspace_identity
  Given 请求携带自制 workspace 路径/cwd 字符串
  When 入口绑定 Scope
  Then workspace_id 为服务端分配值，cwd 仅记录为 runner 映射位置，
       不进入任何持久化键

Scenario: 未认证请求不构造 Scope
  Test:
    Package: octos-cli
    Filter: scope_binding_requires_authenticated_identity
  Given 无有效凭据的请求
  When 到达 Scope 绑定
  Then 拒绝且不产生任何 ExecutionContext

### Rule: execution-context-propagation — 授权载体沿事件路径传递

Scenario: 工具调用携带发起 run 的 Execution（critical）
  Tags: critical, K07
  Test:
    Package: octos-cli
    Filter: tool_invocation_carries_originating_execution_context
  Given 租户 A 的 run 触发工具调用
  When 工具执行读取其上下文
  Then 得到的 Execution 与发起 run 一致；工具侧无法从全局状态推导出
       另一个租户的 Scope

Scenario: 控制命令与主执行共用 Execution 且独立送达
  Test:
    Package: octos-cli
    Filter: control_command_channel_is_independent_of_llm_call
  Given 一个 run 正在执行长 LLM 调用
  When 同 Scope 提交 interrupt
  Then interrupt 不排在 LLM 调用之后，可立即被消费，且携带同一
       Execution 标识

### Rule: oup-compat — 协议可见变更走 UPCR

Scenario: accepted 响应携带 run_id/scope 且有 UPCR 记录
  Test:
    Package: octos-cli
    Filter: upcr_document_exists_for_accepted_run_id_field
  Given turn/start accepted 响应新增 run_id/scope 字段
  When 运行 scripts/check-ui-protocol-upcr.sh
  Then 存在对应 UPCR 文档且兼容测试通过

### Rule: idempotent-submit — 幂等键语义定义（存储在 c2 落库）

Scenario: 同一幂等键不同 payload 判冲突（critical）
  Tags: critical, K01
  Test:
    Package: octos-core
    Filter: idempotency_key_conflict_on_different_payload
  Given 一个 request 的幂等键与 payload_hash 组合
  When 同键不同 payload 再次提交
  Then 判为冲突（类型级判定函数）；同键同 payload 判为重放
