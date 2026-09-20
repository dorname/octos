# K8s 本地部署实战记录（docker-desktop + WSL2）

本文档记录在 docker-desktop K8s 集群（WSL2 后端）成功部署 octos 的完整过程，
所有改动都已落地到 `deploy/k8s/03-cluster-with-config.yaml`。

## 最终状态

```
$ kubectl get all -n octos
pod/octos-58d7468fc-6sck6   1/1     Running    0          53s
pod/pg-954b675bb-q9tpf      1/1     Running    0          12m

service/octos   ClusterIP   10.101.59.193   <none>        8080/TCP
service/pg      ClusterIP   10.104.54.69    <none>        5432/TCP

deployment.apps/octos   1/1     1            1           54s
deployment.apps/pg      1/1     1            1           12m

replicaset.apps/octos-58d7468fc   1         1         1       54s
```

```
$ curl http://localhost:9091/health
{"status":"healthy","service":"octos","version":"2.0.3-rc.11+8e84ea6d"}
```

## 部署过程中遇到并修复的 6 个真实问题

### 问题 0：kubectl 证书错误

```
TLS: failed to verify certificate: x509: certificate signed by unknown authority
```

**根因**：docker-desktop daemon 重启后生成了新的 K8s CA，但 `~/.kube/config`
里的旧 CA 没更新。

**修复**：用 Windows 端的新 kubeconfig 替换：

```bash
cp /mnt/c/Users/lgqfi/.kube/config ~/.kube/config
```

### 问题 1：hostPath `/tmp/octos-k8s/octos` mount 失败

```
MountVolume.SetUp failed for volume "octos-binary" :
  hostPath type check failed: /tmp/octos-k8s/octos is not a file
```

**根因**：docker-desktop VM 的 hostPath 对应 Windows VM 的文件系统，
WSL2 的 `/tmp`（tmpfs）VM 看不到；Windows 端 C:\tmp\octos-k8s 也不一致。

**修复**：弃用 hostPath，改用 **initContainer + HTTP fetch**：

```yaml
# init script 在 alpine 容器里从 host 的 HTTP server 拉 binary
wget -q -O /opt/octos/octos http://host.docker.internal:8088/octos
chmod +x /opt/octos/octos
```

host 端：

```bash
mkdir -p /tmp/octos-host
cp target/x86_64-unknown-linux-musl/release/octos /tmp/octos-host/
cd /tmp/octos-host && python3 -m http.server 8088 --bind 0.0.0.0 &
```

binary 通过 emptyDir 共享给主容器。

### 问题 2：init script shell 语法错误

```
/usr/local/bin/init-config.sh: line 57: syntax error: unexpected ")"
```

**根因**：错误提示里包含字面量 `$(dirname <binary>)`，`<binary>` 在 shell
解析中被当成 `<` 重定向符号 + `binary` 命令。

**修复**：把 `<binary>` 改成 `<dir-of-octos-binary>`（避免 `<` 字符）。

### 问题 3：`host.docker.internal` 解析到不同 IP

```
wget: can't connect to remote host (192.168.65.254): Connection refused
```

**根因**：pod 内的 `host.docker.internal` 解析到 `192.168.65.254`
（docker 网桥的 host loopback），但 WSL http.server 默认绑 `127.0.0.1`，
而 docker 看到的 host IP 是 WSL 的 eth1 `192.168.18.220`。

**修复**：http server 显式 `--bind 0.0.0.0`，让任何来源的 8088 访问都接。

### 问题 4：octos 绑 127.0.0.1，readinessProbe 失败

```
Readiness probe failed: Get "http://10.1.0.42:8080/health":
  dial tcp 10.1.0.42:8080: connect: connection refused
```

**根因**：`octos serve --port 8080` 默认 `--host 127.0.0.1`，只接 loopback；
但 kubelet 的 readinessProbe 用 pod IP 访问，连不上。

**修复**：加 `--host 0.0.0.0`：

```yaml
args:
- serve
- --port
- "8080"
- --host
- "0.0.0.0"
```

### 问题 5：2 副本共享同一 PVC，第二个 panic

```
OCTOS_DATA_DIR_LOCKED: another octos server already owns data directory /tmp/octos-data
```

**根因**：`octos-data` PVC 是 `ReadWriteOnce`（单节点绑定），但 deployment
`replicas: 2` 让两个 pod 同时挂同一 PVC；octos 启动时写 lockfile 到
`/tmp/octos-data/`，第二个 pod 看到 lock 拒绝启动。

**修复**：单节点场景降为 `replicas: 1`：

```yaml
spec:
  replicas: 1
```

> 注：未来要做多副本需要支持 `ReadWriteMany` 共享存储 + 协调锁机制（PG advisory lock 等）—— 不在本地部署目标范围。

## 关键改动（已 commit）

`deploy/k8s/03-cluster-with-config.yaml`：
- ✅ `replicas: 1`（替代 2）
- ✅ `command` 加 `--host 0.0.0.0`（替代默认 127.0.0.1）
- ✅ `octos-binary` volume：hostPath → emptyDir
- ✅ init script 步骤 5 改为 wget 从 host.docker.internal:8088 拉 binary
- ✅ 修复 `<binary>` 语法错误（→ `<dir-of-octos-binary>`）

## 部署步骤（用户实际操作）

```bash
# 1. 验证 K8s 集群
kubectl cluster-info  # 若证书错：cp /mnt/c/Users/<user>/.kube/config ~/.kube/config

# 2. 创建 namespace + llm-credentials secret
kubectl create namespace octos
kubectl create secret generic llm-credentials \
  --from-literal=ANTHROPIC_API_KEY=sk-ant-你的key \
  -n octos

# 3. 启动 host HTTP server 提供 binary
mkdir -p /tmp/octos-host
cp target/x86_64-unknown-linux-musl/release/octos /tmp/octos-host/
cd /tmp/octos-host && python3 -m http.server 8088 --bind 0.0.0.0 &

# 4. 应用 manifest
kubectl apply -f deploy/k8s/03-cluster-with-config.yaml

# 5. 等 ~45 秒
kubectl get pods -n octos -w

# 6. 健康检查
kubectl port-forward -n octos svc/octos 9091:8080 &
curl http://localhost:9091/health
# 期望：{"status":"healthy","service":"octos","version":"2.0.3-rc.11+..."}
```

## LLM API Key 占位符注意

如果 secret 用的是占位符（`ANTHROPIC_API_KEY=REPLACE_ME`），serve 启动
OK 但所有 LLM 调用 401。需替换为真实 key：

```bash
kubectl create secret generic llm-credentials \
  --from-literal=ANTHROPIC_API_KEY=sk-ant-api03-真实key \
  -n octos --dry-run=client -o yaml | kubectl apply -f -
# 然后 rollout restart：
kubectl rollout restart deployment/octos -n octos
```