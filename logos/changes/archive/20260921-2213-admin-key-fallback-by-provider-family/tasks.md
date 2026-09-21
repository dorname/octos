# 实现任务

## [delta] 规格变更
- [x] 无（缺陷修复：回退取值维度修正，不改对外契约形状）

## [code] 代码实现
- [x] apply_admin_llm_key_fallback 按 provider 族解析候选名集合（路由 api_key_env + family ENTRY.api_key_env + key_env_aliases）
- [x] 任一候选命中且非占位即注入到消费 config 的 api_key_env 名下
- [x] 覆盖 family=minimax-token + api_type=anthropic 组合（两态都在候选集）
- [x] admin 无候选 key → no-op 不静默（保留）
- [x] 显式优先 / 种子只读 / warn 带命中名+族名不泄 key 值（保留）
- [x] UT-S16-53 起（名字错位 #30 入案 + 族名命中 + 两候选皆占位 + admin 无 key no-op）
- [x] 用例登记 core-S16-test-cases.md 批 12 + reporter

## 验收
- 导出 ANTHROPIC_API_KEY 环境下 profiles:: + runtime::profile:: 全绿
- 本轮不部署（外环第五次注入链验收）
