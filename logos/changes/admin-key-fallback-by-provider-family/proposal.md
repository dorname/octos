# 变更提案：admin-key-fallback-by-provider-family

> module: core | created: 2026-09-21

## 变更原因
黑板条目 #31（修复 #23 回退取名维度）。#30 真机铁证：UI 供 key 落在 admin profile 的
**`MINIMAX_API_KEY`**（provider family `minimax-token` 的规范默认名），而 cluster-worker
种子路由 `api_key_env=**ANTHROPIC_API_KEY**`（anthropic 协议形态的覆盖名）。#23 回退按
消费 profile 的 `config.api_key_env` **字面名**取 `admin.env_vars[ANTHROPIC_API_KEY]` →
admin 存的是 MINIMAX_API_KEY → 取到 None → **回退 no-op**（变量名错位，非 admin 真无
key）。「UI 配 key 即用」仍不通。

## 变更类型
代码级（缺陷修复：回退取值维度修正，不改对外契约形状）。

## 变更范围
- 影响的功能规格：`runtime/profile.rs apply_admin_llm_key_fallback` 的 admin key 查找维度
- 影响的业务场景：k8s 集群下 UI 配 key 即用（issue #11）——admin 用族规范名、种子用
  协议覆盖名时回退正确命中
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：runtime::profile::（UT-S16-53 起）

## 部署影响
- 是否需要部署：是（外环第五次注入链验收；本提案不部署）
- 影响环境：k8s 集群
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：是（真机：UI 配 key → 会话 → 不再 401 + 回退 warn）

## 变更概述
`apply_admin_llm_key_fallback` 查找 admin key 时按 **provider 族**解析**候选名集合**，
而非字面单一 env 名：
1. 消费 profile 路由的 `api_key_env`（如 ANTHROPIC_API_KEY，协议覆盖名）；
2. provider family 的规范默认名 `ENTRY.api_key_env`（如 minimax-token → MINIMAX_API_KEY）；
3. family 的 `key_env_aliases`（额外别名）。
任一候选在 `admin.env_vars` 命中且非占位即取该 key，**注入到消费 config 的
`api_key_env` 名下**（保持消费链仍按其声明的 env 名读取）。覆盖 family=minimax-token
+ api_type=anthropic 的组合（族名 minimax-token 给 MINIMAX_API_KEY，路由覆盖名
ANTHROPIC_API_KEY 两态都在候选集）。admin 确无任何候选 key → 维持 no-op 不静默。

### 选型理由（选项1）
- **同 provider 族候选名集合（采纳）**：回退语义本是「同 provider 的 key」，env 变量名
  只是载体；族规范名 + 路由覆盖名 + aliases 覆盖了「admin 用族名 / 种子用协议名」的
  错位（#30）。最小改动，不碰 UI 落点、不碰种子（CM ro）。
- **改种子 api_key_env=MINIMAX_API_KEY（弃）**：种子是 CM subPath ro（配置即代码），且
  admin 落点由 UI/身份语义定，两者名字本可不同，改种子不对因。
- **UI 按 family 写规范名（弃）**：改动大且约束 UI 行为。

### 保留语义
显式 key 优先（消费 profile 真实 key 不覆盖）/ 种子只读不碰 / admin 无候选 key no-op
不静默 / 可观测 warn（带命中候选名与族名，**不泄 key 值**）。

## 验收硬标准
导出 `ANTHROPIC_API_KEY` 环境全绿（#26 惯例）；profiles:: + runtime::profile:: 全绿。
