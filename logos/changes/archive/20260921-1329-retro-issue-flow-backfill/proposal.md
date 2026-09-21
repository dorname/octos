# 变更提案：retro-issue-flow-backfill

> module: core | created: 2026-09-21 | retroactive: true（回填型，非新功能）

## 变更原因
批 1/2/3（fork dorname/octos 7 个 issue 的修复）由外环 runbook 驱动直接落地，未按 OpenLogos 流程执行（loop.md 第 6 条新纪律 2026-09-21 生效前）。外环已建追溯提案 `retro-issue-fixes`（commit dcbef311，已归档）完成**提案管辖层**补档；本提案补齐**流程执行层**——测试用例登记与 OpenLogos reporter，两者互补不重叠。黑板条目 #7 即本提案依据。

## 变更类型
流程级（retroactive backfill）——不改任何运行时行为；只补测试用例文档 + reporter JSONL。

## 变更范围
- 影响的需求文档：无（回填不新增需求）
- 影响的功能规格：无（回填不新增规格）
- 影响的业务场景：挂靠 S16（集群域，三批测试均属 k8s 集群部署/前端族修复范畴）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：
  - 批 1（#1 #2 #3 deploy/k8s 族）：`crates/octos-bus/tests/deploy_directory_layout.rs` 新增断言 + `crates/octos-bus/tests/pg_persistence_matrix.rs` 10 项既有测试
  - 批 2（#4 #5 octos-web 族）：`octos-web/src/runtime/hydrate-projection.test.ts` 新增回归 3 项（vitest）
  - 批 3（#7 #8 AppUI 族）：`deploy_directory_layout.rs` 新增 `test_octos_deployment_mounts_cluster_worker_profile_configmap`（含于批 1 文件，用例 ID 独立登记）

## 部署影响
- 是否需要部署：否
- 部署原因：纯文档 + JSONL 追加，不触碰运行时
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述
为三批已落地修复补齐 OpenLogos 流程执行层：
1. 在 `logos/resources/test/` 按 `<module>-SXX-test-cases.md` 规范补建 `core-S16-test-cases.md`（S16 集群域，全局编号不撞 S01-S17），登记三批修复的全部自动化用例 ID（UT 命名沿用 S17 体系 `UT-SXX-NN`）；
2. 向 `logos/resources/verify/test-results.jsonl` 追加三批测试的 pass 记录（case_id / status / duration / timestamp / scenario），数据取黑板 #4/#5/#6 采认批注存证的真实结果（批 1: 12/12 + 10/10；批 2: 3/3；批 3: 13/13），timestamp 用各批复验实际时间；
3. 不改任何代码 / 规格 / 部署物。

## 用例 ID 清单（本批登记，输出代码前明示）
- 批 1：UT-S16-01 ~ UT-S16-12（deploy_directory_layout 12 项）、UT-S16-13 ~ UT-S16-22（pg_persistence_matrix 10 项）
- 批 2：UT-S16-23 ~ UT-S16-25（octos-web hydrate 回归 3 项）
- 批 3：UT-S16-26（cluster-worker-profile CM 挂载断言 1 项）
合计 26 项。
