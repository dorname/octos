# Delta: prd/3-technical-plan/3-deployment — core-01-deployment-plan.md

> target: logos/resources/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md

## ADDED — 十、S17 Watchdog 本地常驻部署

## 十、S17 Watchdog 本地常驻部署

### 10.1 支持范围与默认状态

- 第一阶段部署目标：Linux systemd user service；本地开发机与隔离测试环境。
- 默认只安装 unit/template，不执行 `enable`；operator 明确启用后才具备常驻监督语义。
- macOS/Windows 可运行 `octos watchdog run` / `run-once`，但本变更不提供 launchd/Windows Service，也不宣称崩溃自动拉起。
- 无业务数据库迁移；本地监督状态可删除重建，但删除会重新建立 baseline。

### 10.2 unit 契约

模板单元建议为 `octos-watchdog@.service`，实例配置通过 `%i` 映射到已安装的项目配置文件：

```ini
[Unit]
Description=Octos OctoLoop watchdog (%i)
After=default.target

[Service]
Type=simple
ExecStart=%h/.local/bin/octos watchdog run --config %h/.config/octos/watchdog/%i.toml
Restart=always
RestartSec=5s
NoNewPrivileges=yes
PrivateTmp=yes

[Install]
WantedBy=default.target
```

最终 unit 路径与二进制安装路径由安装脚本探测并渲染，禁止假设 `%h/.local/bin` 必然存在。unit 不携带 LLM/API 凭据；Watchdog 只调用本机 herdr 与只读 outer-duty check。

### 10.3 安装与启用

```bash
# 安装模板与指定项目配置，但不启用
./deploy/scripts/install-watchdog-user-service.sh --project /absolute/project

# operator 明确启用
systemctl --user daemon-reload
systemctl --user enable --now octos-watchdog@<project-id>.service

# 状态与日志
systemctl --user status octos-watchdog@<project-id>.service
journalctl --user -u octos-watchdog@<project-id>.service
octos watchdog status --project /absolute/project --json
```

若用户 session 在登出后仍需运行，`loginctl enable-linger <user>` 属 operator 决策，安装脚本只检测并提示，不自动修改系统登录策略。

### 10.4 状态、日志与权限

| 资源 | 建议位置 | 权限/生命周期 |
|------|----------|---------------|
| 配置 | `$XDG_CONFIG_HOME/octos/watchdog/<project-id>.toml` | 0600；不含模型凭据 |
| 状态 | `$XDG_STATE_HOME/octos/watchdog/<project-id>/state.json` | 0600；原子替换；升级前备份 |
| 告警 | 同状态目录 `alerts.jsonl` | 0600；append-only，可轮转 |
| 服务日志 | systemd journal | 不输出完整 prompt/凭据 |

Watchdog 与被监督项目使用同一非 root 用户运行，确保可读黑板与 Git，但不额外授予 root、Docker socket 或免沙箱权限。

### 10.5 升级与回滚

升级：先 `stop` 服务，备份 state，替换二进制/unit，`daemon-reload` 后启动并执行 status/run-once 检查。state schema 不兼容必须显式迁移或 fail closed，禁止静默归零 fuse。

回滚：

```bash
systemctl --user disable --now octos-watchdog@<project-id>.service
./deploy/scripts/uninstall-watchdog-user-service.sh --project /absolute/project --keep-state
```

回滚只停用/移除 Watchdog unit 与配置；默认保留 state/alerts，且绝不删除 `.octos/OUTER_LOOP_REVIEW.md`、goal ledger、checkpoint、Git commit、herdr pane 或 outer-duty 元数据。

### 10.6 S17 部署后检查与冒烟

| ID | 检查项 | 期望 |
|----|--------|------|
| SMOKE-S17-01 | unit 崩溃恢复 | 主进程退出后 systemd 自动拉起，state cursor 保持 |
| SMOKE-S17-02 | ACK→外环 | 新 ACK 只向 HELD holder 投递一次 |
| SMOKE-S17-03 | budget_limited→外环 | checkpoint 保留，外环收到证据指针 |
| SMOKE-S17-04 | idle+未 ACK→内环 | 唯一 cwd 匹配的 idle 内环收到一次指针 |
| SMOKE-S17-05 | 三次无进展熔断 | 第三观察窗后 fused，停止第 4 次 prompt 并告警 |
| SMOKE-S17-06 | 重启去重 | 服务重启不重放已 delivered signal |

冒烟 runner 必须使用隔离 fixture 项目、假 herdr/duty adapter 或专用测试 panes，禁止向真实生产 agent 注入。结果追加至 `logos/resources/verify/smoke-results.jsonl`。
