# 实现任务

## [code] 代码实现
> 删后续自检：六维打分 4 分（影响范围 3-5 文件=1；2-3 分支行为=1；无 wire 契约变化=0；测试 4-8 用例=1；bundle 易回滚=0；根因已定位、1 个待验证假设=1），0-7 分非大任务 → 单切片。症状 A/B/C 三处修复同属一条「hydrate 快照 → 投影渲染 → FIFO 结算」端到端链路，拆分后单片无法独立真机验收，故单片闭环。
> 测试 ID 说明：octos-web 为新纳入模块，`logos/resources/test/` 仅有 `core-*` 后端用例、无 octos-web 真实 UT/ST ID 可标注（与 archive/20260922-1943-web-ghost-queued-condition 同一先例）；本切片以行为契约为验收依据（见 proposal.md「变更概述」），测试 ID 注册表补建为模块纳入后的独立事项。
- [x] 单切片：① projection-store/hydrate-projection 消费 `projection_thread_sequences` checkpoints——快照内孤立 `TurnTerminal`（前驱 seq 因服务端压缩缺失）按 checkpoint 对齐 admitted 并结算 FIFO；canonical 快照缺消息行的 thread 回退 `messages` 转录行重建，reset→hydrate 循环不丢已渲染行；② projection-render-adapter 时间戳 fallback 去 seq 化（回退接收时刻或隐藏，禁 epoch）；③ 清理 ui-protocol-runtime.ts 调试 console.log；同步 UT（孤立 terminal+checkpoint→admitted 结算 / compacted thread 转录回退渲染 / 缺 persisted_at→非 epoch 三类用例 + 既有套件不回退）+ OpenLogos reporter 写入 logos/resources/verify/test-results.jsonl（octos-web 262e79b：vitest 1112 passed，72 失败均为预存失败；tsc -b + vite build PASS）

## [deploy] 部署任务
- [ ] 重新构建 octos-web dist（strict build）并注入本地 docker-desktop `octos` ns 的 web pod（nginx webroot），滚动重启后重挂 port-forward，供操作员真机复验 issue #19 验收标准
- [ ] 若服务端（crates/octos-cli）改动落地：musl 重建注入 k8s（路径同 #12 修复），本地 fixture smoke + k8s WS 复验
