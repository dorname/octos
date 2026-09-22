# 变更提案：fix-startup-profile-llm-hot-reload

> module: core | created: 2026-09-22

## 变更原因
用户反馈：在 web 界面（AppUI）配置/切换 LLM 后，K8s 后端的 `octos serve` 实际请求仍走旧模型——配置已持久化但运行时不生效，必须重启进程。根因是启动期钉住（startup-pinned）profile 的 LLM 运行时存在**三层锁**：

1. **提交层** `commit_profile_llm_runtime_transition`（ui_protocol_transport.rs:13905）：`startup_pinned = state.profiles.contains_key(profile_id)`（13910）为真时直接返回 `RestartRequired`（13924），不走动态重建路径；
2. **解析层** `ensure_session_profile_runtime`（21783）：动态缓存未命中后，`state.profiles.get(profile_id)`（21810）短路返回启动快照，永远轮不到从 profile store 文件重引导（21820 之后的代际守卫重建逻辑）；
3. **读取层** `resolve_session_profile_runtime`（21693）：`dynamic.or_else(state.profiles.get)`——动态映射优先，但第 1 层从不为启动 profile 写入动态映射，回退永远拿到旧快照。

矛盾已核实可解：
- mutation 持久化统一走 `store.save_with_merge`（13285），启动 profile 的新配置**已经落在 store 文件里**，重引导所需输入齐备；
- `ProfileRuntime`（runtime/profile.rs:514）是纯配置快照（provider 链 + 工具注册表 + 提示词），无长驻进程所有权，重建幂等，旧 Arc 随会话缓存驱逐自然释放；
- 代际守卫（`bump_profile_runtime_generation` / `insert_profile_runtime_if_current`，21742-21781）与 bootstrap 互斥锁（21721）已为动态 profile 解决了并发重建竞争，机制可直接复用到启动 profile。

## 变更类型
代码级（行为契约变化：`restart_required` 语义收窄——启动 profile 的 LLM mutation 从"保存但必须重启"变为"保存并即时热加载"）。

## 变更范围
- 影响的需求文档：无（行为修复，不新增需求）
- 影响的功能规格：`logos/spec/` 中 profile LLM 运行时转换语义（#2164 奠底的 `Reloaded / RestartRequired / Deferred / PersistedButNotLive` 四态中，启动 profile 不再产生 `RestartRequired`）
- 影响的业务场景：AppUI `profile/llm/select`、`profile/llm/upsert`、`profile/llm/delete` 三个 mutation 的提交后转换
- 影响的 API：`profile/llm/select` / `profile/llm/upsert` / `profile/llm/delete` 的响应字段 `restart_required`（启动 profile 场景从 true 变为 false，新增 `Reloaded` 路径）
- 影响的 DB 表：无
- 影响的编排测试：无（API 面不变，仅字段值语义变化）

## 部署影响
- 是否需要部署：是
- 部署原因：K8s 后端 `octos serve` 是用户痛点现场，修复后需重建二进制并滚动 pod 验证热加载真机闭环
- 影响环境：本地（docker-desktop k8s `octos` ns）
- 是否涉及数据迁移：否
- 是否需要回滚预案：否（代码级行为修复，回滚 = 还原二进制）
- 是否需要 smoke：是（真机复验：AppUI 切换模型 → 不要求重启 → 下一轮对话实际走新模型）

## 变更概述
拆除启动 profile LLM 运行时的三层锁，让启动期钉住的 profile 与动态 profile 走**同一条**提交后转换路径：

1. `commit_profile_llm_runtime_transition` 删除 `startup_pinned → RestartRequired` 短路（13910/13924），所有 profile 统一进入"驱逐 → 代际守卫 → 重建"流程；
2. `ensure_session_profile_runtime` 增加"重建模式"入参（或等价机制）：提交后重建场景跳过 21810 的启动快照短路，从 store 文件重引导并写入动态缓存；普通冷启动解析保持原短路语义（不为每个启动 profile 重复引导）；
3. `runtime_policy_stamp_for_profile`（10926）与 `profile/llm/select` 的 `restart_required` 盖章（13144）同步更新：stamp 的 model/provider 改为按 `resolve_session_profile_runtime` 的真实优先级（动态缓存 → 启动快照）报告，select 不再对启动 profile 盖 `restart_required: true`。

约束：动态缓存键（`octos_home::profile_id`）对启动 profile 同样可计算；`state.profiles` 保持不可变（启动快照继续充当引导兜底与 steer sweep 等场景的枚举源），新运行时只写入动态缓存——`resolve_session_profile_runtime` 的动态优先语义天然让新配置生效。
