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

（本段在 plan 段留空：本提案需要代码实现（删 6 通道实现与引用、删 3 个 FFI crate、冗余脚本清理、CLAUDE.md/README/milestone-ci.sh 对齐），`[code]` 切片由 merge 后的 `slice-planner` 基于已合并规格和真实 UT/ST ID 统一规划。此处仅保留 `## [code]` 标题，勿提前填写切片项。）
