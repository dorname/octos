# Delta: prd/2-product-design/1-feature-specs — core-04-serve-api-design.md

> target: logos/resources/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md

## ADDED — 1附.2 Profile LLM 运行时热加载语义

### 1附.2 Profile LLM 运行时热加载语义

AppUI `profile/llm/select`、`profile/llm/upsert`、`profile/llm/delete` 三个 mutation 持久化提交后，运行时转换对**所有** profile 一视同仁——不再区分「启动期钉住（startup-pinned）」与「动态」两类：

1. **驱逐**：会话运行时缓存按 profile 失效（`invalidate_profile`）；
2. **代际守卫**：动态 ProfileRuntime 缓存的代际先递增再清槽，使在途的旧配置引导在插入时被拒（#2164 机制复用到启动 profile）；
3. **重建**：从 profile store 的已提交文件重引导 `ProfileRuntime`（纯配置快照，幂等），写入动态缓存——`resolve_session_profile_runtime` 的动态优先语义使下一轮对话即时命中新模型，无需重启进程。

`state.profiles` 启动快照保持不可变，仅充当：(a) 无任何 LLM mutation 发生时的引导兜底；(b) steer sweep 等需要枚举 profile 的场景源。一旦某 profile 发生过 LLM mutation，其运行时真相移交动态缓存。

**响应契约**：三个 mutation 的响应仍携带 `restart_required` 字段（恒在，客户端不得以字段缺失作判断），但启动 profile 场景不再出现 `restart_required=true`；转换结果（`disposition`）取值收窄为 `reloaded` / `deferred` / `persisted_but_not_live`（`restart_required` disposition 退役，仅历史兼容保留枚举）。`runtime_policy_stamp` 的 `model`/`provider` 按「动态缓存 → 启动快照」的真实服务优先级报告，与下一轮实际服务的模型一致。

#### 验收条件（交互级）

##### 正常：启动 profile 切换模型即时生效
- **GIVEN** `octos serve` 运行中，目标 profile 为启动期钉住（来自启动配置）
- **WHEN** 客户端调用 `profile/llm/select`（或 upsert）切换主模型并成功提交
- **THEN** 响应 `restart_required=false`、`disposition=reloaded`；同一 profile 的下一轮对话实际由新模型服务（stamp 与 LLM 请求均命中新模型），进程未重启

##### 正常：无 mutation 的冷启动解析不重复引导
- **GIVEN** serve 刚启动，某启动 profile 从未发生过 LLM mutation
- **WHEN** 该 profile 的会话解析运行时
- **THEN** 直接使用启动快照，不触发额外的 ProfileRuntime 引导（无双倍内存/双重 redb 占用）

##### 异常：重建与并发提交竞争
- **GIVEN** 一次 LLM mutation 提交后运行时在重建中
- **WHEN** 第二次 mutation 在重建完成前提交
- **THEN** 代际守卫拒绝在途旧 runtime 写入缓存；最终缓存中的 runtime 来自最后一次提交的文件；客户端收到可读错误并被提示重试，不会静默服务旧模型

##### 异常：重建失败不破坏现状
- **GIVEN** mutation 已持久化但新配置无法引导（如凭证缺失）
- **WHEN** 提交后转换尝试重建
- **THEN** 响应 `disposition=persisted_but_not_live` 并附可读错误；旧运行时已被驱逐，下一轮对话重试引导并呈报同一错误，而非静默回落旧模型
