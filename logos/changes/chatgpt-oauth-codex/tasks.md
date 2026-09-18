# 实现任务

## [delta] 规格变更
- [x] 产出测试规格 delta:`deltas/test/core-S01-test-cases.md`(场景 S01 ChatGPT 订阅登录与 Codex 后端对话,UT-S01-01..40 + ST-S01-01 [manual])

## [code] 代码实现
> 删后续自检:评分 8+(跨 octos-cli/octos-llm 两 crate、异步刷新流程、多命令接线),按凭证流向垂直切 2 片;任一片删除后续都可独立过 verify(auth 层可单独测;Codex provider 层依赖 auth 产出的 credential 但有 ApiKey 兜底路径)。代码已随本提案一并实现(存量 WIP 纳入),两片均已落地并全绿。合并期修复:init.rs 中 stash 版 minimax-cn preset 与上游重复注册导致 2 个上游测试失败,已去重保留上游版本。
- [x] 切片1:auth 层——新 deviceauth JSON 设备码流、JWT claim 解析、阻塞式刷新与凭证合并、`AuthCredential.account_id`、`ResolvedCredential` 分类与 `resolve_credential` 自动刷新、auth status/init 接线(覆盖 UT-S01-01..09、UT-S01-18..34)
- [x] 切片2:Codex provider 层——`openai_responses` Codex 后端模式(base_url/headers/`store:false`/`instructions`)、registry 接线与订阅模型识别(覆盖 UT-S01-10..17、UT-S01-35..40,ST-S01-01 [manual] 由人工端到端覆盖两片)
