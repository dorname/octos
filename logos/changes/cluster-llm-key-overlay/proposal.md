# 变更提案：cluster-llm-key-overlay

> module: core | created: 2026-09-21 | 依据：黑板条目 #13（issue #11 治本：集群模式 UI LLM key 配置路径）

## 变更原因
issue #11（外环全链实证）：cluster-worker profile 被 #7 改为 ConfigMap 只读挂载后，UI 无法为其供 LLM key——UI 配 key 写入的是 admin profile（PVC 可写，保存成功），但对话走 DEFAULT_PROFILE=cluster-worker（ConfigMap 设定），其 json 是 CM 只读（写不进），secret 若为 REPLACE_ME 占位则对话 401。配置路径断裂。#7 修「滚动丢失」时切断了 PVC 残留的隐性供 key 通道，属设计缺口而非回归 bug。

## 变更类型
代码级（主仓 Rust：profile 加载/合并逻辑）+ 部署级（deploy 清单挂载布局）+ 文档级（deploy/docs 三通道与优先级）。

## 选型设计（方案 c：CM 种子 + PVC 可写覆盖合并，推荐理由）
**选方案 c**，不选 a/b，理由：
- **弃 a**（DEFAULT_PROFILE 改可写 profile）：把 CM 种子去掉、profile 全落 PVC——回到 #7 之前的「PVC 残留遮蔽 ConfigMap」问题，#7 的设计初衷（profile 作为配置而非 PVC 状态）被破坏，语义弱。
- **弃 b**（UI 受控写 secret 通道）：让 UI 直接写 K8s Secret——跨层（AppUI → K8s API）安全面复杂（RBAC/审计/多租户隔离），且 secret 是集群级共享资源，per-profile key 语义不匹配。
- **选 c**（CM 种子 + PVC 可写覆盖合并）：cluster-worker **保留 CM 为种子**（滚动稳定，#7 设计初衷保住），**允许 UI 显式配置落 PVC 覆盖层**（`<id>.override.json`）；覆盖层带**来源标记**（`managed_by: "ui"` + `updated_at`）以区分「显式用户配置」与「历史残留」——化解与 #7「防 PVC 残留遮蔽」初衷的张力：**残留无标记 → 种子胜**（#7 防的正是这个）；**显式配置有标记 → 覆盖胜**（UI 配 key 即用）。

## 合并语义（定案）
- `ProfileStore::get(id)`：
  1. 读种子 `<id>.json`（CM 只读挂载路径，若存在）；
  2. 读覆盖层 `<id>.override.json`（PVC 可写路径，若存在）；
  3. 覆盖层**有** `managed_by: "ui"` 标记 → 深合并（覆盖层字段优先，主要是 `config.llm` 与 `config.env_vars`），返回合并结果；
  4. 覆盖层**无**标记（历史残留）→ 忽略覆盖层，返回种子（#7 防残留语义保留）；
  5. 无覆盖层 → 返回种子；无种子 → 返回覆盖层（若带标记）或 None。
- `ProfileStore::save(profile)`：
  - 若 `profile_path(id)` 是只读（CM 挂载，写会 EROFS）→ 改写 `<id>.override.json`（PVC 可写），并自动打 `managed_by: "ui"` + `updated_at` 标记；
  - 若可写 → 照旧写 `<id>.json`（非 CM 挂载的 profile，如 admin）。

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：profile 加载/保存的合并语义（内部行为，不改对外 API 形状）
- 影响的业务场景：S16 集群域（集群模式 LLM key 配置）
- 影响的 API：PUT /api/my/profile、PUT /api/admin/profiles/{id}（行为：CM 种子 profile 的保存落覆盖层）
- 影响的 DB 表：无
- 影响的编排测试：crates/octos-cli profiles 测试（种子+覆盖合并、来源标记、EROFS 改写）

## 本批覆盖的 UT 用例 ID（挂 S16 域接 UT-S16-36 起）
- **UT-S16-36** — 种子+无覆盖：get 返回种子原样
- **UT-S16-37** — 种子+带 managed_by=ui 标记覆盖：get 深合并，覆盖层 llm/env_vars 优先
- **UT-S16-38** — 种子+无标记覆盖（历史残留）：get 忽略覆盖层，种子胜（#7 防残留语义）
- **UT-S16-39** — save 到只读 CM 种子路径：改写 override.json 并打 managed_by=ui + updated_at 标记
- **UT-S16-40** — save 到可写路径（非 CM 挂载）：照旧写 <id>.json，不产生 override
与 `logos/resources/test/core-S16-test-cases.md` 对齐（merge 时追加）。

## 部署影响
- 是否需要部署：是（deploy 清单 + docs）
- 部署原因：挂载布局（override 层 PVC 路径）与文档（三通道）
- 影响环境：本地 docker-desktop octos ns
- 是否涉及数据迁移：否（override 层是新文件，无历史 override 时行为同现状）
- 是否需要回滚预案：否
- 是否需要 smoke：是（真机验收按 issue #11 四条标准）

## 验收标准（issue #11 四条）
1. 集群模式下 UI 配 key（不 kubectl 换 secret）后对话立即可用；
2. rollout 重启后配置保留（覆盖层语义）；
3. CM 种子变更仍可生效（无覆盖时）；
4. 文档：deploy/docs 写明集群模式 LLM key 的三条通道（UI 覆盖层 / kubectl secret / CM 种子）与优先级。
