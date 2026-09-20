# core-07 K8s 集群无状态化 — 功能规格

> 覆盖场景：S16（K8s 多副本无状态化部署与故障恢复）
> 需求来源：core-01-requirements.md §四 S16
> 决策来源：docs/adr/cluster-state-and-execution.md
> 配套部署：prd/3-technical-plan/3-deployment/core-01-deployment-plan.md
> 配套时序：prd/3-technical-plan/2-scenario-implementation/core-S16-k8s-cluster.md

## 一、S16: 集群模式交互规格

### 1.1 部署形态选择

| 形态 | 命令入口 | 适用 | 状态持久化 |
|------|---------|------|-----------|
| baseline | `./deploy/scripts/deploy-k8s.sh baseline` | 冒烟/最小镜像验证 | emptyDir / 进程内（不保证跨 Pod） |
| hostpath | `./deploy/scripts/deploy-k8s.sh hostpath` | 单节点开发 | HostPath 挂载工作区 |
| cluster | `./deploy/scripts/deploy-k8s.sh cluster` | 多副本目标形态 | PostgreSQL + PVC（按清单） |

**交互流程**：
1. 运维准备镜像（musl/release）与 `config.json` / 密钥
2. 选择形态并执行部署脚本；cluster 形态注入 ConfigMap + Secret，等待 PG Ready 后跑迁移
3. 通过 Service 访问 Dashboard / UI Protocol
4. 滚动或杀 Pod 后，客户端重连；服务端按 Scope 从 PG 回放

### 1.2 Scope 与执行身份

```text
Scope = tenant_id + profile_id + workspace_id + session_id
Execution = Scope + thread_id + run_id + attempt_id
```

- wire session ID 进入服务端后绑定完整 Scope，沿查询、缓存、广播、审批、工具与审计携带
- 跨 Pod 可见性以 PG 为准，进程内缓存仅为加速层

### 1.3 故障恢复语义

| 能力 | 行为 |
|------|------|
| 租约 | `run_leases` 在 DB 事务内占有/递增 epoch；旧 epoch 不可写 |
| 检查点 | `run_checkpoints` + `tool_invocations` 支持重建执行 |
| 事件 | `session_events` 单调 seq + outbox；WS 断线按 seq 回放 |
| Cron | 集群模式使用 PG 后端 `CronServicePg`，禁止仅内存调度作为真相 |

#### 验收条件（交互级）

##### 正常：三种形态可切换文档路径
- **GIVEN** 仓库含 `deploy/k8s/01-baseline.yaml` / `02-hostpath-dev.yaml` / `03-cluster-with-config.yaml`
- **WHEN** 用户按 `deploy/README.md` 选择形态部署
- **THEN** 脚本退出码 0 或给出明确失败原因；cluster 形态下 PG 迁移完成

##### 正常：审批跨 Pod 可见
- **GIVEN** 集群模式双副本，Pod A 产生待审批
- **WHEN** Pod A 被删除，用户经 Service 在 Pod B 上批准/拒绝
- **THEN** 决策落 PG 且对后续请求可见，不要求粘滞到原 Pod

##### 异常：用 baseline 冒充生产多副本
- **GIVEN** 用户以 baseline 形态拉起多副本且无 PG
- **WHEN** 跨 Pod 查询同一会话真相
- **THEN** 文档与 doctor/部署说明明确警告该形态不提供跨 Pod 持久化保证；不得宣传为生产就绪
