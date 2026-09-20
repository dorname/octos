# Deployment Report — S17 OctoLoop Night Watchdog

> 生成时间：2026-09-21（UTC+8）
> 变更提案：`octoloop-night-watchdog`
> 部署方案：`logos/resources/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md` §十

## 目标环境

- 隔离测试环境：WSL2（systemd 为 PID 1，`systemctl --user` 可用），隔离根目录 `/run/octos-watchdog-deploy.*`
  - 选 `/run` 的原因：unit 模板带 `PrivateTmp=yes`，`/tmp` 与 `/var/tmp` 在服务命名空间内不可见
- 被部署物：`octos-watchdog@<project-id>.service`（user unit）+ `<project-id>.toml` 项目配置
- 二进制：`target/debug/octos`（dev profile）
- 隔离 fixture：临时 git 项目（黑板 `.octos/OUTER_LOOP_REVIEW.md`、`goal-ledger.jsonl`、checkpoint、基线提交），假 `herdr`/`octoscode` adapter，未触达任何真实工作会话

## 执行命令摘要

| 步骤 | 命令 | 结果 |
|------|------|------|
| 安装 | `deploy/scripts/install-watchdog-user-service.sh --project <P> --octos-bin <B>` | OK；输出"已安装但未启用"，`is-enabled` 确认为默认不启用 |
| 挂接（隔离环境特需） | `systemctl --user link <unit>` | OK；隔离 `XDG_CONFIG_HOME` 对 manager 不可见，link 为标准挂接方式（真实安装使用默认 `~/.config`，无需此步） |
| 显式启用 | `systemctl --user enable --now octos-watchdog@<id>.service` | OK；3s 内生成 `state.json`，`is-active=active` |
| 只读状态 | `octos watchdog status --config <C> --json` | OK；sources 均 healthy，cursor 可读 |
| 崩溃恢复 | `kill -KILL <MainPID>` | OK；`Restart=always` 拉起新主进程，board/event cursor 与崩溃前一致 |
| 日志 | `journalctl --user -u <unit>` / `systemctl --user status` | WSL2 用户 journal 无持久化（`No journal files`）；以 `systemctl status` 摘要记录进程/内存/CGroup 证据 |
| 卸载/回滚 | `deploy/scripts/uninstall-watchdog-user-service.sh --project <P> --keep-state` | OK；unit 与配置移除，`daemon-reload` 后 `list-unit-files` 无残留 |

## 迁移结果

本变更无数据库/数据迁移；首次启动以 EOF/highwater 建立 baseline（`state.json` 原子写入，0600）。

## 服务启动结果

- `Active: active (running)`，Main PID 正常，内存 8.8M，CPU 21ms
- 崩溃恢复实测：`kill -KILL` 后 systemd 自动拉起，`status --json` 前后 cursor 完全一致（无信号重放）

## 回滚验证（不删除事实源）

| 事实源 | 卸载后状态 |
|--------|-----------|
| 黑板 `.octos/OUTER_LOOP_REVIEW.md` | 保留（ACK 行原文不变） |
| goal ledger `.octos/goal-ledger.jsonl` | 保留（字节级一致） |
| checkpoint 文件 | 保留（内容 `keep` 不变） |
| 业务 Git 提交 | 保留（HEAD 卸载前后一致） |
| 监督 state/alerts（`XDG_STATE_HOME`） | 保留（`--keep-state` 语义） |

## 回滚点

- 回滚即运行卸载脚本：只摘除 unit 与配置，不触碰黑板、goal ledger、checkpoint、Git 提交、herdr pane 或 outer-duty 元数据
- 重新部署：重跑安装脚本（幂等，配置已存在时不覆盖）

## 未解决风险

1. WSL2 用户 journal 非持久化，重启 WSL 后历史日志不可查；生产 Linux 默认持久化 journal 不受影响
2. 当前用户 linger 未启用：登出后服务是否继续运行由 operator 决定（安装脚本只提示、不代为修改，符合方案）
3. 安装脚本生成的默认配置不含 `[sources].events`：在 events 无法唯一发现（0 或多候选）时服务按设计 fail closed，operator 需按部署方案显式配置绝对路径（本次验收即按此操作）
4. `openlogos verify` 全量回归中另有 6 个与本变更无关的 chmod 只读 fixture 测试在 root 环境下失败（cron_panel/gateway/agent_orchestrator 等模块；root 免疫文件权限位），非 root CI 不受影响
