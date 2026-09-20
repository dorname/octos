# 实现任务

## [delta] 规格变更
- [x] 在 `core-01-requirements.md` 新增 S17 夜间监督与有界续推，定义活性保证、非目标与验收条件；delta 声明合并时将 `scenario_counter.next_id` 由 17 更新为 18
- [x] 更新 S06 无人值守需求，区分普通 cron 迭代耗尽、goal `budget_limited` 与外环裁决路径
- [x] 更新 `core-03-gateway-channels-design.md`，并新增 `core-08-octoloop-watchdog-design.md` 定义 Watchdog 配置、状态、告警与禁用/回滚交互
- [x] 先产出 `core-S17-octoloop-night-watchdog.md` 场景时序图，再从时序图推导 CLI/本地接口设计
- [x] 更新架构文档，明确 Watchdog、黑板、events、goal ledger、herdr 与 outer-duty 的边界及唯一事实源
- [x] 更新部署方案，定义 systemd user service、自动重启、日志、状态目录、安装/卸载与 smoke 步骤
- [x] 新增 S17 测试与 smoke 用例文档，覆盖正常、异常、重启恢复、安全与幂等矩阵

## [code] 代码实现

（本段在规格合并前留空；`openlogos merge` 完成后由 slice-planner 基于已合并规格与 UT-S17-01..20、ST-S17-01..14 划分真实代码切片，每片同时包含业务代码、对应测试与 OpenLogos reporter。）

## [deploy] 部署与冒烟
- [ ] 在隔离测试环境安装并显式启用 systemd user service，验证 `Restart=always`、状态恢复、日志与 status
- [ ] 按部署方案验证卸载/回滚后不删除黑板、goal ledger、checkpoint 或业务提交，并生成 deployment report / `DEPLOY_DONE`
