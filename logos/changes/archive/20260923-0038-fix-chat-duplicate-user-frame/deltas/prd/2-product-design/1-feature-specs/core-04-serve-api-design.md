# Delta: prd/2-product-design/1-feature-specs — core-04-serve-api-design.md

> target: logos/resources/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md

## ADDED — 1附.4 user_message 投影 client_message_id 贯通契约

### 1附.4 user_message 投影 client_message_id 贯通契约

AppUI chat 的同一 user 消息在投影链路存在**两种来源**：turn 提交时的 live `user_message` envelope（commit observer 即射，thread_id 为当轮 turn 派生 id）与 `session/hydrate` 快照中的 durable message 行（thread_id 可能被重盖章为服务端规范线程 id）。为使前端 projection 对同一消息只渲染一个 frame，`client_message_id` 必须全链路贯通：

1. **盖章规则**：WS `turn/start` 处理流程构建/持久化 user `Message` 时，以 `client_message_id = Some(turn_id)` 盖章（AppUI 客户端将 `turn_id` 与 clientMessageId 钉为同一 UUID，二者天然同值）；其他客户端未提供时保持 `None`，行为与现状一致。
2. **live envelope**：`user_message` v2 envelope 携带 `client_message_id`（commit observer 转发 durable 行的同名字段，既有逻辑，无需新协议）。
3. **hydrate 快照**：`session/hydrate` 的 `messages[]` 序列化携带 `client_message_id`（字段在契约中原为 optional，纯增量填充）。
4. **前端去重语义**：projection-store 对 `user_message` 按 `client_message_id` 跨线程去重——同一会话已收录相同 cmid 的 user frame（任意 thread_id）时，后到的一律判 duplicate；该去重对「live 先到、snapshot 后到」与「snapshot 先到、live 后到」两种时序对称成立。

历史数据兼容：持久化行 `client_message_id` 为空时前端保持现状渲染（不去重），仅新消息受益于去重。

#### 验收条件（交互级）

##### 正常：发送一条消息主区只有一个 user frame
- **GIVEN** AppUI chat 已连接 k8s 后端
- **WHEN** 用户发送一条消息并等到 assistant 回复完成后再观察 90s
- **THEN** 主区该消息的 user frame 数恒为 1（不出现 ISO 时间戳的重复 frame）；刷新页面重新打开会话后仍为 1

##### 正常：snapshot 先到时 live echo 被去重
- **GIVEN** 某 user 消息的 durable 行已通过 hydrate 快照收录（携带 cmid）
- **WHEN** 该消息的 live `user_message` envelope 随后到达（同 cmid、不同 thread_id）
- **THEN** projection-store 判为 duplicate，不渲染第二个 user frame

##### 异常：cmid 缺失时保持现状
- **GIVEN** 持久化行 `client_message_id` 为空（历史数据 / 非 AppUI 写入）
- **WHEN** hydrate 快照与 live envelope 同时覆盖该消息
- **THEN** 前端按现状渲染（可能双 frame），不崩溃、不错误吞并其他消息
