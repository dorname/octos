# 实现任务

> retroactive backfill — 按 loop.md 第 6 条流程补齐，不改运行时。

## [delta] 规格变更
- [x] 建立 `logos/resources/test/core-S16-test-cases.md`（S16 集群域用例登记，26 项 UT-S16-01..26）

## [code] 代码实现
- [x] 无代码变更（回填型）

## [verify] 验证与报告
- [x] 向 `logos/resources/verify/test-results.jsonl` 追加 26 项 pass 记录（reporter 数据取黑板 #4/#5/#6 采认批注存证结果）
- [x] `openlogos merge retro-issue-flow-backfill` 执行 merge（merge 后停下，verify/archive/push 由外环执行）

## 验收
- 用例文档存在且 ID 全局唯一（S16，不撞 S01-S17）
- test-results.jsonl 新增 26 行，case_id 与文档对齐，timestamp 为各批复验实际时间
- 本提案 ACK 附 slug + 用例 ID 清单
