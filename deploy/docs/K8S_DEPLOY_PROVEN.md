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

> 系统化说明（三层路径语义对照表、`:18088` Windows 侧实况、判定标准）见
> [K8S_INSTALL.md · WSL2 + Docker Desktop：binary 提供方式](./K8S_INSTALL.md#wsl2--docker-desktopbinary-提供方式重要)。

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

## PVC 残留 profile 会覆盖 ConfigMap 的 LLM 配置（优先级语义）

`DEFAULT_PROFILE` 等 ConfigMap 变量**只在 profile 文件不存在时**生效——init
脚本对 profile 采取"存在即跳过"策略。而 `octos-data` PVC 的寿命独立于
Deployment：删 pod、`rollout restart` 甚至 `kubectl delete -f` 都**不会**
删掉 PVC 上已写入的 `profiles/<id>.json`。

结果：你改了 ConfigMap 的 `LLM_MODEL`/`LLM_PROVIDER` → `kubectl apply` →
新 pod 起来**仍用旧模型**。这不是配置没生效，是 PVC 上的旧 profile 文件
优先级更高。

**判定**（看 pod 实际加载的 profile，而不是 ConfigMap）：

```bash
kubectl -n octos exec deploy/octos --   cat /tmp/octos-data/profiles/cluster-worker.json
```

**清理手段（按力度）**：

```bash
# A. 只删指定 profile 文件（下一次 init 会按 ConfigMap 重建）
kubectl -n octos exec deploy/octos --   rm /tmp/octos-data/profiles/cluster-worker.json
kubectl -n octos rollout restart deploy/octos

# B. 删整个 octos-data PVC（LLM key 若只存 profile env_vars 会一并丢失；
#    存在 K8s Secret 里的不受影响）
kubectl -n octos delete pvc octos-data
kubectl -n octos delete pod -l app=octos   # 令 PVC 重新绑定并重建

# C. 全清（连 PG 数据、workspace 一起）：kubectl delete namespace octos
```

> 注意：admin 等通过 API/UI 创建的 profile 同样落在该 PVC；仅删
> `cluster-worker.json` 不会动它们。

## 停止与再部署

### 停止（按力度）

**A. 只停 Pod，保留 PVC / ConfigMap / Secret（可快速再拉起）：**

```bash
kubectl -n octos scale deploy/octos --replicas=0
kubectl -n octos scale deploy/pg --replicas=0
pkill -f 'port-forward.*octos' || true
```

**B. 按清单卸载资源（namespace 可能仍在；PVC 是否删除以 yaml 为准）：**

```bash
pkill -f 'port-forward.*octos' || true
kubectl delete -f deploy/k8s/03-cluster-with-config.yaml
```

**C. 整命名空间清掉（含 PG / workspace 等持久数据）：**

```bash
kubectl delete namespace octos
```

同时停掉本机给 init 拉 binary 的 HTTP（manifest 默认 `:8088`；若 live
ConfigMap 改过端口则以实际为准，例如 Windows 上的 `:18088`）。

### 再部署（cluster + musl postgres binary）

```bash
cd <repo-root>   # 含 logos/logos.config.json / deploy/ 的目录

# 1. 构建（cluster 必须带 postgres，见 #2436）
cargo build --release --target x86_64-unknown-linux-musl -p octos-cli \
  --no-default-features --features api,postgres

# 2. 提供 binary HTTP（init 从 host.docker.internal 拉取）
#    默认端口与 03-cluster-with-config.yaml 一致：8088
#    WSL2 + Docker Desktop：WSL 里 listen 常进不了集群；优先在 Windows 侧
#    对含 octos 文件的目录执行：python -m http.server 8088 --bind 0.0.0.0
mkdir -p /tmp/octos-k8s-bin
cp target/x86_64-unknown-linux-musl/release/octos /tmp/octos-k8s-bin/octos
# （若坚持在 WSL 起 HTTP 且集群已 patch 为 18088，则端口必须与 ConfigMap 一致）

# 3. 应用
./deploy/scripts/deploy-k8s.sh cluster
# 或：kubectl apply -f deploy/k8s/03-cluster-with-config.yaml
kubectl -n octos rollout status deploy/octos --timeout=300s

# 4. 访问（任选本地端口；须与 OCTOS_APPUI_ALLOWED_ORIGINS 对齐）
kubectl -n octos port-forward svc/octos 50080:8080
curl -sf http://127.0.0.1:50080/health
# App: http://127.0.0.1:50080/app/
```

若 port-forward 使用 **50080**（而非文档早期的 9091），确认 Deployment 的
`OCTOS_APPUI_ALLOWED_ORIGINS` 含：

`http://127.0.0.1:50080,http://localhost:50080`

（serve 只会自动放行**容器绑定端口** 8080 的 loopback，不会自动放行 PF 端口。）

```bash
kubectl -n octos set env deploy/octos \
  OCTOS_APPUI_ALLOWED_ORIGINS='http://localhost:9091,http://127.0.0.1:9091,http://localhost:8080,http://127.0.0.1:8080,http://127.0.0.1:50080,http://localhost:50080'
kubectl -n octos rollout status deploy/octos --timeout=300s
```

### port-forward 端口被占用

`bind: address already in use` 时，WSL 的 `pkill` 可能杀不到 Windows 侧
`kubectl.exe` 监听。先清占用再转发：

```bash
pkill -f 'port-forward.*50080' || true
# Windows PowerShell：
# Get-NetTCPConnection -LocalPort 50080 -State Listen -ErrorAction SilentlyContinue |
#   ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -ErrorAction SilentlyContinue }
kubectl -n octos port-forward svc/octos 50080:8080
```
