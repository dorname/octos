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

- [x] 批次 A：实现 Watchdog 配置校验、首次 EOF baseline、黑板/events 增量分类、稳定 signal id、进展指纹、轮转游标、原子状态、告警脱敏与三次无进展熔断；同步交付 UT-S17-01..05、UT-S17-10..16、UT-S17-18..20、ST-S17-01、ST-S17-06..08、ST-S17-12、ST-S17-14 及 OpenLogos reporter
- [x] 批次 B：实现 herdr agent list 的 canonical cwd 精确发现、多候选 fail closed、outer-duty HELD 权威校验、accepted-only 投递状态机与单周期调度；同步交付 UT-S17-06..09、UT-S17-17、ST-S17-02..05、ST-S17-09..11、ST-S17-14 及 OpenLogos reporter
- [x] 批次 C：接入 `octos watchdog run/run-once/status`，实现 systemd user unit、默认不启用的安装/卸载脚本与隔离 smoke runner；同步交付 ST-S17-13、SMOKE-S17-01..06，回归 UT-S17-01..20、ST-S17-01..14 并生成完整 OpenLogos 结果账本

## [deploy] 部署与冒烟
- [x] 在隔离测试环境安装并显式启用 systemd user service，验证 `Restart=always`、状态恢复、日志与 status
- [x] 按部署方案验证卸载/回滚后不删除黑板、goal ledger、checkpoint 或业务提交，并生成 deployment report / `DEPLOY_DONE`
