# 变更提案：fix-chat-header-debug-leak

> module: octos-web | created: 2026-09-22

## 变更原因
Issue #13（fork dorname/octos）：chat 会话页头部把内部状态字段渲染给用户——`Router` 折叠按钮与字符串 `none`（issue 还提到数字 `1`，疑似 ghost-queued 残留气泡，已由 octos-web #63 修复，需真机复验）。已定位两处确定泄漏：

1. **`Router` pill**（`router-mode-menu.tsx:59`）：`adaptiveMode === null`（会话 provider 链未包装 AdaptiveRouter——单 provider profile 的常态）时 pill 仍渲染，文案回退为裸 `Router`；点开后 switcher 对不可用链也只剩三个禁用按钮。这是一个对终端用户无意义的工程师控件。
2. **`none`**（`cost-bar.tsx:20-21`）：`visible = displayModel && displayModel !== "none" ? displayModel : provider`——model 为 `"none"` 时回退到 provider，但 provider 同为 `"none"` 时未拦截，直接渲染字符串 `none`（sidebar footer 的 WorkbenchStatusPill 已有 `!== "none"` 双重防护，cost-bar 漏了 provider 侧）。

## 变更类型
代码级（前端渲染条件修正，无协议变化）。

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：无（行为契约由 issue 期望定义：只渲染用户可见字段）
- 影响的业务场景：chat 会话页头部
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无

## 部署影响
- 是否需要部署：是
- 部署原因：k8s web pod 运行旧 dist；需重建 dist 并注入验证
- 影响环境：本地（docker-desktop k8s `octos` ns）
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：是（真机/headless 复验：单 provider 会话头部不再出现 Router pill 与 none；自适应路由会话 pill 仍出现且可用）

## 变更概述
1. `RouterModeMenu`：`adaptiveMode === null` 时整体返回 `null`（无自适应链就没有可切换的模式，控件对用户无意义）；有模式时维持现状。
2. `CostBar`：`visible` 解析后再判 `visible === "none"` 一并返回 `null`，与 sidebar footer 的防护口径对齐。

含 vitest UT（RouterModeMenu：null→不渲染、有模式→渲染；CostBar：model/provider 均 none→不渲染、provider 为 none 但 model 有效→渲染 model）+ OpenLogos reporter。issue 中的 `1` 若真机复验仍存在则另立提案。
