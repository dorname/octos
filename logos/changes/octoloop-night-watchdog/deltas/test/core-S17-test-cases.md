# Delta: test — core-S17-test-cases.md

> target: logos/resources/test/core-S17-test-cases.md(全新文档)

## ADDED — core-S17 OctoLoop 夜间监督与有界续推 — 测试用例

# core-S17 OctoLoop 夜间监督与有界续推 — 测试用例

> 场景：独立 Watchdog 读取 OLP 黑板、events、goal/ledger 与 Git 进展，按 outer-duty 权威通过 herdr 有界唤醒内外环。
> 设计来源：`core-S17-octoloop-night-watchdog.md`、`core-08-octoloop-watchdog-design.md`。
> Reporter：每个自动化用例必须以本表 ID 追加 OpenLogos JSONL 记录到 `logos/resources/verify/test-results.jsonl`；未写 reporter 的测试不计入验收。

## 单元测试

| ID | 描述 | 前置/输入 | 预期输出 |
|----|------|-----------|----------|
| UT-S17-01 | 黑板基线仅观察新增域 | 启动前已有历史 ACK，启动后无新增 | 不产生 signal |
| UT-S17-02 | v1 ACK 分类 | 基线后新增 done/wontdo/blocked ACK | 分别生成唯一 `board_ack` signal |
| UT-S17-03 | 运行时负信号分类 | blocked、escalation、budget_limited 事件 | 生成对应 typed signal 与 object id |
| UT-S17-04 | 稳定 signal id | 同 project/source position/kind/object 重算 | id 完全相同 |
| UT-S17-05 | delivered 去重 | state 已含 signal id | 重扫不再调用 prompt sender |
| UT-S17-06 | cwd 精确匹配 | 相同前缀、软链与 canonical path 候选 | 仅 canonical 完全相等者匹配 |
| UT-S17-07 | 多候选 fail closed | 同角色两个匹配候选 | ambiguous，不选择 focused/最近活动者 |
| UT-S17-08 | HELD 外环可投递 | duty=HELD 且 holder/agent 唯一对应 | 允许生成 outer dispatch |
| UT-S17-09 | VACANT/ERROR 不投递 | duty=VACANT、ERROR、unsupported、timeout | pending+alert，不 prompt、不 acquire |
| UT-S17-10 | 指纹覆盖三类进展 | board highwater、goal/ledger highwater、Git HEAD 逐一变化 | 任一变化均生成新 fingerprint |
| UT-S17-11 | 状态抖动不算进展 | 仅 agent working↔idle 或日志时间变化 | fingerprint 不变 |
| UT-S17-12 | 有进展重置计数 | retry_count>0 后 fingerprint 变化 | retry_count=0、fuse=false |
| UT-S17-13 | 第三次无进展熔断 | 同任务连续三个观察窗无变化 | retry_count=3、fused=true、禁止第 4 次 dispatch |
| UT-S17-14 | 原子状态往返 | pending/delivered/retry/fuse 完整 state | 写入再读取字段不丢失，版本校验通过 |
| UT-S17-15 | 损坏状态 fail closed | 非法 JSON、版本不支持、project 不匹配 | 错误+告警，不归零 cursor/retry |
| UT-S17-16 | 事件轮转 generation | file identity 改变或 truncate | 新 generation，旧 delivered id 不重放 |
| UT-S17-17 | 投递回执状态机 | accepted、blocked、stalled、timeout | 仅 accepted 写 delivered，其余保留 pending |
| UT-S17-18 | 配置安全校验 | 无限 retry、相对 project、任意 shell 字段 | 拒绝配置并给出字段级错误 |
| UT-S17-19 | status 只读 | 带 pending/fuse 的 state | 输出完整 JSON，不改变 cursor/mtime、不调用适配器 |
| UT-S17-20 | 告警脱敏 | 含 token/完整 prompt 的底层错误 | 输出不含凭据与完整 prompt |

## 场景测试

| ID | 描述 | 覆盖 Steps | 前置条件 | 操作序列 | 预期结果 |
|----|------|------------|----------|----------|----------|
| ST-S17-01 | 首启建立基线 | 1–7 | fixture 含历史 ACK/events | run-once 两次且无新增 | 两次均不 prompt；state 保存 EOF/highwater |
| ST-S17-02 | 新 ACK 唤醒 holder 一次 | 5–12a | duty HELD，唯一 outer | 追加 ACK，run-once，再 run-once | 第一次 outer accepted；第二次零投递 |
| ST-S17-03 | blocked/escalation 唤醒外环 | 5–12a | duty HELD | 分别追加两类事件 | 每个新事件一个独立 signal，均指向证据 |
| ST-S17-04 | budget_limited 保留 checkpoint | 5–13a | goal fixture 有 checkpoint | 追加 budget_limited 事件 | outer 收门铃；checkpoint 未改；goal 未伪装成功 |
| ST-S17-05 | idle 未 ACK 唤醒内环 | 8b–12b | 唯一 idle inner，Active 有未 ACK | 超过 idle 阈值后 run-once | inner 收到最小未 ACK 条目指针 |
| ST-S17-06 | 真实进展清零 | 16–17 | 已有 retry_count=2 | inner 追加 ACK 或 Git HEAD 前移 | 下一周期 retry=0、fuse=false |
| ST-S17-07 | 三次无进展熔断 | EX-17.1 | 固定时钟/不变源 | 连续跨过三个观察窗 | 正好三次 inner prompt；随后 fuse+alert，无第 4 次 |
| ST-S17-08 | Watchdog 重启恢复 | 2、12、17 | 已 delivered signal 与 retry state | 终止并重启后 run-once | 不重放 delivered；retry/fuse 保持 |
| ST-S17-09 | outer-duty VACANT/ERROR | EX-9a.1 | 有外环候选但 duty 不可信 | 产生 ACK/负信号 | 零 outer prompt；pending+operator alert |
| ST-S17-10 | 多项目/多外环隔离 | 3–4 | 两项目、两个 outer pane | 项目 A 产生信号 | 只触达 cwd=A 且持 A 锁的 outer |
| ST-S17-11 | herdr 故障恢复 | EX-10a.1 | prompt 第一次 timeout、第二次 accepted | 两周期投递 | 首次 pending，后续 accepted；业务 retry 不误增 |
| ST-S17-12 | 事件轮转不断档 | EX-5.1 | 可控 old/new file identity | 消费旧尾部后 rotate 并写新事件 | 新事件处理一次，旧事件不重放 |
| ST-S17-13 | systemd 单实例崩溃拉起 | 部署 10.2 | 隔离 user service | kill Watchdog 主进程 | systemd 拉起；state 恢复；无重复 prompt |
| ST-S17-14 | 授权边界 | EX-13a.1 | fixture 进入 OpenLogos human gate/loop-exhausted | 运行多个监督周期 | 只有状态门铃，无 merge/verify/deploy/smoke/archive/push 动作 |

## 覆盖门禁

- 自动化分母：UT-S17-01..20、ST-S17-01..14；需要真实 systemd 的 ST-S17-13 可在不支持环境标记 skipped，但部署验收环境必须执行。
- 每个 ST 使用隔离临时项目、假 agent adapters 或专用 panes，不触达真实工作会话。
- `cargo test`、focused ST runner、fmt 与 clippy 的命令及 Git commit 必须写入 reporter evidence。
- 实现前按这些 ID 划分代码切片；每片同时提交业务代码、对应 UT/ST 与 reporter。
