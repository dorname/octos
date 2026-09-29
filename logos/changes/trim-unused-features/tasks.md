# 实现任务

## [delta] 规格变更

- [x] 产出 delta 文件到 `deltas/prd/1-product-requirements/core-01-requirements.md` — 基线数字与 1.1 通道数更新为裁剪后口径（9 IM 通道 / 20 平台 crate / 35 member）
- [x] 产出 delta 文件到 `deltas/prd/2-product-design/1-feature-specs/core-03-gateway-channels-design.md` — 通道清单 17 → 11（删 dingtalk/slack/line/email/qq-bot/wechat，注明 matrix 编译耦合保留原因）
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md` — L5 层移除 FFI 绑定、通道数字、crate 数 23→20
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/1-architecture/core-system-map.md` — crate 清单删 ffi/uniffi/pyo3 三行（REMOVED-ITEMS 点名）、member 38→35、通道 17→11、关键入口同步
- [x] ~~产出 delta 文件到 `deltas/prd/3-technical-plan/3-deployment/core-01-deployment-plan.md`~~ — 核查无实改（词边界核查零命中，见 NOTES.md）
- [x] ~~产出 delta 文件到 `deltas/prd/3-technical-plan/2-scenario-implementation/core-S04-gateway-channel.md`~~ — 核查无实改（以 Telegram 为示例、其余为泛称，见 NOTES.md）
- [x] ~~产出 delta 文件到 `deltas/test/core-S01-test-cases.md`~~ — 核查无实改（零命中，见 NOTES.md）
- [x] ~~产出 delta 文件到 `deltas/test/core-S16-test-cases.md`~~ — 核查无实改（零命中，见 NOTES.md）
- [x] 核查 core-04/05/07/08 feature-specs 与 core-02 需求详表中的零散通道引用 — 全部零命中，结果已记录于 `NOTES.md`

## [code] 代码实现

> **六维打分（2026-09-29 slice-planner）**：影响范围 2（跨 bus / cli / 根 Cargo.toml / scripts / docs）+ 行为复杂度 1（删除为主，feature 门分支清理）+ 契约变化 1（cargo feature 集对外可见变更）+ 测试规模 2（无新增用例，但全量 workspace 回归 = 多 crate 测试矩阵）+ 风险 1（被删 feature 的外部构建兼容性；git 历史即回滚）+ 不确定性 1（引用点已调研，`line` 词边界等少量待实现期核验）= **8 分，大任务，垂直拆分**。
>
> **测试 ID 口径**：本提案为纯删除，无新增 UT/ST ID；NOTES.md 已核查 S01/S16/S17 测试规格与被删对象零交集。各切片的验证口径 = **复用既有全量回归**（UT-S01-01..25 / ST-S01-01 / UT-S17 / ST-S17-01..14 等保持绿），外加提案承诺的门槛：`cargo build --workspace`、`cargo test --workspace`、`node scripts/check-mermaid.mjs`、canonical feature 安装命令可用。
>
> **删后续证伪门（逐片自检结论）**：
> - 切片 1（FFI 家族）：单独做、删掉 2/3 → FFI 三 crate 已核验无 workspace 内反向依赖，members 移除后 build+test 全绿 (a)✓；workspace 38→35 端到端可观察 (b)✓。
> - 切片 2（6 通道）：单独做、删掉 1/3 → 通道为 feature 编译解耦，切片内含全部引用点清理，build+test 全绿 (a)✓；通道实现 17→11 端到端可观察 (b)✓。与切片 1 互不依赖。
> - 切片 3（冗余清理 + 文档收口）：单独做、删掉 1/2 → 纯文档/脚本动作不影响编译，verify 全绿 (a)✓；仓库根与文档口径对齐端到端可观察 (b)✓。
> 三片均为垂直闭环（各自含代码 + 回归验证 + reporter），无横向工种切分，无前向依赖，按 1→2→3 串行。

