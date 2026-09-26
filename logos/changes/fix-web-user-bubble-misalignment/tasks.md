# 实现任务

## [code] 代码实现

切片评分（slice-planner 六维）：影响范围 1（3-4 文件）+ 行为复杂度 1（2-3 分支）+ 契约 0（无 wire 变更）+ 测试 1（4-8 用例）+ 风险 0（易回滚）+ 不确定性 1 = **4 分 → 单切片**。删后续自检：无后续切片可删。

### 切片 S1：turn 归并 + ghost 锚定（唯一切片，闭环）

- [x] S1.1 `octos-web/src/store/projection.ts` — `projectWithMetrics` 增加按 `turn_id` 归并 ThreadView 的后处理：同 turn 的 user/assistant/tool/terminal 帧分属不同 thread_id 时合并为一个块，位置取组内最早出现处（行为契约 ①③）
- [x] S1.2 `octos-web/src/components/thread-ghost-layout.ts`（新增纯函数模块）— `layoutThreadsWithGhosts`：`ghost.clientMessageId === thread.turnId || thread.id` 的 ghost 锚定到其 turn 的 thread 正前方；锚定目标为 placeholder 线程时标记抑制空 user 占位行；未锚定 ghost 保持垫底（行为契约 ②）
- [x] S1.3 `octos-web/src/components/chat-thread.tsx` — ThreadList 改按 layout 渲染；ThreadView 支持抑制空 user 占位行（仅 placeholderOrigin 线程）
- [x] S1.4 回归测试（RED→GREEN）：
  - `projection.test.ts`：同 turn 分 thread 到达 → 单块且用户在前；assistant thread 先 admitted → 归并块保持在最早位置（排序稳定）；不同 turn 不归并（对照）
  - `thread-ghost-layout.test.ts`（新增）：ghost 锚定在其 turn 上方；placeholder 抑制标记；未锚定 ghost 垫底
  - `chat-thread-projection-v2.test.tsx`：组件级端到端——同 turn user/assistant 分 thread ingest → DOM 单 bundle、用户气泡在助手上方
- [x] S1.5 OpenLogos reporter：测试结果写入 `logos/resources/verify/test-results.jsonl`

> 测试 ID 说明：octos-web 为新纳入模块，`logos/resources/test/` 仅有 `core-*` 后端用例、无 octos-web 真实 UT/ST ID 可标注（与 archive/20260924-0018-fix-web-turn-terminal-fifo-wedge 同一先例）；本提案以行为契约为验收依据（见 proposal.md「变更概述」），测试 ID 注册表补建为模块纳入后的独立事项。

## [deploy] 部署任务
- [ ] 重新构建 octos-web dist（strict build）并注入本地 docker-desktop `octos` ns 的 web pod（tarball 放入 18088 真实服务根 /tmp/octos-k8s-bin/，旧包备份留存），滚动重启后确认 port-forward 127.0.0.1:5174 服务新 bundle，供操作员真机复验 issue #20 验收标准
