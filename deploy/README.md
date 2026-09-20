# octos 部署包

本目录包含 octos 的所有部署和安装相关文件，按部署方式分类组织。

## 目录结构

```
deploy/
├── README.md                    # 本文件（总览）
├── k8s/                          # Kubernetes 部署 manifests
│   ├── 01-baseline.yaml          # 单节点最小部署（共享 docker daemon）
│   ├── 02-hostpath-dev.yaml       # 开发模式（hostPath 挂载本地 binary）
│   └── 03-cluster-with-config.yaml # 生产级（ConfigMap + Secret）
├── docker/                       # Docker Compose 部署
│   └── docker-compose.yml        # 本地开发用 docker compose
├── scripts/                      # 部署/卸载/测试脚本
│   └── deploy-k8s.sh             # k8s 部署脚本（支持 baseline/hostpath/cluster）
└── docs/                         # 详细文档
    ├── K8S_INSTALL.md            # k8s 安装详细说明
    └── DEPLOYMENT_VARIANTS.md    # 部署变体对比
```

## 快速开始

### 选项 1：本地 docker compose（最简单）

```bash
cd deploy/docker
docker compose up -d
# 等待启动后验证
curl http://localhost:8080/health
```

### 选项 2：本地 k8s（推荐）

```bash
# 部署 baseline 版本（最简单）
./deploy/scripts/deploy-k8s.sh baseline

# 部署 hostpath 版本（开发用，需要本地 binary）
# cluster / hostpath 必须带 postgres，否则 DATABASE_URL 会被忽略（#2436）
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli \
  --no-default-features --features "api,postgres"
mkdir -p /tmp/octos-k8s-bin && cp target/x86_64-unknown-linux-musl/release/octos /tmp/octos-k8s-bin/
./deploy/scripts/deploy-k8s.sh hostpath

# 部署 cluster 版本（完整 ConfigMap + Secret）
./deploy/scripts/deploy-k8s.sh cluster
```

### 选项 3：直接二进制运行（stdio 模式）

```bash
cargo build --release -p octos-cli
./target/release/octos serve --stdio --solo --danger-full-access
```

## 卸载 / 停止

```bash
# 软停（保留 PVC/配置）：scale replicas=0
kubectl -n octos scale deploy/octos --replicas=0
kubectl -n octos scale deploy/pg --replicas=0

# k8s 按清单卸载
kubectl delete -f deploy/k8s/03-cluster-with-config.yaml
kubectl delete -f deploy/k8s/01-baseline.yaml
# 全清：kubectl delete namespace octos

# docker compose 卸载
cd deploy/docker && docker compose down
```

停止与再部署的完整步骤（binary HTTP、port-forward、AppUI origin）见
[docs/K8S_DEPLOY_PROVEN.md](docs/K8S_DEPLOY_PROVEN.md#停止与再部署) 与
[docs/K8S_INSTALL.md](docs/K8S_INSTALL.md#停止与再部署)。

## 详细文档

- **[k8s 安装说明](docs/K8S_INSTALL.md)** — 完整的 k8s 部署指南，包括前置条件、镜像构建、配置文件注入、故障排查
- **[部署变体对比](docs/DEPLOYMENT_VARIANTS.md)** — baseline / hostpath / cluster 三种部署的差异、适用场景、迁移路径

## 核心概念

### 部署模式

octos 支持两种运行模式：

| 模式 | 触发条件 | 数据持久化 | 适用场景 |
|---|---|---|---|
| **stdio 模式** | `octos serve --stdio` | 本地文件（redb + JSON） | 单进程开发 |
| **cluster 模式** | 设置 `DATABASE_URL` 环境变量 | 本地文件 + **PG**（approvals、cron、events） | 多副本 cluster 部署 |

### ConfigMap + Secret 注入模式（k8s 推荐）

k8s 部署时使用 ConfigMap + Secret 把配置外部化：

```
ConfigMap octos-config    (非敏感)
  ├── DATABASE_URL
  ├── OCTOS_AUTH_TOKEN
  ├── WORKSPACE_DIR
  └── LLM_* 配置

Secret llm-credentials     (敏感)
  └── ANTHROPIC_API_KEY

Deployment octos
  └── envFrom: [configMapRef, secretRef]
```

## 版本历史

- **v1.0** (2026-09-16)：初始版本，包含 baseline / hostpath / cluster 三种 k8s 部署 + docker compose
