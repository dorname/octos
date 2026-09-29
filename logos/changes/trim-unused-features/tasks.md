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
- [ ] 切片 2：删除 6 个 IM 通道 —— 删 `crates/octos-bus/src/{dingtalk,slack,line,email,qq_bot,wechat}_channel.rs` 及 `lib.rs` 导出；`octos-bus/Cargo.toml` 移除对应 feature 与 email 专属依赖（async-imap/lettre/mailparse）；删 `crates/octos-cli/src/commands/gateway/adapters/` 中对应适配器；清理 cli 侧 config/profiles/channels/account/api/webhook_proxy 等引用点与 `octos-cli/Cargo.toml` feature 定义（注意 `line` 需词边界精确匹配，避免误伤文本行处理代码；`email` 需区分通道与 send-email 技能）。验证：`cargo build --workspace` + `cargo test --workspace` 全绿 + canonical 安装命令（`--features "api,telegram,discord,whatsapp,feishu,twilio,wecom,wecom-bot,audio_mp3"`）可用 + OpenLogos reporter（复用全量回归，无新增 UT/ST）
- [ ] 切片 3：冗余清理与文档收口 —— 根目录一次性修复笔记（DEEP_RESEARCH_TIMEOUT_FIX.md、OUTER_LOOP_REVIEW.md 等，实现期列全清单）归档至 `docs/archive/`；`scripts/`（55 个）逐个审查，删除明确遗留/重复脚本（删除清单写入本任务勾选备注）；`README.md`、`CLAUDE.md` 数字与清单对齐裁剪后口径（9 IM 通道 / 11 通道实现 / 20 平台 crate / 35 member）。验证：`node scripts/check-mermaid.mjs` 0 失败 + `cargo build --workspace` 仍绿 + OpenLogos reporter（复用全量回归，无新增 UT/ST）
