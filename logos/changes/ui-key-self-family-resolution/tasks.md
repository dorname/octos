# 实现任务

## [delta] 规格变更
- [x] 无（治本 a′：服务端自体 by-family key 解析，不改对外契约形状）

## [code] 代码实现
- [x] bootstrap key 解析处新增自体 by-family 解析（任何 profile 含 admin）
- [x] 路由 api_key_env 在自身 env_vars 缺失/占位 → 按族候选名（ENTRY.api_key_env + key_env_aliases，去重路由名优先）在自身 env_vars 查真实 key → 注入 config.env_vars[路由名]
- [x] 先于/独立于 #23 跨 profile 回退，admin 不再被排除
- [x] 显式 key 优先不变 / 成功注入 warn!（带命中名+族名，不泄 key 值）/ 只注入内存 Config 不写 seed/override
- [x] #23/#31 跨 profile 回退原样保留
- [x] UT-S16-57 起（admin 名不匹配注入/名匹配 no-op/无候选不注入/占位跳过）
- [x] 用例登记 core-S16-test-cases.md 批 13 + reporter（hermetic，仅 test-only 名）

## 验收
- 导出 ANTHROPIC_API_KEY 环境 runtime::profile:: + profiles:: 全绿
- 验收门：部署后 WS probe profile_id=admin turn completed 无 401（外环部署后执行）
