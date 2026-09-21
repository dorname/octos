# 实现任务

## [delta] 规格变更
- [x] session/open ack 的 SessionOpened.session_id 现携带规范键（{profile}:api:{raw}）;listSessions 经规范键解析（读路径 profile_id() 一级优先）

## [code] 代码实现
- [x] ① session/open 铸造规范键+ack 返回（mint_canonical_session_key 纯函数，仅显式 params.profile_id 触发，kind=api，topic 保留）— commit e8938d80
- [x] ② 读路径 resolve_sessions_for_lookup 已以 session_id.profile_id() 一级优先（既有，规范键一级命中，legacy 推断链保留为存量裸键回退）— 确认无需改动
- [x] ③ Admin WS connection_profile_id → ADMIN_PROFILE_ID（对齐 REST）— commit 7d1362b5（前轮已落）
- [x] ④ octos-web sessionTimestamp 适配第三种键形 {profile}:api:web-{ts}（+uuid-v7）+ 单测 — submodule commit 1407897（只 commit 未 push）
- [ ] ④ 续作：octos-web 采信 open ack 规范 id 回写会话状态（generateSessionId :221 生成处 → ack 规范 id 采信）— 见下"遗留"
- [x] 用例登记 core-S16-test-cases.md 批 14 + reporter

## 测试（hermetic）
- [x] mint bare+profile→规范键 / topic 保留 / 已 scoped+无 profile 跳过（mint_canonical 3 例）
- [x] canonical 读路径 profile 一级命中 + 裸键 Admin 身份 legacy 命中 admin（canonical_key_read_path_resolves_profile_first）
- [x] 裸键跨 profile 不回归（cross_profile_does_not_regress）
- [x] octos-web sessionTimestamp 规范键 ms + uuid-v7 两例（34/34 绿）

## 验收
- 验收门（外环部署后执行）：WS probe 裸 id(profile_id=admin)→ack 规范键→turn 完成→新连接 hydrate→messages 重放
- octos-web 只 commit 严禁 push

## 遗留（NEAR-LIMIT 申报）
- ④ 完整 ack id 回写（generateSessionId 采信规范 id）需更深运行时状态改造，单轮预算不足
- ①铸造引入 2 个 cross-profile scope 测试回归（cold_scope_.../should_deliver_later_... 断言铸造前裸键 scope 语义）——预期行为变更，需适配为规范键语义，下一轮
