# 部署变体对比

本文档对比 octos 的三种 k8s 部署变体、适用场景、迁移路径。

## 总览

| 变体 | 文件 | 适用场景 | 复杂度 |
|---|---|---|---|
| **baseline** | `01-baseline.yaml` | 第一次部署、demo、POC | ⭐ |
| **hostpath** | `02-hostpath-dev.yaml` | 本地开发、快速迭代 binary | ⭐⭐ |
| **cluster** | `03-cluster-with-config.yaml` | 生产、多副本、CI/CD | ⭐⭐⭐ |

## 详细对比

### baseline（单节点最小部署）

**结构**：
- 单 namespace `octos`
- 1 个 PG pod + Service
- 1 个 octos Deployment（默认 2 副本，可改 1）
- 无 ConfigMap、无 Secret
- token 硬编码在 manifest

**Pros**：
- 部署最快（一条 kubectl apply）
- 依赖最少（只需要 PG image）
- 适合 demo 和 POC

**Cons**：
- API token 明文在 manifest（不生产）
- LLM key 没注入（agent 无法调 LLM）
- PG 数据无持久化（emptyDir，重启丢）

**适用**：
- 第一次跑通流程
- 给团队演示
- 本地 smoke test

**部署命令**：
```bash
./deploy/scripts/deploy-k8s.sh baseline
```

### hostpath（开发模式）

**结构**：
- 同 baseline
- 多一个 hostPath volume：`/tmp/octos-k8s/octos` → `/opt/octos`
- binary 在 WSL2 宿主，pod 启动时挂载

**Pros**：
- 改 binary 不用 rebuild image
- `kubectl rollout restart` 即可加载新 binary
- 适合快速迭代

**Cons**：
- **只支持单节点集群**（docker-desktop / minikube）
- WSL2 宿主路径与 docker-desktop VM 路径不互通（WSL2 特有坑）
- 生产绝对不能用

**适用**：
- 本地开发
- 调试 binary 问题
- 频繁改 binary 的迭代

**部署命令**：
```bash
# 1. 编译 musl binary
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli --no-default-features --features "api,postgres"

# 2. 放到 hostPath 目录
mkdir -p /tmp/octos-k8s
cp target/x86_64-unknown-linux-musl/release/octos /tmp/octos-k8s/

# 3. 部署
./deploy/scripts/deploy-k8s.sh hostpath
```

### cluster（生产级）

**结构**：
- 同 baseline
- ConfigMap `octos-config`：非敏感配置（DATABASE_URL、WORKSPACE_DIR、LLM provider 等）
- Secret `llm-credentials`：敏感凭据（ANTHROPIC_API_KEY）
- ConfigMap `octos-init-script`：init-config.sh 启动脚本
- Deployment 用 `envFrom: [configMapRef, secretRef]` 自动注入
- init container 跑 PG migrations + 写 config.json

**Pros**：
- 配置外部化（ConfigMap 可 git 版本控制）
- 敏感凭据加密（Secret）
- 多副本 + K10 cron 集群
- 启动时主动跑 migrations（避免惰性 migrate 漏跑）

**Cons**：
- 部署复杂（多个 ConfigMap/Secret/Deployment）
- 需要手动创建 Secret（kubectl create secret）
- init 脚本需要调试

**适用**：
- 生产部署
- CI/CD 流水线
- 多副本 cluster

**部署命令**：
```bash
# 1. 编译 + build image
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli --no-default-features --features "api,postgres"
docker build -t octos:k8s-stateless .

# 2. 创建 Secret（敏感）
kubectl create secret generic llm-credentials \
  --from-literal=ANTHROPIC_API_KEY=<your-key> \
  -n octos

# 3. 部署
./deploy/scripts/deploy-k8s.sh cluster
```

## 迁移路径

### 从 baseline 到 hostpath（开发阶段）

```bash
# 1. 编译 musl binary
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli --no-default-features --features "api,postgres"

# 2. 准备 hostPath 目录
mkdir -p /tmp/octos-k8s
cp target/x86_64-unknown-linux-musl/release/octos /tmp/octos-k8s/

# 3. 删除 baseline 部署
kubectl delete -f deploy/k8s/01-baseline.yaml

# 4. 部署 hostpath
kubectl apply -f deploy/k8s/02-hostpath-dev.yaml

# 5. 改 binary 后重新加载
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli --no-default-features --features "api,postgres"
cp target/x86_64-unknown-linux-musl/release/octos /tmp/octos-k8s/
kubectl rollout restart deployment/octos -n octos
```

### 从 hostpath 到 cluster（生产化）

```bash
# 1. Build 镜像（hostpath 不需要 image，cluster 需要）
docker build -t octos:k8s-stateless .

# 2. 删除 hostpath 部署
kubectl delete -f deploy/k8s/02-hostpath-dev.yaml

# 3. 创建 Secret
kubectl create secret generic llm-credentials \
  --from-literal=ANTHROPIC_API_KEY=<your-key> \
  -n octos

# 4. 部署 cluster
kubectl apply -f deploy/k8s/03-cluster-with-config.yaml

# 5. 数据迁移（如需要）
# 如果 hostpath 期间产生了 session 数据，先导出
kubectl exec -n octos <old-octos-pod> -- tar czf - /tmp/octos-data > backup.tar.gz

# 导入到 cluster 部署
# 注意：PG 数据已在 PG 里，只需导入 redb 文件（如果有）
```

## 选择建议

| 你的需求 | 推荐变体 |
|---|---|
| 第一次跑通流程 | baseline |
| 本地开发、调试 binary | hostpath |
| CI/CD 测试 | cluster（小型） |
| 生产多副本 | cluster（完整） |
| 给客户演示 | baseline → cluster |

## 配置差异汇总

| 配置项 | baseline | hostpath | cluster |
|---|---|---|---|
| DATABASE_URL | env | env | ConfigMap |
| OCTOS_AUTH_TOKEN | manifest 硬编码 | manifest 硬编码 | ConfigMap |
| ANTHROPIC_API_KEY | 无 | 无 | Secret |
| WORKSPACE_DIR | 默认 | 默认 | ConfigMap |
| LLM_* 配置 | 默认 | 默认 | ConfigMap |
| PG migrations | 惰性 | 惰性 | 启动时主动跑 |
| 多副本 | 否（默认 2） | 否（默认 2） | ✅ 是 |
