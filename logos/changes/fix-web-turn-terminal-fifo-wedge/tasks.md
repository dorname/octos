# 实现任务

## [code] 代码实现
（本段在 plan 段留空：本提案需要代码实现，但 `[code]` 切片由 merge 后的 `slice-planner` 基于已合并规格和真实 UT/ST ID 统一规划。此处仅保留 `## [code]` 标题，勿提前填写切片项。）

> 测试 ID 说明：octos-web 为新纳入模块，`logos/resources/test/` 仅有 `core-*` 后端用例、无 octos-web 真实 UT/ST ID 可标注；本提案以行为契约为验收依据（见 proposal.md「变更概述」），测试 ID 注册表补建为模块纳入后的独立事项。

## [deploy] 部署任务
- [ ] 重新构建 octos-web dist（strict build）并注入本地 docker-desktop `octos` ns 的 web pod（nginx webroot），滚动重启后重挂 port-forward，供操作员真机复验 issue #19 验收标准
- [ ] 若服务端（crates/octos-cli）改动落地：musl 重建注入 k8s（路径同 #12 修复），本地 fixture smoke + k8s WS 复验
