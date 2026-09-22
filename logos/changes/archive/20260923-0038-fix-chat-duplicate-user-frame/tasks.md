# 实现任务

## [delta] 规格变更
- [x] `core-04-serve-api-design.md` 附录增补：WS `user_message` envelope 与 `session/hydrate` messages 的 `client_message_id` 贯通契约（turn_id == client_message_id 盖章规则 + 前端 cmid 去重语义）→ 已合并为 1附.4

## [code] 代码实现
- [x] 切片 1（core 后端）：WS `turn/start` persist loop 对首个无 cmid 的 user 行以 `client_message_id = Some(turn_id)` 盖章（steer 行保持 None）；live envelope（commit observer 转发）与 hydrate messages（既有序列化）自动携带 cmid。Rust UT 3 个（CORE-DUPFRAME-01/02）+ OpenLogos reporter
- [x] 切片 2（octos-web 前端）：(a) `projection-store.ts` `ingestCanonical` user_message 跨线程 cmid 对称去重；(b) **rehydrate 热循环熔断**（诊断中发现 60s 内 6626 次 session/hydrate）：快照重放入口的同步递归请求延迟到宏任务 + 无进展 hydrate 指数退避（250ms→30s 封顶，进展即复位）。vitest UT（OW-DUPFRAME-01..03）+ OpenLogos reporter

## [deploy] 部署任务
- [x] musl 重建 octos 二进制 + 重建 octos-web dist，注入 k8s pod；playwright 真机复验：发送消息 90s 内主区 user frame == 1，刷新后仍 == 1 ✅（另实证：live envelope 与 hydrate 快照均携带 cmid=turn_id；90s 窗口 session/hydrate RPC 仅 19 次，修复前同窗口 6626 次）