- [x] 切片 1：删除 FFI 绑定家族（2026-09-30 完成）—— 已删 `crates/octos-ffi/`、`crates/octos-uniffi/`、`crates/octos-pyo3/`、`scripts/check-oup-bindings.py`（实现期裁决：oup-runtime 套件移除绑定检查环节，脚本一并删除）；根 `Cargo.toml` members 38→35；已清理 cli 侧 5 处 ffi 注释引用与 `octos-wasm` 注释/README、根 `README.md`/`README-zh.md` 嵌入章节；`scripts/milestone-ci.sh` oup-runtime 改为仅构建 `-p octos-cli`。验证：`cargo build --workspace` PASS（2m01s）；35 member 逐 crate 回归通过（octos-agent 2884+151、octos-cli 3927+151， reporter 见 `logos/resources/verify/test-results.jsonl` TRIM-S1-*）。环境伪失败已记录：cron_service_pg ×4（需本地 PG）、root 权限注入 ×7+1（CAP_DAC_OVERRIDE/HOME=/root）、execute_timeout_returns_error ×1（WSL2 procps kill 裸形式误杀进程组，详见 NOTES.md）
- [x] 切片 2：删除 6 个 IM 通道（2026-09-30 完成）—— 已删 `crates/octos-bus/src/{dingtalk,slack,line,email,qq_bot,wechat}_channel.rs` 及 `lib.rs` 导出；`octos-bus/Cargo.toml` 移除 6 feature 与 email 专属依赖（async-imap/tokio-rustls/webpki-roots/lettre/mailparse optional）；删 `crates/octos-cli/src/commands/gateway/adapters/` 中 6 适配器；清理 cli 侧 profiles（ChannelCredentials 变体/default_*/channel_to_entry/webhook_port 收口）、account、account_handler、api/admin（含 WeChat QR 区段 6672 行起全删）、api/auth_handlers（persist_wechat_bot_token/QR 端点）、api/router（4 条 wechat QR + line/dingtalk webhook 代理路由）、api/webhook_proxy（line/dingtalk 代理 fn）、channels、prompt、config（doctor VALID_CHANNELS +matrix）、process_manager（start_wechat_bridge 等 168 行）、gateway/mod 与 runtime（--wechat-bridge-url）、session_actor 注释、octos-agent message 工具描述示例；`octos-server/Cargo.toml` 同步删 6 转发 feature。保留决策：`is_registered_channel_name`/SessionKey 通道名单保留已删通道名（历史会话键兼容，注释标注）；octos-core types.rs:626 同理；otp.rs SMTP 属 dashboard OTP 功能非 email 通道。验证：`cargo build --workspace` PASS（1m30s，0 warning）+ canonical feature `cargo check` PASS + octos-bus 293 / octos-agent 2884 / octos-cli lib 3919（7 个失败全部定性：5 环境伪失败 + 2 预存失败，见 NOTES.md）+ octos-cli 集成 23/23 文件全绿 + octos-server 空壳 OK；reporter 见 `logos/resources/verify/test-results.jsonl` TRIM-S2-*（TRIM-S1-* 6 条因工作区 jsonl 被 S17 复跑覆盖，已自 8e975c7e 恢复追加）。trim 引入破损 1 处已修：profiles::test_config_from_profile 删 Slack 条目后 `channels.len()` 断言 2→1
- [x] 切片 3：冗余清理与文档收口（2026-09-30 完成）—— ① 根目录一次性笔记归档：`DEEP_RESEARCH_TIMEOUT_FIX.md`、`OUTER_LOOP_REVIEW.md`（untracked 历史外环黑板）→ `docs/archive/`（新建；`ORG-README.md` 为组织仓库导航非一次性笔记，保留并做数字对齐）。② scripts/ 审查结论：**零删除** —— 54 项逐一引用扫描 + 测试对象存在性核对，0 引用组中 9 个 `test-*` 的测试对象均存活于 `scripts/frp/` 等处（如 test-setup-caddy→frp/setup-caddy.sh），烟测（smoke-s16/s17）与 OpenLogos 场景 verify 绑定，其余为活跃工具（render_readme_diagrams/olp-board-append 等）；仅修 3 个脚本内已删通道 feature 引用（milestone-ci.sh FEATURES、local-tenant-deploy.sh 通道列表、install.sh 提示文案）。③ 文档数字对齐：`CLAUDE.md`（35-member/20 平台 crate、9 IM 通道/11 impls）、`ORG-README.md`（9 IM channels ×2 处）、**`book/` + `book-zh/` 文档站 16 文件 ~110 处**（channels.md 删 Slack/DingTalk/Email/WeChat 桥/LINE 五章 ×2 语言、installation/configuration/architecture/troubleshooting/introduction/advanced/skill-development 清单与示例对齐）、`crates/octos-bus/src/coalesce.rs` 删 `ChunkConfig::slack()` 死代码（切片 2 漏网，head 截断致漏检）。验证：`node scripts/check-mermaid.mjs` 35 块 0 失败 + `cargo build --workspace` 0 error 0 warning + octos-bus 293 PASS（cron_service_pg ×4 环境伪失败已 skip）；reporter TRIM-S3-*
