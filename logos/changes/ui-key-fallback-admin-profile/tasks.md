# 实现任务

> **流程硬节点**：本提案只到 proposal 确认点。以下实施步骤在 **operator 确认选型后**
> 才执行；当前仅列出计划，未实施。

## [delta] 规格变更
- [ ] 无对外契约形状变更（方案 a 为解析链末端回退，不改 API/配置 schema）

## [code] 代码实现（方案 a，待 operator 确认后实施）
- [ ] 运行时 key 解析链末端加「消费 profile key 缺/占位 → 回退 admin profile 同 provider llm key」
- [ ] 占位判定：`REPLACE_ME` / 空 / 缺失
- [ ] 回退命中写可观测日志（tracing::warn!/info!）
- [ ] 显式 key 优先（cluster-worker 有显式 key 不回退）
- [ ] admin 也无 key → 报错（不静默）
- [ ] 回退不触碰 cluster-worker 种子/override 只读语义（#13/#15/#20 保留）

## 测试（UT-S16-47 起，待实施）
- [ ] UT-S16-47: cluster-worker key env 缺失 → 回退 admin key 命中 + 日志
- [ ] UT-S16-48: env 值=REPLACE_ME 占位 → 回退
- [ ] UT-S16-49: cluster-worker 显式 key → 不回退（显式优先）
- [ ] UT-S16-50: admin 也无 key → 报错（不静默）
- [ ] UT-S16-51: 回退不污染种子/override（只读探测语义保留）
- [ ] 用例登记 core-S16-test-cases.md 批 10 + reporter test-results.jsonl

## 验收
- profiles:: / auth_handlers:: 全绿（--test-threads=4）
- 真机四条验收（issue #11）由外环重做注入链执行：UI 配 key 即用 / 滚动保留 / 种子仍生效 / 文档

## 当前状态
**PENDING operator 选型确认（a/b/c）。确认前不实施、不写代码、不部署。**
