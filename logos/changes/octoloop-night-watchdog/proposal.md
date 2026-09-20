# 变更提案：octoloop-night-watchdog

> module: core | created: 2026-09-20

## 变更原因
当前 OctoLoop 已具备内环 `turn-continuation`、50 轮预算 checkpoint、
`budget_limited` 状态、黑板 ACK 与运行时事件流，但缺少一个独立于 Claude
和 octoscode 会话生命周期的常驻监督器。现有黑板/事件哨兵只输出信号后
退出，不会自动重挂，也不会把信号转换成 `herdr agent prompt` 门铃；因此
内环完成 ACK、进入 `blocked` / `budget_limited`、或空闲但仍有未完成条目
时，外环可能持续 idle，夜间执行形成静默停顿。

本变更来源于实际双环运行反馈：内环在迭代预算耗尽后需要人为续推，外环
也不会因黑板文件变化自动醒来。目标不是承诺任务必然完成，而是建立可审计
的活性保证——无人值守期间系统必须进入“继续推进、唤醒外环裁决、或达到
有界重试后明确停止并告警”三态之一，禁止无限重试和静默停摆。

## 变更类型
需求级 + 设计级 + 接口级 + 代码级

## 变更范围
- 影响的需求文档：`core-01-requirements.md`，新增 S17“OctoLoop 夜间监督与有界续推”，并补充 P03/P10 的可恢复性验收条件
- 影响的功能规格：`core-03-gateway-channels-design.md`（无人值守监督、告警与调度边界）；必要时补充独立的 core-08 OctoLoop 功能规格
- 影响的业务场景：扩展 S06 的无人值守失败恢复；新增 S17 场景时序图，场景号使用全局下一个编号 S17
- 影响的架构：新增常驻 Watchdog、持久游标/重试状态、事件分类器、进展判定器与 herdr 唤醒适配器；`outer-duty` 锁仍是外环主审权唯一裁定面
- 影响的 CLI / 本地接口：拟新增 `octos watchdog` 的 start/status/run-once 或等价入口；所有接口必须由 S17 时序图推导后再定稿
- 影响的 DB 表：不修改业务数据库；监督状态使用本地持久存储，具体格式在技术设计阶段确定，需支持重启去重与游标恢复
- 影响的编排测试：新增 ACK 唤醒、blocked/escalation/budget_limited 唤醒、空闲续推、三次无进展熔断、重启恢复、锁权校验与重复信号去重的 API 编排测试
- 跨仓依赖：herdr 提供 agent/pane 发现与 prompt 注入；octoscode 提供 outer-duty 与黑板协议。当前提案不直接修改 `/home/kyle/octoscode`，若需协议或发行物变更必须在对应仓库另建提案

## 部署影响
- 是否需要部署：是
- 部署原因：需要安装并启用独立于模型会话的常驻监督进程；Linux 首选 systemd user service，并配置进程退出自动重启
- 影响环境：本地开发机、测试环境；不默认启用到生产多租户环境
- 是否涉及数据迁移：否；仅新增可删除并可重建的本地监督状态
- 是否需要回滚预案：是；停用服务、移除 unit、保留黑板/goal ledger/checkpoint，不回滚业务提交
- 是否需要 smoke：是；需验证 ACK→外环、budget_limited→外环、idle+未 ACK→内环、三次无进展→停止告警、Watchdog 重启→去重恢复

## 变更概述
新增一个 Rust 常驻 Watchdog，读取项目黑板、运行时 `events.jsonl`、goal
状态与 herdr agent 状态，将新 ACK、`blocked`、`escalation`、
`budget_limited`、以及“内环 idle 且存在未 ACK 条目”等信号转换为有界、
幂等的门铃。ACK 与负信号唤醒持有当前项目 `outer-duty` 的外环；普通空闲
续拍可唤醒内环。每次派出都必须重新挂正/负哨，并在外环裁决后回执内环。

监督器以持久游标和进展指纹去重。进展指纹至少覆盖黑板新增行、目标状态/
ledger 进展与 Git HEAD；同一任务最多自动续推 3 次，连续无进展则停止内环
重试、唤醒外环并产生明确告警。外环死亡或锁为 `VACANT` 时只告警，不自动
跨域夺锁；机器休眠、所有模型车道同时失效、operator-only 审批等场景不伪装
成成功。

本变更不以 Claude 的通用 `auto mode` 作为权限依据，不绕过 OpenLogos
guard，也不扩大 merge、verify、部署、smoke、archive、push 的授权范围。
只有用户显式选择 `openlogos next --auto` 时，现有全自动授权语义才适用；
即使如此，测试未收敛与 `gate:implement:loop-exhausted` 仍必须硬阻塞。
