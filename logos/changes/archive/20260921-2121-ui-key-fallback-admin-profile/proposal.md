# 变更提案：ui-key-fallback-admin-profile

> module: core | created: 2026-09-21 | status: **PENDING operator 选型确认（流程硬节点，确认前不实施）**

## 变更原因
黑板条目 #23（issue #11 第三层缺口）。#22 后 operator 真机实测（2026-09-21 20:05
保存+发消息）**仍 401**。铁证：
1. `admin.json` mtime 20:05——保存动作落在了 **admin** profile；
   `cluster-worker.override.json` 仍 19:57（内环验收值，未被触碰）。
2. 代码：`octos-web` API Keys 面板走 `PUT /api/my/profile`；serve 端
   `resolve_my_profile_id`（`auth_handlers.rs:3920`）对 `AuthIdentity::Admin`
   **固定返回 `ADMIN_PROFILE_ID`="admin"**（`auth_handlers.rs:3962`）。
3. 会话跑 `DEFAULT_PROFILE=cluster-worker`（`deploy/k8s/03-cluster-with-config.yaml:115`）。
4. 种子路由 `api_key_env=ANTHROPIC_API_KEY` ← secret 值 `REPLACE_ME`（占位）。

**结论**：override 机制本身已通（#22 证），但「UI 供 key 的落点（admin）与消费
profile（cluster-worker）」**结构性错位**——UI 写进 admin.json 的 key 永远不会被
cluster-worker 会话用到。「UI 配 key 即用」（issue #11 验收标准）在现产品结构下
**不可达**，需产品级修复。这是洋葱第三层：#14 挂载只读 → #19 单文件只读 →
#23 落点/消费错位。

## 变更类型
产品级 / 设计级（涉及 key 解析链语义或 UI/身份语义，视选型而定）。

## 变更范围
- 影响的功能规格：LLM API key 的解析链（`config.rs get_api_key_with_env` /
  `resolve_api_key`）与/或 `resolve_my_profile_id` 身份→profile 映射、
  `octos-web` Settings 写入目标 profile。
- 影响的业务场景：k8s 集群下 UI 配 key 即用（issue #11 四条标准之一）。
- 影响的 API：视选型（方案 a 不动 API；方案 b 可能调 `/api/my/profile` 语义或加
  profile picker）。
- 影响的 DB 表：无。
- 影响的编排测试：profiles:: / auth_handlers:: 单测（UT-S16-47 起）。

## 部署影响
- 是否需要部署：是（外环重做注入链 + 真机四条验收；本提案不部署）
- 影响环境：k8s 集群
- 是否涉及数据迁移：否
- 是否需要回滚预案：视选型（方案 a 纯增量回退链，可关；方案 c 改 env 可回滚）
- 是否需要 smoke：是（UI 配 key → 发消息 → 不再 401）

## 三方案对比

### 方案 a：cluster-worker 解析链 key 缺失/占位时回退 admin profile 的 llm key（**推荐**）
- **机制**：在 cluster-worker（运行时消费 profile）的 key 解析链末端，当
  `api_key_env` 指向的环境变量**缺失或值为占位**（`REPLACE_ME`/空）时，**回退读
  admin profile 的同 provider llm key**（UI 供 key 的实际落点）。回退命中时写
  `tracing::warn!/info!` 日志（可观测：「cluster-worker key 缺占位，回退 admin
  profile key」）。
- **优点**：最小侵入——不动 UI、不动身份语义（`resolve_my_profile_id` 仍
  Admin→admin）、不动 DEFAULT_PROFILE；UI 供 key 立即对 cluster-worker 会话生效，
  「UI 配 key 即用」直达。纯增量回退链，可写场景行为不变，风险低。
- **缺点**：跨 profile 读 key 引入「admin 是 key 兜底源」的隐式耦合；多 profile
  多 key 时语义需界定（仅当消费 profile 缺/占位才回退，不覆盖显式配置）。
- **UT（UT-S16-47 起）**：UT-S16-47 cluster-worker key env 缺失 → 回退 admin key
  命中 + 日志；UT-S16-48 env 值=REPLACE_ME 占位 → 同样回退；UT-S16-49
  cluster-worker 显式 key 在 → 不回退（显式优先）；UT-S16-50 admin 也无 key →
  报错（不静默）；UT-S16-51 回退不污染 cluster-worker 种子/override（只读探测
  语义保留）。

### 方案 b：Settings 写入面向会话 profile（profile picker 或 my/profile 语义调整）
- **机制**：UI 让用户选目标 profile（picker），或 `resolve_my_profile_id` 对
  Admin 改为返回「当前会话 profile」而非固定 admin。
- **优点**：写读同点，语义最直观，无跨 profile 耦合。
- **缺点**：UI 改动大（octos-web picker / API 语义变更）；`resolve_my_profile_id`
  Admin 固定 admin 是**既有身份语义**（admin 是特权管理 profile），改动影响面
  广（多 `/api/my/*` 端点、host-scope 分支、鉴权）；多租户 subdomain 语义纠缠。
  风险高、周期长。
- **弃选理由**：改动面与风险远超收益；admin 身份语义是产品既有契约，不宜为
  单一 key 落点问题重构。

### 方案 c：DEFAULT_PROFILE=admin（部署级一改）
- **机制**：`03-cluster-with-config.yaml` 的 `DEFAULT_PROFILE` 改 `admin`，会话
  直接跑 admin profile（UI 供 key 落点=消费点重合）。
- **优点**：改动最小（一行 env）。
- **缺点**：**放弃 cluster-worker 的「配置即代码」隔离**——cluster-worker 种子
  （CM 版本化、可审计、可滚动）失去意义；admin 是可写管理 profile，会话直接写
  它等于放弃种子不可变性（#13/#15/#20 三层只读保护的设计初衷）；回滚需再改
  env+滚动。
- **弃选理由**：摧毁配置即代码隔离，与 #11 治本方向（种子+覆盖）背道而驰。

## 推荐倾向
**方案 a**。理由：直达「UI 配 key 即用」且最小侵入，不动 UI/身份/部署语义；
回退仅在消费 profile key 缺/占位时触发，显式配置优先，可观测（日志），可写
场景行为不变。方案 b/c 分别以高改动风险或摧毁隔离为代价，弃选理由如上。

## 流程硬节点
本提案**只到 proposal 确认点**——写完 proposal+tasks 即停，ask_outer 通知，
**等 operator 确认选型后才允许实施**。严禁先动手写代码。本轮不部署。
