# 实现任务

## [delta] 规格变更
- [x] openlogos adopt 接入仓库(logos/ 结构、AGENTS.md、CLAUDE.md OPENLOGOS 段、.claude 插件与钩子)
- [x] 归档 agent-spec 历史合约:specs/ → logos/changes/archive/agent-spec-legacy/(19 份,附 README)
- [x] 修正 docs/requirements/REQ-SERVE-BP-001.md、docs/ARC_AGENT_TASK_MCP.md 中的 specs/ 引用路径
- [x] 验证:`cargo check -p octos-llm -p octos-cli --features "api,telegram,discord,whatsapp,feishu,twilio,wecom,wecom-bot,audio_mp3"` 通过;`openlogos status` 正常显示
