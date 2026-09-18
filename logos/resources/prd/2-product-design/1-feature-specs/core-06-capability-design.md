# core-06 能力扩展与韧性 — 功能规格

> 覆盖场景：S08（记忆沉淀与检索复用）、S09（技能插件安装与使用）、S10（MCP 服务器接入与工具扩展）、S12（LLM 故障转移与自适应路由）
> 需求来源：core-01-requirements.md（Phase 1）
> 配套原型：`core-06-capability-terminal.md`

## 一、S08: 记忆沉淀与检索复用 — 交互规格

### 1.1 `octos memory`

**命令格式**：
- `octos memory status` — 查看记忆库状态
- `octos memory search <QUERY> [--kind <K>] [--source <S>] [--limit <N>]` — 检索
- `octos memory refresh [--dry-run]` — 驱动 memory-refresh 流水线（提取/合并近期经验）
- `octos memory reindex` — 重建检索索引
- `octos memory add --kind <KIND> <TEXT...> [--sensitive]` — 手工写入笔记

**记忆体系**：
- **EpisodeStore**（`.octos/episodes.redb`）：任务完成摘要（save_episodes 开启时由 agent 自动写入）
- **MemoryStore**（MEMORY.md + 每日笔记）：长期偏好与决策，7 天窗口注入系统提示词
- **HybridSearch**：BM25 + 向量（HNSW）混合排序，默认权重 0.7 向量 / 0.3 BM25；无 embedding provider 时降级 BM25-only

**交互流程**：
1. agent 完成任务 → 摘要自动写入 EpisodeStore（含任务类型、结果、修改文件）
2. 用户可随时 `octos memory search "鉴权"` 验证检索命中
3. `octos memory refresh` 触发提取/合并 pass：近期会话/episode 中的经验提炼进 MEMORY.md 与每日笔记
4. 新会话构建系统提示词时，相关记忆自动注入（用户无感）
5. 敏感内容用 `--sensitive` 标记，注入与检索时受约束

#### 验收条件（交互级）

##### 正常：检索命中已沉淀经验
- **GIVEN** agent 昨天完成了"修复 OAuth 刷新"任务（episode 已写入）
- **WHEN** 用户运行 `octos memory search "OAuth 刷新" --limit 5`
- **THEN** 输出按混合排序的相关条目（含昨日 episode 摘要、来源类型、时间）；退出码 0

##### 正常：refresh 干跑
- **GIVEN** 近 7 天有若干会话与 episode
- **WHEN** 用户运行 `octos memory refresh --dry-run`
- **THEN** 输出将被提取/合并的候选条目与目标文件，但不实际写入；再次不带 --dry-run 运行后 MEMORY.md/每日笔记更新

##### 异常：无 embedding 降级
- **GIVEN** 未配置 embedding provider
- **WHEN** 用户运行 `octos memory search "部署"`
- **THEN** 输出中包含降级提示（BM25-only）且正常返回结果，退出码 0

## 二、S09: 技能插件安装与使用 — 交互规格

### 2.1 `octos skills`

**命令格式**：`octos skills <list|install|remove> [NAME]`

**插件模型**：技能是自带 `manifest.json` 的自包含二进制（声明 id/version/tools/requires 等）；发现按优先级扫描目录（profile > user > bundled > legacy）；门控检查二进制存在性、必需环境变量、OS 匹配；调用走二进制协议 `./binary <tool_name>`（JSON stdin → JSON `{success, output, files_to_send}` stdout）。`spawn_only: true` 的工具在 agent 循环中自动转后台执行，SKILL.md 自动注入系统提示词。

**交互流程**：
1. `octos skills list` 查看已发现技能及门控状态（可用/不可用原因）
2. `octos skills install weather` 安装；输出安装的技能与工具清单
3. 在 chat 中直接使用自然语言触发（agent 看到工具规格并调用）
4. `octos skills remove weather` 卸载

#### 验收条件（交互级）

##### 正常：安装到调用
- **GIVEN** 技能源可用
- **WHEN** 用户依次运行 `octos skills install weather`、`octos skills list`、在 chat 中问"北京明天天气"
- **THEN** install 输出成功与工具名；list 中 weather 显示"可用"；chat 中 agent 调用天气工具并返回结果

