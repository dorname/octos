# 实现任务

## [delta] 规格变更
- [ ] 无（错误路径行为收紧，不改对外契约形状）

## [code] 代码实现
- [ ] RetryProvider 鉴权失败不重试断言（UT-S16-27/28，crates/octos-llm/src/retry.rs 测试区补）
- [ ] turn/error 上游鉴权摘要断言（UT-S16-29，crates/octos-cli turn 落态路径补）
- [ ] 用例登记追加到 logos/resources/test/core-S16-test-cases.md（UT-S16-27/28/29）
- [ ] reporter：test-results.jsonl 追加三行 pass 记录

## 验收
- UT-S16-27/28/29 全过（401/403 不重试；401 turn/error 带摘要）
- `openlogos merge serve-auth-failfast` 执行后经 ask_outer 发通知（条目 #8/待验收/slug/commit/测试结果）
- CPU 限载：JOBS=1、threads=4 全程
