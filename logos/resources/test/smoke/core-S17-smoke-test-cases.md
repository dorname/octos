# core-S17 Watchdog smoke 测试用例

> 关联场景：S17（OctoLoop 夜间监督与有界续推）
> 关联部署方案：`core-01-deployment-plan.md` §十
> 环境：隔离 Linux user session、fixture 项目、假 herdr/outer-duty adapter 或专用测试 panes

## 用例列表

| ID | 检查项 | 期望 | 类型 |
|----|--------|------|------|
| SMOKE-S17-01 | unit 崩溃恢复 | kill 后 `Restart=always` 拉起，state cursor 保持 | auto |
| SMOKE-S17-02 | ACK→外环 | 基线后新 ACK 只向 HELD holder 投递一次 | auto |
| SMOKE-S17-03 | budget_limited→外环 | checkpoint 不变，外环收到一次证据指针 | auto |
| SMOKE-S17-04 | idle+未 ACK→内环 | 唯一 idle inner 收到最小未 ACK 条目指针 | auto |
| SMOKE-S17-05 | 三次无进展熔断 | 正好三次 prompt 后 fused，产生告警且无第 4 次 | auto |
| SMOKE-S17-06 | 重启去重 | Watchdog 重启后不重放已 delivered signal | auto |

## Runner 契约

```bash
./scripts/smoke-s17-watchdog.sh
```

runner 必须：

1. 创建临时 fixture 项目和独立 XDG_CONFIG_HOME/XDG_STATE_HOME；
2. 使用专用 user unit instance，不操作真实项目 Watchdog；
3. 清理 unit 与 fixture，但保留失败日志路径；
4. 将每个真实 ID 的结果追加到 `logos/resources/verify/smoke-results.jsonl`（或 `OPENLOGOS_SMOKE_RESULT_PATH`）；
5. 任一用例失败时退出非 0，不写 `SMOKE_PASS`。
