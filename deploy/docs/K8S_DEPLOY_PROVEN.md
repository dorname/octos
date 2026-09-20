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

### 问题 6：`rollout restart` 后新 pod CrashLoopBackOff，rollout 永久卡住

```
OCTOS_DATA_DIR_LOCKED: another octos server already owns data directory /tmp/octos-data
```

（与问题 5 同一报错，但触发路径不同。）

**根因**：Deployment 用默认 `RollingUpdate` 策略（`maxSurge: 25%`）。
即使 `replicas: 1`，rollout 期间也会**先起新 pod、再杀旧 pod**——新旧
两个 pod 短暂并存，新 pod 撞上旧 pod 持有的 `/tmp/octos-data` lockfile，
直接退出 → CrashLoopBackOff → 新 pod 永远不 Ready → rollout 无法完成，
Service 一直把流量发给**旧 pod（旧 binary/旧前端）**。
`kubectl rollout status` 会一直 hang。

**修复**：octos Deployment 显式声明 `strategy: Recreate`——先完全终止
旧 pod（释放 lockfile + RWO 挂载），再启动新 pod。单节点本地部署可接受
秒级停机：

```yaml
spec:
  replicas: 1
  strategy:
    type: Recreate
```

**验证**：`kubectl apply` 后 `kubectl rollout status deployment/octos -n octos`
能正常完成，且只有 1 个 octos pod。

### 问题 7：rollout 后前端报 "Unable to establish the UI Protocol connection"

前端 SPA（`localhost:9091/app/chat`）页面能开，但发消息报 UI Protocol
连接失败。

**根因**：`kubectl port-forward` 的目标 pod 被 rollout 替换后，端口转发
进程会死亡（"lost connection to pod"）或随宿主 shell 退出——此时
localhost:9091 没有任何监听。浏览器里已打开的 SPA 是内存中的旧页面，
发消息时 WS 握手 TCP 被拒，浏览器不暴露具体原因，前端 10 秒启动超时后
抛出该通用错误。

**修复**：每次 rollout / pod 重建后**重启 port-forward**：

```bash
kubectl port-forward -n octos svc/octos 9091:8080 &
```

然后刷新浏览器页面。若仍报错，检查：

```bash
# 1. port-forward 进程活着、9091 有监听
curl http://localhost:9091/health
# 2. 服务端 WS 握手正常（带 token 应返回 101）
curl -i -H "Connection: Upgrade" -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  -H "Origin: http://localhost:9091" \
  "http://localhost:9091/api/ui-protocol/ws?token=$OCTOS_AUTH_TOKEN"
# 3. 以上都正常但浏览器仍失败 → 浏览器 token 失效，重新登录
```

**WSL2 特别注意（2026-09-17 实战）**：浏览器在 Windows 侧，而
port-forward 跑在 WSL 里时，NAT 模式下 WSL 的 localhostForwarding 只把
**IPv4** `127.0.0.1:9091` 转发到 WSL；Windows 的 `localhost` 优先解析到
IPv6 `[::1]`，而 `[::1]:9091` 上可能有已失效的僵尸转发（accept 连接但
永不响应）——浏览器 TCP 连接"成功"后握手挂死，前端 10 秒超时抛同一条
通用错误。表现为：`curl http://127.0.0.1:9091/health` 通、
`curl http://localhost:9091/health` 超时。此时**浏览器改用
`http://127.0.0.1:9091/app/chat`**（`http://127.0.0.1:9091` 已在
`OCTOS_APPUI_ALLOWED_ORIGINS` 白名单内），或 `wsl --shutdown` 后重启
port-forward 清除僵尸转发。

### 问题 8：前端报 UI Protocol 连接失败，实为 `session/open` RPC 报 unknown provider

前端横幅 "Unable to establish the UI Protocol connection" 是**通用启动超时**文案，
WS 传输层其实可能完全正常。实战排查路径（按序排除）：

1. WS 升级到 `/api/ui-protocol/ws` 返回 **101**（传输层正常）
2. `client_hello` 拿到 `server_hello`（能力协商正常）
3. `session/open` 返回 `-32603: failed to bootstrap ProfileRuntime ... unknown
   provider: minimax-token` —— **真正根因**

**根因**：`crates/octos-llm/src/registry/minimax_token.rs` 家族文件存在但
`registry/mod.rs` 从未注册（缺 `mod minimax_token;` 和 `ALL` 条目），而 PVC 上
admin profile 的 `llm.primary.family_id` 已指向 `minimax-token`（修复见
specs/task-minimax-token-registry-wiring.spec.md）。`session/open` 需要 bootstrap
profile 的 LLM provider，lookup 失败 → RPC 错误 → 前端断开重试 → 10 秒超时横幅。

**修复**：注册家族并重新构建部署 binary（`+48fb033b` 起已含）。
**排查手法**：在 9091 上临时跑一个记录首包的 TCP relay（转发到 19091 的
kubectl port-forward），即可看到浏览器真实的 upgrade 请求与每个 WS RPC 帧。

**教训**：页面能开 ≠ WS 通；WS 通（101）≠ session/open 成功。三层要分开验证。

## 关键改动（已 commit）

`deploy/k8s/03-cluster-with-config.yaml`：
- ✅ `replicas: 1`（替代 2）
- ✅ `strategy: Recreate`（替代默认 RollingUpdate，见问题 6）
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