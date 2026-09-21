# 变更提案：canonical-profile-scoped-session-keys

> module: core | created: 2026-09-22 | operator 已授权「授权给你」

## 变更原因
黑板条目 #40（会话切换历史丢失根治）。**根因已外环实证**：hydrate 门禁
`resolve_sessions_for_lookup`（`ui_protocol_transport.rs:21914`）按 键profile→连接身份→路由
解析；裸 `web-*` 键 + Admin 身份（`connection_profile_id=None`,`:6500` 注释自认）+ 无子域
→ 兜底顶层 `_main` → `unknown_session`；而 session/open 按显式参数路由注册 admin 管理器——
**读写两路解析不一致**（open 认得 hydrate 不认得）。外环 probe 已用 operator 真实会话
`web-1789871791268-87x3ps` 复现（磁盘 JSONL+ledger 双在，hydrate 报 unknown_session）。

## 变更类型
设计级 / 接口级（session key 归属解析语义 + WS ack 契约 + octos-web 采信）。

## 变更范围
- 影响的功能规格：`SessionKey` 铸造/解析（`octos-core/types.rs`）、`resolve_sessions_for_lookup`
  读路径归属、`handle_session_open`/`open_session_result` 铸造与 ack、`authenticated_profile_id`
  Admin 语义、octos-web bridge（`session-context.tsx` 生成处 + 时间戳排序）
- 影响的业务场景：AppUI 会话切换后历史保留（裸 web 键跨切换可读）
- 影响的 API：WS `session/open` ack 新增规范 id 字段；listSessions 返回规范 id
- 影响的 DB 表：无（ledger/JSONL 文件键）
- 影响的编排测试：runtime / ui_protocol / octos-web vitest

## 部署影响
- 是否需要部署：是（验收门部署后外环执行；operator 已授权，push 另请示）
- 影响环境：k8s 集群
- 是否涉及数据迁移：否（不迁移存量数据，legacy 推断回退兼容存量裸键）
- 是否需要回滚预案：是（读路径 legacy 回退保留，可回滚）
- 是否需要 smoke：是（验收门见下）

## 变更概述（键为唯一事实源，推断只作 legacy 回退）
1. **session/open 铸造规范键 + ack 返回**：对裸客户端 id + 显式 `profile_id=P` 铸造规范键
   （按 `SessionKey` 既有文法 `{profile}:{channel}:{id}`,kind 从 ui-protocol 现状确定），
   open ack 返回规范 id;ledger/JSONL/会话注册表全用规范键。
2. **读路径按键一级解析**：全部读路径（hydrate/state/messages/listSessions）归属解析以
   `session_id.profile_id()` 为第一优先；现有推断链降级为 legacy 兼容（不迁移存量数据）。
3. **Admin WS 语义对齐 REST**:`AuthIdentity::Admin` 的 `connection_profile_id` 解析为
   `ADMIN_PROFILE_ID`（对齐 `resolve_my_profile_id`)，使存量 admin 裸键历史立即可读。
4. **octos-web 采信规范 id**:bridge 采信 open ack 的规范 id（`session-context.tsx:221`
   生成处与 `:334` 时间戳排序逻辑需适配第三种键形，含其单测）;listSessions 返回规范 id。

## 测试（全 hermetic)
裸 id+profile_id→全链规范键 / ack 返回规范 id / hydrate 按键一级命中 / 存量裸键+Admin
身份→hydrate 命中 admin 管理器（operator 虫 case)/ 裸键跨 profile 场景不回归 /
listSessions 规范化。

## 验收门（部署后外环执行）
WS probe 开裸 id(profile_id=admin)→ ack 规范键 → turn 完成 → **新连接** hydrate 该会话
→ messages 重放（历史跨切换保留）；再由 operator 5174 真机切换验收。

## 约束
octos-web submodule 只 commit 严禁 push（红线）。verify/archive/重建/注入外环；push 另请示。
CPU 限载。
