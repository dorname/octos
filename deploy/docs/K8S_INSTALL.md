# octos k8s 安装详细说明

本文档说明如何在 Kubernetes 集群中部署 octos，包括前置条件、镜像构建、配置注入、验证步骤和故障排查。

## 目录

1. [架构概览](#架构概览)
2. [前置条件](#前置条件)
3. [镜像构建](#镜像构建)
4. [三种部署变体](#三种部署变体)
5. [WSL2 + Docker Desktop：binary 提供方式（重要）](#wsl2--docker-desktopbinary-提供方式重要)
6. [配置注入（ConfigMap + Secret）](#配置注入)
7. [部署步骤](#部署步骤)
8. [部署后 Smoke（SMOKE-S16）](#部署后-smokesmoke-s16)
9. [验证](#验证)
10. [故障排查](#故障排查)
11. [升级与回滚](#升级与回滚)
12. [停止与再部署](#停止与再部署)

---

## 架构概览

```
┌──────────────────────────────────────────────────────────────┐
│ Kubernetes Namespace: octos                                  │
│                                                               │
│  ┌────────────────────────────┐ ┌──────────────────────┐ │
│  │ Deployment: pg              │ │ ConfigMap: octos-config│ │
│  │ image: postgres:16-alpine  │ │ - DATABASE_URL        │ │
│  │                            │ │ - OCTOS_AUTH_TOKEN    │ │
│  └────────────────────────────┘ │ - WORKSPACE_DIR       │ │
│             │                  │ - LLM_* 配置           │ │
│             ▼                  └──────────────────────┘ │
│  ┌────────────────────────────┐              │ │
│  │ Service: pg                │              ▼ │
│  │ port: 5432                 │ ┌──────────────────────┐ │
│  └────────────────────────────┘ │ Secret: llm-credentials│ │
│                                │ - ANTHROPIC_API_KEY  │ │
│                                └──────────────────────┘ │
│                                            │ │
│  ┌────────────────────────────────────────▼──────────────┐ │
│  │ Deployment: octos (replicas: 2)                        │ │
│  │                                                        │ │
│  │  ┌──────────────────┐  ┌──────────────────┐         │ │
│  │  │ Pod 1            │  │ Pod 2            │         │ │
│  │  │ octos serve     │  │ octos serve     │         │ │
│  │  │ + CronServicePg │  │ + CronServicePg │         │ │
│  │  └──────────────────┘  └──────────────────┘         │ │
│  └────────────────────────────────────────────────────────┘ │
│                                │ │
│  ┌────────────────────────────▼──────────────┐ │
│  │ Service: octos (ClusterIP, port: 8080)   │ │
│  └───────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────┘
```

**关键点**：
- **PG** 存储：approvals（K05）、cron schedules（K10/K18）、session events（K06）、run leases（K02/K03/K17）
- **LocalStore**（redb 文件）：profiles、users、tenants、ui-protocol ledger
- **CronServicePg**（双副本）：K10 单 claim 保证同一 firing 只被一个 Pod 处理

---

## 前置条件

### 1. 集群要求

| 组件 | 最低版本 | 推荐版本 |
|---|---|---|
| Kubernetes | 1.25 | 1.28+ |
| kubectl | 1.25 | 1.28+ |
| 节点架构 | amd64 / arm64 | amd64 |
| 内存 | 每 Pod ≥ 512MB | ≥ 1GB |
| CPU | 每 Pod ≥ 200m | ≥ 500m |
| 存储 | ≥ 5GB（PG 数据）| ≥ 20GB |

### 2. 镜像

**Base 镜像**（自动从 Docker Hub 拉取）：
- `rust:1.88-alpine` — builder stage
- `alpine:3.21` — runtime stage
- `postgres:16-alpine` — PG 数据库

**应用镜像**（需要本地 build）：
- `octos:k8s-stateless` — 从 `Dockerfile` build

### 3. 网络端口

| 端口 | 用途 |
|---|---|
| 8080 | octos HTTP API（内部 Service） |
| 5432 | PostgreSQL（内部 Service） |
| 443/80 | 外部访问（如果需要 Ingress） |

### 4. kubectl 配置

```bash
kubectl cluster-info
kubectl get nodes
```

应该看到 `control-plane` 节点 Ready。

---

## 镜像构建

### 方式 1：Docker build（推荐）

```bash
cd /home/kyle/octos
docker build -t octos:k8s-stateless .
```

构建过程（~10-15 分钟）：
1. `rust:1.88-alpine` 编译 octos binary（musl）
2. `alpine:3.21` 安装运行时依赖（ca-certificates、tzdata、chromium 等）
3. 复制 binary 到 runtime image

### 方式 2：预构建 binary + 手动打包

如果 docker build 不可用（如 CI 环境），可以：

```bash
# 1. 用 musl 编译
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli --no-default-features --features "api,postgres"

# 2. 用 docker import 创建镜像
echo "FROM alpine:3.21
RUN apk add --no-cache ca-certificates tzdata
COPY target/x86_64-unknown-linux-musl/release/octos /usr/local/bin/octos
ENTRYPOINT [\"octos\"]" | docker build -t octos:k8s-stateless -
```

### 方式 3：使用 ConfigMap（仅限小 binary）

如果 binary < 1MB，可以用 ConfigMap：

```bash
# 但当前 binary ~120MB，不适用
ls -lh target/x86_64-unknown-linux-musl/release/octos
```

---

## 三种部署变体

### 变体 A：`baseline` — 单节点最小部署（`01-baseline.yaml`）

**适用场景**：第一次部署、demo、POC

**特点**：
- 单 namespace、单 PG、单 octos 部署
- **无 ConfigMap**——用 env 直接注入
- **无 Secret**——token 硬编码（仅 demo 用）

**部署**：

```bash
./deploy/scripts/deploy-k8s.sh baseline
```

**验证**：

```bash
kubectl get pods -n octos
kubectl port-forward -n octos svc/octos 8080:8080
curl http://127.0.0.1:8080/health
```

### 变体 B：`hostpath` — 开发模式（`02-hostpath-dev.yaml`）

**适用场景**：本地开发、快速迭代 binary

**特点**：
- **hostPath** 挂载本地 binary 到 pod（dev only）
- 修改 `target/x86_64-unknown-linux-musl/release/octos` → 重启 pod 即生效

**部署**：

```bash
# 1. 编译
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli --no-default-features --features "api,postgres"

# 2. 准备 binary 目录
mkdir -p /tmp/octos-k8s
cp target/x86_64-unknown-linux-musl/release/octos /tmp/octos-k8s/

# 3. 部署
./deploy/scripts/deploy-k8s.sh hostpath
```

**优势**：改 binary 后只需 `kubectl rollout restart deployment/octos -n octos`

**警告**：hostPath 只在单节点集群（docker-desktop、minikube）有效。生产用 ConfigMap 或私有 registry。

### 变体 C：`cluster` — 生产级（`03-cluster-with-config.yaml`）

**适用场景**：生产部署、多副本、CI/CD

**特点**：
- ✅ **ConfigMap** 注入非敏感配置
- ✅ **Secret** 注入 LLM API key
- ✅ **init container** 跑 PG migrations + 写 config.json
- ✅ **2 副本** octos 部署（K10 cron 集群）

**部署**：

```bash
# 1. 创建 Secret（敏感）
kubectl create secret generic llm-credentials \
  --from-literal=ANTHROPIC_API_KEY=sk-ant-xxxxx \
  -n octos

# 2. 部署
./deploy/scripts/deploy-k8s.sh cluster
```

**优势**：
- 配置外部化（ConfigMap 可版本控制）
- Secret 加密存储（K8s etcd 加密）
- 多副本 + K10 cron 集群
- PG migrations 启动时跑

---

## WSL2 + Docker Desktop：binary 提供方式（重要）

在 WSL2 + Docker Desktop for Windows 环境下，**无法用 bind-mount（hostPath）向
cluster init 提供 octos binary**。这是三类部署变体中最常见的第一个坑。

### 为什么 bind-mount 不可用（路径语义三层不一致）

hostPath 的路径是**以 Docker Desktop 的 LinuxKit VM 为视角**解析的，而
WSL2 场景下至少存在三套互不一致的文件系统视图：

| 你写入的位置 | 你以为 VM 能看到 | 实际结果 |
|---|---|---|
| WSL 的 `/tmp/octos-k8s/octos` | 同名路径 | ❌ VM 的 `/tmp` 是自己的 tmpfs，看不到 WSL 的 `/tmp`（tmpfs 各自独立） |
| WSL 的 `/home/.../octos` | VM 同名路径 | ❌ 默认未配置 WSL→VM 的文件共享；VM 根文件系统不含 WSL 发行版目录 |
| Windows 的 `C:\tmp\octos-k8s\octos` | `/tmp/octos-k8s/octos` | ❌ VM 内的挂载点与 Windows 盘符映射随 Docker Desktop 版本变化，**没有稳定可写的固定路径**，`type: File` 检查常报 `not a file` |

实测报错形态（`kubectl describe pod`）：

```
MountVolume.SetUp failed for volume "octos-binary":
  hostPath type check failed: /tmp/octos-k8s/octos is not a file
```

因此 cluster 变体（`03-cluster-with-config.yaml`）自 #2436 起**不再使用
hostPath**，改由 init container 通过 HTTP 从宿主拉取 binary（见下）。

### 正确做法：宿主 HTTP 服务 + init wget

init 脚本在 alpine 容器里执行：

```sh
wget -q -O /opt/octos/octos http://host.docker.internal:8088/octos
chmod +x /opt/octos/octos
```

宿主侧在**含 musl binary 的目录**起一个 HTTP 服务，**必须 `--bind 0.0.0.0`**：

```bash
# WSL 内（推荐起点）
mkdir -p /tmp/octos-k8s-bin
cp target/x86_64-unknown-linux-musl/release/octos /tmp/octos-k8s-bin/octos
cd /tmp/octos-k8s-bin && python3 -m http.server 8088 --bind 0.0.0.0
```

为什么必须 `0.0.0.0`：pod 内 `host.docker.internal` 解析到的是
**Docker 网桥视角的宿主地址**（如 `192.168.65.254` / WSL eth1 地址），
不是 `127.0.0.1`；服务只绑 loopback 时 init 会报
`wget: can't connect to remote host ... Connection refused`。

### Windows 侧端口实况（:18088 案例）

在部分 WSL2 + Docker Desktop 组合里，**WSL 内 listen 的 8088 进不了集群**
（`host.docker.internal` 只能到达 Windows 宿主侧）。此时把 HTTP 服务放到
**Windows 侧**执行（PowerShell，在含 `octos` 文件的目录）：

```powershell
python -m http.server 8088 --bind 0.0.0.0
```

若 8088 在 Windows 侧已被占用（或防火墙策略限制），可改用 18088 并
**同步修改** ConfigMap `octos-init-script` 中 init 脚本的 wget URL：

```bash
kubectl -n octos edit configmap octos-init-script   # 8088 → 18088
kubectl -n octos rollout restart deploy/octos
```

判定标准：init container 日志出现
`Binary installed: octos <version>` 即 binary 提供链路打通；若报
`ERROR: failed to fetch binary`，按本节顺序排查（绑定地址 → 服务侧别 →
端口一致性）。

更多实况细节（含当时六个真实故障的完整复现）见
[K8S_DEPLOY_PROVEN.md](./K8S_DEPLOY_PROVEN.md)。

---

## 配置注入

### ConfigMap 配置项

`octos-config` ConfigMap 包含：

| 字段 | 必填 | 说明 |
|---|---|---|
| `DATABASE_URL` | ✅ | PG 连接字符串 |
| `OCTOS_AUTH_TOKEN` | ✅ | API 认证 token |
| `DEFAULT_PROFILE` | ⚠️ | 默认 profile 名（默认 `cluster-worker`） |
| `WORKSPACE_DIR` | ⚠️ | pod 内 workspace 路径 |
| `LLM_PROVIDER` | ⚠️ | `anthropic` / `openai` / `moonshot` |
| `LLM_MODEL` | ⚠️ | 模型名（如 `k3`、`gpt-4`） |
| `LLM_BASE_URL` | ⚠️ | LLM API base URL |
| `LLM_API_KEY_ENV` | ⚠️ | 哪个 env var 存 API key（默认 `ANTHROPIC_API_KEY`） |
| `RUST_LOG` | ❌ | 日志级别（默认 `info`） |

### Secret 配置项

`llm-credentials` Secret 包含：

| 字段 | 必填 | 说明 |
|---|---|---|
| `ANTHROPIC_API_KEY` | ⚠️ | Anthropic API key（生产） |
| `OPENAI_API_KEY` | ⚠️ | OpenAI API key（如果用 OpenAI） |
| `MOONSHOT_API_KEY` | ⚠️ | Moonshot API key（如果用 Moonshot） |

**注意**：Secret 里存的是 API key 的**值**，而 ConfigMap 里 `LLM_API_KEY_ENV` 告诉 octos 从哪个 env var 读。两者必须匹配。

### init 脚本流程

`init-config.sh` 在 pod 启动时执行：

```
1. 创建 /tmp/octos-data（instance data dir）
2. 合并 env vars → /tmp/octos-data/config.json
3. 创建默认 profile（如果不存在）：
   /tmp/octos-data/profiles/<DEFAULT_PROFILE>/config.json
4. 创建 workspace 目录
5. 如果设置了 DATABASE_URL：
   - 不主动跑迁移——octos 没有 `migrate` 子命令，PG 建表是 **lazy migrate**
     （serve 在首次 DB 操作时执行迁移，须 binary 带 `--features postgres`）
6. 从宿主 HTTP 拉 binary（WSL 场景见上文专节），然后 exec octos serve "$@"
```

---

## 部署步骤

### 完整流程（变体 C）

```bash
# 1. 编译 musl binary
cd /home/kyle/octos
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli --no-default-features --features "api,postgres"

# 2. Build docker image
docker build -t octos:k8s-stateless .

# 3. 创建 namespace + ConfigMap + Secret + Deployment
kubectl apply -f deploy/k8s/03-cluster-with-config.yaml

# 或用脚本：
./deploy/scripts/deploy-k8s.sh cluster

# 4. 注入 LLM API key
kubectl create secret generic llm-credentials \
  --from-literal=ANTHROPIC_API_KEY=<your-key> \
  -n octos \
  --dry-run=client -o yaml | kubectl apply -f -

# 5. 等待 pods ready
kubectl wait --for=condition=ready pod -l app=octos -n octos --timeout=120s
kubectl wait --for=condition=ready pod -l app=pg -n octos --timeout=90s

# 6. 验证
kubectl get pods -n octos
kubectl get svc -n octos

# 7. Port-forward 测试
kubectl port-forward -n octos svc/octos 8080:8080 &
curl http://127.0.0.1:8080/health
```

---

## 部署后 Smoke（SMOKE-S16）

部署不是以 `kubectl get pods` 全 Ready 为终点——**必须跑一次 smoke**。本仓
入口脚本：`scripts/smoke-s16-k8s.sh`（OpenLogos SMOKE-S16-* runner，针对
docker-desktop 本地 `octos` namespace）。

```bash
./scripts/smoke-s16-k8s.sh
# 结果写入 logos/resources/verify/smoke-results.jsonl（每次全量覆写）
```

判定标准（4 项全 PASS 才算部署闭环）：

| ID | 场景 | 判定 |
|---|---|---|
| SMOKE-S16-01 | deploy/octos Available + port-forward 后 `GET /health` 返回 `"status":"healthy"` | 存活面 |
| SMOKE-S16-02 | `GET /api/version` 返回 `"service":"octos"` | API 面正确标识 |
| SMOKE-S16-03 | PG `pg_tables` 中 `sessions,session_events,approvals,run_leases,schedules` 五表齐备 | **迁移面（lazy migrate 已触发）** |
| SMOKE-S16-04 | 删 pod → rollout 恢复 → `/health` 再度 healthy | 自愈面（PVC 数据不丢） |

其中 S16-03 直接回答"cluster Ready 后 PG 是否真的有表"——lazy migrate 在
首次 DB 操作时才建表，光看 pod Ready 无法证明；必须以查表为准。

环境变量：`OCTOS_SMOKE_NS`（默认 `octos`）、`OCTOS_SMOKE_K8S_SERVER`
（显式 API 地址，如 `https://127.0.0.1:6443`，WSL docker-desktop 场景自动
探测）。端口转发固定占 `127.0.0.1:50080`。

> 关于"PG 空表"的历史根因：早期 musl binary 未带 `--features postgres`
> （issue 系 #2436），attach 逻辑整块缺失，建表永不发生；该根因已由
> fcacde31 修复（构建命令统一要求 `--features api,postgres`），断言由
> 0b7ad0c9 更新。今天若 S16-03 仍 fail，先核对 binary 构建参数，再查
> DATABASE_URL 连通性，最后才是 lazy migrate 触发路径。

## 验证

### 健康检查

```bash
# HTTP 健康
curl http://127.0.0.1:8080/health
# 预期：{"status":"healthy","service":"octos","version":"..."}

# Pod 状态
kubectl get pods -n octos
# 预期：2 个 octos pod + 1 个 pg pod 都 Running

# PG 连接验证
kubectl exec -n octos <pg-pod> -- psql -U postgres -d octos -c '\dt'
# 预期：列出所有表（schedules、approvals、session_events 等）
```

### 日志查看

```bash
# octos serve 日志
kubectl logs -n octos -l app=octos --tail=100

# PG 日志
kubectl logs -n octos -l app=pg --tail=100

# 实时跟踪
kubectl logs -n octos -l app=octos -f
```

### K10 单 claim 验证

```bash
# 在两个 pod 上分别添加同名 schedule
kubectl exec -n octos <pod1> -- sh -c "..."
kubectl exec -n octos <pod2> -- sh -c "..."

# 应该只有一个 Pod 成功 claim
```

---

## 故障排查

### 1. Pod 一直 ContainerCreating

```bash
kubectl describe pod <pod-name> -n octos
```

常见原因：
- **镜像拉取失败**：检查 `docker images` 和 imagePullPolicy
- **PVC 绑定失败**：检查 storage class
- **资源不足**：检查 `kubectl describe nodes`

### 2. octos pod CrashLoopBackOff

```bash
kubectl logs -n octos <pod-name> --previous
```

常见原因：
- **octos-data 目录权限**：检查 `securityContext.fsGroup`
- **DATABASE_URL 错误**：测试 PG 连接 `psql $DATABASE_URL`
- **API key 缺失**：检查 Secret 是否注入

### 3. agent 无法调 LLM

排查步骤：

```bash
# 1. 检查 API key 是否注入
kubectl exec -n octos <pod> -- env | grep -iE "anthropic|openai"

# 2. 检查 profile 配置
kubectl exec -n octos <pod> -- cat /tmp/octos-data/profiles/cluster-worker/config.json

# 3. 查看 RUST_LOG
kubectl logs -n octos <pod> | grep -i "llm\|api_key\|provider"
```

### 4. PG 表没创建

**根因**：PG 建表是 lazy migrate——migrations 在首次 DB 操作时才执行，pod Ready 不代表
已建表；历史上更常见的主因是 musl binary 未带 `--features postgres`
（#2436，已由 fcacde31 修复构建要求）。

**解决**：不要找 `octos migrate` 子命令（不存在）。按序核对：
1) 构建命令含 `--features api,postgres`；2) `DATABASE_URL` 可达；
3) 跑 `./scripts/smoke-s16-k8s.sh`，以 SMOKE-S16-03 查表结果为准
   （见上文"部署后 Smoke"节）。

### 5. hostPath 在多节点集群不工作

**根因**：hostPath 只在单节点集群有效。

**解决**：
- 单节点：继续用 hostPath（dev only）
- 多节点：用 PVC（PersistentVolumeClaim）或私有 registry 推 image

---

## 升级与回滚

### 升级 octos

```bash
# 1. 重新 build image
docker build -t octos:k8s-stateless .

# 2. 重启 deployment（rolling update）
kubectl rollout restart deployment/octos -n octos

# 3. 查看进度
kubectl rollout status deployment/octos -n octos
```

### 回滚

```bash
# 1. 查看历史
kubectl rollout history deployment/octos -n octos

# 2. 回滚到上一个版本
kubectl rollout undo deployment/octos -n octos
```

### 清理

见下一节 [停止与再部署](#停止与再部署)（含「只停 Pod」与「删 namespace」分级操作）。

---

## 停止与再部署

本地 docker-desktop / ns `octos` 的实操清单（含 port-forward、binary HTTP、
AppUI origin）以 **[K8S_DEPLOY_PROVEN.md](./K8S_DEPLOY_PROVEN.md#停止与再部署)** 为准。

摘要：

| 力度 | 命令要点 | 保留什么 |
|------|----------|----------|
| 软停 | `kubectl -n octos scale deploy/{octos,pg} --replicas=0` | PVC / CM / Secret |
| 卸清单 | `kubectl delete -f deploy/k8s/03-cluster-with-config.yaml` | 视 yaml；常仍留 PVC |
| 全清 | `kubectl delete namespace octos` | 无 |

再部署：`cargo build … --features api,postgres` → 本机 HTTP 提供 musl binary →
`./deploy/scripts/deploy-k8s.sh cluster` → `port-forward` → 核对
`OCTOS_APPUI_ALLOWED_ORIGINS` 与浏览器 origin（PF 端口需显式加入）。

---

## 参考

- [Octos UI Protocol Spec](../specs/task-ui-protocol-v1alpha1.spec.md)
- [K8s 无状态化 plan](../analysis/octos-k8s-plugin-factory-plan-2026-09-14.md)
- [Goal 01 最终报告](../analysis/octos-k8s-goal01-final-report.md)
