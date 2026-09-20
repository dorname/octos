# octos k8s 安装详细说明

本文档说明如何在 Kubernetes 集群中部署 octos，包括前置条件、镜像构建、配置注入、验证步骤和故障排查。

## 目录

1. [架构概览](#架构概览)
2. [前置条件](#前置条件)
3. [镜像构建](#镜像构建)
4. [三种部署变体](#三种部署变体)
5. [配置注入（ConfigMap + Secret）](#配置注入)
6. [部署步骤](#部署步骤)
7. [验证](#验证)
8. [故障排查](#故障排查)
9. [升级与回滚](#升级与回滚)

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
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli

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
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli

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
   - 主动跑 `octos migrate`（PG migrations）
6. exec octos serve "$@"
```

---

## 部署步骤

### 完整流程（变体 C）

```bash
# 1. 编译 musl binary
cd /home/kyle/octos
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli

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

**根因**：octos serve 的 migrations 是惰性的（第一次 DB 操作时触发）。

**解决**：在 init 脚本中显式调用 `octos migrate`：

```yaml
# init container
initContainers:
- name: migrate
  image: octos:k8s-stateless
  command: ["octos", "--instance-data-dir", "/tmp/octos-data", "migrate"]
```

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

```bash
# 卸载所有 k8s 资源
kubectl delete -f deploy/k8s/03-cluster-with-config.yaml
kubectl delete -f deploy/k8s/01-baseline.yaml

# 删除 namespace
kubectl delete namespace octos
```

---

## 参考

- [Octos UI Protocol Spec](../specs/task-ui-protocol-v1alpha1.spec.md)
- [K8s 无状态化 plan](../analysis/octos-k8s-plugin-factory-plan-2026-09-14.md)
- [Goal 01 最终报告](../analysis/octos-k8s-goal01-final-report.md)
