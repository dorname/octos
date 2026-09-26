# 实现任务

## [code] 代码实现
（本段在 plan 段留空：本提案需要代码实现，但 `[code]` 切片由 merge 后的 `slice-planner` 基于已合并规格和真实 UT/ST ID 统一规划。此处仅保留 `## [code]` 标题，勿提前填写切片项。）

> 测试 ID 说明：octos-web 为新纳入模块，`logos/resources/test/` 仅有 `core-*` 后端用例、无 octos-web 真实 UT/ST ID 可标注（与 archive/20260924-0018-fix-web-turn-terminal-fifo-wedge 同一先例）；本提案以行为契约为验收依据（见 proposal.md「变更概述」），测试 ID 注册表补建为模块纳入后的独立事项。

## [deploy] 部署任务
- [ ] 重新构建 octos-web dist（strict build）并注入本地 docker-desktop `octos` ns 的 web pod（tarball 放入 18088 真实服务根 /tmp/octos-k8s-bin/，旧包备份留存），滚动重启后确认 port-forward 127.0.0.1:5174 服务新 bundle，供操作员真机复验 issue #20 验收标准
