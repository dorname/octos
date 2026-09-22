# 实现任务

## [code] 代码实现
> 删后续自检：六维打分 1 分（影响范围 2-3 文件=1；单路径 bugfix=0；无契约变化=0；测试 1-3 用例=0；易回滚=0；原因已明=0），0-7 分非大任务 → 单切片。
> 测试 ID 说明：octos-web 为新纳入模块，`logos/resources/test/` 仅有 `core-*` 后端用例、无 octos-web 真实 UT/ST ID 可标注；本切片以行为契约为验收依据（行为契约见 proposal.md「变更概述」），测试 ID 注册表补建为模块纳入后的独立事项。
- [x] 单切片：将 `dispatchGhostQueued`（octos-web/src/runtime/ui-protocol-send.ts）的 `queued` 判定从 `total > 1` 改为「该消息经会话 FIFO 生命周期门驻留（未真正上 wire）」这一直接事实；保持 `total > 1` 场景行为不回退；同步 chat-thread.tsx 的 GhostSpec.queued 传递与 GhostBubble.tsx 的 30s 定时器门控语义自洽；含 UT（单条驻留→queued=true 不点火；total>1→queued=true；live→queued=false 点火 三类用例）+ OpenLogos reporter 写入 logos/resources/verify/test-results.jsonl

## [deploy] 部署任务
- [x] 重新构建 octos-web dist（npm run build，strict）并打包注入本地 docker-desktop `octos` ns 的 web pod（nginx webroot），滚动重启后重挂 port-forward，供操作员真机复验
