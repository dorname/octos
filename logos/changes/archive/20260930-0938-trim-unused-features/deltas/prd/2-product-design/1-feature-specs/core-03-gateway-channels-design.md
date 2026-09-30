## MODIFIED — 1.1 `octos gateway` 与 `octos channels`

**命令格式**：
- `octos gateway [--profile <NAME>]` — 以常驻进程运行多通道网关
- `octos channels <list|add|remove|test> ...` — 通道配置管理

**通道配置**（config.json，以 telegram 为例）：

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token_env": "TELEGRAM_BOT_TOKEN",
      "require_mention": true
    }
  }
}
```

**支持的 11 个通道**：api、cli、discord、feishu、matrix、matrix-user、telegram、twilio、wecom、wecom-bot、whatsapp。（2026-09-29 trim-unused-features：dingtalk / slack / line / email / qq-bot / wechat 已移除；matrix / matrix-user 因 `api` feature 编译耦合保留。）

**交互流程**：
1. 用户在 config.json 配置通道凭据（token 走环境变量，不落盘明文）并启用通道
2. 运行 `octos gateway`；CLI 加载配置 → 构建 LLM/工具/记忆栈 → 启动各通道适配器（轮询或 webhook）→ 打印已启动通道清单
3. 入站消息到达：通道适配 → 解析/创建会话（session key 含通道与 chat 标识）→ 入队给会话 actor → agent 处理（LLM + 工具）
4. 回复出站：按通道格式渲染 → coalescing 分片（段落 > 换行 > 句子 > 空格 > 硬切，逐通道长度上限，≤ 50 片，UTF-8 安全边界）→ 逐片发送
5. 会话命令：用户在 IM 中发 `/new` 开新会话（fork，保留 parent 链）、`/s` 或 `/sessions` 查看、`/back` 返回上一会话
6. 停止：SIGINT/SIGTERM 触发优雅关闭（AtomicBool 停机信号），在处理的轮次完成后退出