##### 正常：spawn_only 立即返回
- **GIVEN** 已安装含 spawn_only 工具的技能
- **WHEN** agent 调用该工具
- **THEN** 会话立即显示"任务已转入后台"的占位回执（含任务句柄），agent 继续当前对话；任务完成后结果回注会话

##### 异常：缺环境变量
- **GIVEN** 技能 manifest 声明 `requires.env = ["WEATHER_API_KEY"]` 且未设置
- **WHEN** 用户运行 `octos skills list`
- **THEN** 该技能标记为不可用并列出缺失的 WEATHER_API_KEY；agent 调用被拒并提示配置方法

## 三、S10: MCP 服务器接入与工具扩展 — 交互规格

### 3.1 `octos mcp` 与 `octos mcp-serve`

**命令格式**：
- `octos mcp login <SERVER>` / `octos mcp logout <SERVER>` — OAuth MCP 服务器认证
- `octos mcp-serve` — 把 octos 工具面作为 MCP server 对外提供（供外部编排器调用）

**接入方式**（config.json 声明 stdio MCP server）：

```json
{
  "mcp": {
    "servers": {
      "fs-tools": { "command": "npx", "args": ["-y", "@example/mcp-fs"] }
    }
  }
}
```

**交互流程**：
1. 用户在 config 声明 MCP server（命令 + 参数 + 可选 env；env 经 BLOCKED_ENV_VARS 消毒）
2. agent 启动时通过 JSON-RPC stdio 握手，拉取工具清单
3. 每个工具的 input schema 校验（深度 ≤ 10、大小 ≤ 64KB），合法者注册进 ToolRegistry
4. LLM 每轮可见这些工具规格并调用；调用经 stdio 转发给 MCP server 执行
5. 反向：`octos mcp-serve` 让外部编排器把 octos 当 MCP server 调用

#### 验收条件（交互级）

##### 正常：注册并调用
- **GIVEN** config 声明了一个可用的 stdio MCP server
- **WHEN** 用户启动 chat 并发起匹配请求
- **THEN** 启动日志显示该 server 握手成功与注册工具数；agent 调用 MCP 工具并返回结果

##### 异常：schema 超限
- **GIVEN** MCP server 暴露了 schema 深度 12 的工具
- **WHEN** agent 启动注册阶段
- **THEN** 日志显示该工具被拒绝注册及原因（超过深度上限 10）；其余工具正常注册；进程不崩溃

## 四、S12: LLM 故障转移与自适应路由 — 交互规格

### 4.1 配置面

```json
{
  "providers": [
    { "name": "anthropic", "model": "claude-sonnet-5" },
    { "name": "openai", "model": "gpt-5" }
  ],
  "adaptive_routing": { "enabled": true }
}
```

**三层韧性**（实际包装链）：每个基础提供商先包 `RetryProvider`（429/5xx 指数退避）→ 多提供商时由 `ProviderChain`（主备链 + 熔断）或 `AdaptiveRouter`（lane 打分、熔断、探测、hedge racing）统一编排。

### 4.2 交互流程

1. 用户配置主备提供商（可开启 adaptive_routing）
2. 请求首先命中路由层：健康 lane 直接服务
3. 主提供商 429：RetryProvider 按指数退避重试；仍失败则链式切到备提供商
4. 某 lane 连续失败达阈值：熔断，流量切到健康 lane；恢复期半开试探
5. 全部不可用：返回携带各跳失败原因的链路错误

#### 验收条件（交互级）

##### 正常：切换对用户透明
- **GIVEN** 配置了 anthropic（主）与 openai（备）
- **WHEN** anthropic 持续 429，用户在 chat 中继续对话
- **THEN** 回答正常返回（来自 openai）；--verbose 下可见重试与切换记录；用户未被要求任何操作

##### 正常：熔断恢复
- **GIVEN** anthropic lane 已熔断
- **WHEN** 熔断冷却期结束
- **THEN** 路由层以半开方式放行试探请求；成功后 lane 恢复为健康并重新参与打分

##### 异常：全链路失败的信息完整
- **GIVEN** 两个提供商均不可用
- **WHEN** 用户发送消息
- **THEN** 错误输出逐跳列出失败原因（如 anthropic: 429 quota / openai: connection refused），并给出可操作建议；不显示伪造的成功回复

**原型**：`core-06-capability-terminal.md`
