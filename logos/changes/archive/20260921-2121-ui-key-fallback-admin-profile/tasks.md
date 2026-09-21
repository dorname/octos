# 实现任务

> operator 已裁定选型 = 方案 a。本提案已按方案 a 范围实施完成。

## [delta] 规格变更
- [x] 无对外契约形状变更（方案 a 为解析链末端回退，不改 API/配置 schema）

## [code] 代码实现（方案 a）
- [x] 消费 profile 解析链 key 缺/占位 → 回退 admin profile 同 provider llm key（apply_admin_llm_key_fallback，bootstrap_resolved 建 provider 前）
- [x] 占位判定：REPLACE_ME / 空 / 缺失
- [x] 回退命中写可观测日志（tracing::warn!）
- [x] 显式 key 优先（cluster-worker 有真实 key 不回退）
- [x] admin 也无 key → no-op，下游缺失错误照常报（不静默）
- [x] 回退只注入内存 Config.env_vars，不触碰种子/override（#13/#15/#20 只读语义保留）

## 测试（UT-S16-47..51）
- [x] UT-S16-47: 缺失 → 回退 admin key 注入
- [x] UT-S16-48: REPLACE_ME 占位 → 回退
- [x] UT-S16-49: 显式 key → 不回退（显式优先）
- [x] UT-S16-50: admin 无 key → no-op 报错下游
- [x] UT-S16-51: admin 自身不回退 + 种子/override/admin.json 字节不动
- [x] 用例登记 core-S16-test-cases.md 批 10 + reporter test-results.jsonl（+5，总行 125）

## 验收
- profiles:: 108/108 + runtime::profile:: 28/28 绿（--test-threads=4）
- 本轮不部署（真机四条验收由外环重做注入链执行）
