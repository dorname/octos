# NOTES — 核查结论（无实改文件）

2026-09-29 对以下文件做被删通道（dingtalk / slack / line / email / qq-bot / wechat）与 FFI（octos-ffi / uniffi / pyo3）词边界精确核查（`grep -niE '\b(...)\b'`），结论均为**无引用、无需 delta**：

| 文件 | 结论 |
|---|---|
| `prd/3-technical-plan/3-deployment/core-01-deployment-plan.md` | 无被删通道 / FFI 引用；feature 清单仅泛写 `--features "api,..."`，不含具体被删 feature |
| `prd/3-technical-plan/2-scenario-implementation/core-S04-gateway-channel.md` | 以 Telegram 为示例通道，其余均为「通道适配器」泛称，无被删通道具名引用 |
| `test/core-S01-test-cases.md` | 无被删通道 / FFI 相关用例（早前粗筛命中为 `baseline` 等子串误报） |
| `test/core-S16-test-cases.md` | 同上，无引用 |
| `prd/2-product-design/1-feature-specs/core-04-serve-api-design.md` | 无引用 |
| `prd/2-product-design/1-feature-specs/core-05-orchestration-design.md` | 无引用 |
| `prd/2-product-design/1-feature-specs/core-07-k8s-cluster-design.md` | 无引用 |
| `prd/2-product-design/1-feature-specs/core-08-octoloop-watchdog-design.md` | 无引用 |

因此 tasks.md 中对应 5 项 delta 任务以「核查无实改」闭环，不产 delta 文件。

## 切片 1 验证记录（2026-09-30）

### 门槛结果

| 门槛 | 结果 |
|---|---|
| `cargo build --workspace` | PASS（2m01s，35 member） |
| 逐 crate 全量回归 | 33/35 crate 全绿；octos-agent 2884+151 全绿；octos-cli 3927+151 通过、7 个 root 环境伪失败（见下） |

### 环境性失败（均预存，非 trim 引入；trim 对应文件零改动）

1. **octos-bus `cron_service_pg` ×4**：需本地 PostgreSQL（默认 `postgres://postgres:octos@127.0.0.1:5432/octos`），本机 5432 未监听（k8s PG 容器未映射宿主端口，不动用户集群数据）。门控跳过 `--skip cron_service_pg`。
2. **root 权限注入伪失败 ×7（octos-cli）+ ×1（octos-pipeline）**：测试用 chmod 0o555/0o000 注入「不可写/不可读」失败，root 的 CAP_DAC_OVERRIDE 击穿权限检查导致注入失效；另有 1 例 HOME=/root 触发 `workspace_root_escape_under_system_path` 守卫（/root 属受限系统路径）。**root 下必现，普通用户环境应通过**。门控跳过对应测试名。
3. **octos-agent `execute_timeout_returns_error` ×1**：WSL2 下 `/usr/bin/kill`（procps-ng）对**不带 `--` 的负 pid 参数**解析歧义，实际执行 `kill(0, SIGKILL)` 团灭调用者进程组（strace + 隔离实验确证）。触发点 `plugins/tool.rs:2938` 与 `:3056`（guard :2361 的 `--` 形式正确）。macOS/BSD 与 util-linux kill 不受影响——解释了测试注释「passes on macOS and bare-metal Linux」。该测试有 docker 检测跳过门，但 WSL2（`0::/init.scope`）不命中。**这是潜在生产级 bug：Linux + procps 环境下任何插件 stdin 超时/等待超时都会团灭 octos 进程组**。

### 上述第 3 项的修复建议（未实施——按守卫规则需另立变更提案）

`crates/octos-agent/src/plugins/tool.rs` 两处 `args(["-9", &format!("-{child_pid}")])` 改为 `args(["-9", "--", &format!("-{child_pid}")])`（与 :2361 guard 一致），并给该测试补 WSL2 跳过条件（如 `systemd-detect-virt = wsl` 或 `/proc/version` 检测）。

### 本切片实际改动清单

- 删除：`crates/octos-ffi/`（7 文件）、`crates/octos-uniffi/`（6 文件）、`crates/octos-pyo3/`（8 文件）、`scripts/check-oup-bindings.py`
- 根 `Cargo.toml`：members 38 → 35
- `scripts/milestone-ci.sh`：oup-runtime 去除 ffi/uniffi 构建与 check-oup-bindings 调用
- `crates/octos-cli/src/{config.rs,profiles.rs,runtime/profile.rs,commands/chat.rs,commands/mod.rs}`：ffi 提及注释中性化（5 处，无逻辑改动）
- `crates/octos-wasm/{src/lib.rs,Cargo.toml,README.md}`、根 `README.md`、`README-zh.md`：ffi 嵌入章节/提及清理
