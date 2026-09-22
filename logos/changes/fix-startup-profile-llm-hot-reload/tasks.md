# 实现任务

## [delta] 规格变更
- [x] 产出 delta：profile LLM 运行时转换语义——启动 profile 的 mutation 提交后从 `RestartRequired` 变为与动态 profile 一致的 `Reloaded / Deferred / PersistedButNotLive`；`restart_required` 响应字段在启动 profile 场景不再为 true

## [code] 代码实现
> 单切片预判：影响 1 个文件（ui_protocol_transport.rs）3 处函数 + UT，单路径 bugfix，原因已明，易回滚——六维打分 ≤2 分，单切片闭环。
- [ ] 单切片：拆除启动 profile LLM 运行时三层锁——(1) `commit_profile_llm_runtime_transition` 删除 `startup_pinned → RestartRequired` 短路，统一走驱逐+代际守卫+重建；(2) `ensure_session_profile_runtime` 增加重建模式（提交后场景跳过 `state.profiles` 短路、从 store 文件重引导并写入动态缓存）；(3) `runtime_policy_stamp_for_profile` 按动态缓存→启动快照的真实优先级报告 model/provider，`profile/llm/select` 不再对启动 profile 盖 `restart_required: true`。含 UT（启动 profile upsert 后 → 新 runtime 写入动态缓存且 `restart_required=false`；重建中竞争提交 → 代际守卫拒绝旧 runtime；无 mutation 的冷启动解析 → 仍走启动快照不重复引导）+ OpenLogos reporter 写入 logos/resources/verify/test-results.jsonl

## [deploy] 部署任务
- [ ] 重建 octos 二进制（CPU 受限：nice -n 19 + CARGO_BUILD_JOBS=4），经 ConfigMap 注入本地 docker-desktop `octos` ns 滚动重建 pod，真机 smoke：AppUI 切换模型 → 响应 `restart_required=false` → 下一轮对话实际命中新模型
