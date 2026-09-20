#!/usr/bin/env bash
# S17 Watchdog 隔离 systemd user smoke；不会向真实 agent 窗格注入。
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESULT_PATH="${OPENLOGOS_SMOKE_RESULT_PATH:-${ROOT}/logos/resources/verify/smoke-results.jsonl}"
OCTOS_BIN="${OCTOS_BIN:-${ROOT}/target/debug/octos}"
# 用户级 systemd manager 按账户 pw_dir(而非 $HOME 环境变量)搜索 unit;
# 覆盖 HOME 前先记录真实 unit 目录,供 link/cleanup 使用。
REAL_HOME="$(getent passwd "$(id -un)" | cut -d: -f6)"
REAL_UNIT_DIR="${REAL_HOME}/.config/systemd/user"
# unit 带 PrivateTmp=yes：/tmp 与 /var/tmp 在服务命名空间内不可见,
# 隔离现场必须放在二者之外(选 /run,tmpfs 且仅本次运行使用)。
TMP="$(mktemp -d /run/octos-watchdog-smoke.XXXXXX)"
PROJECT="${TMP}/project"
export XDG_CONFIG_HOME="${TMP}/config"
export XDG_STATE_HOME="${TMP}/state"
export HOME="${TMP}/home"
mkdir -p "$PROJECT/.octos" "$TMP/bin" "$HOME"
PROJECT="$(realpath "$PROJECT")"
PROJECT_ID="$(printf '%s' "$PROJECT" | sha256sum | cut -c1-24)"
UNIT="octos-watchdog@${PROJECT_ID}.service"
CONFIG="${XDG_CONFIG_HOME}/octos/watchdog/${PROJECT_ID}.toml"
STATE="${XDG_STATE_HOME}/octos/watchdog/${PROJECT_ID}/state.json"
PROMPTS="${TMP}/prompts.log"
EVENTS="${TMP}/events.jsonl"
FAILURES=0

mkdir -p "$(dirname "$RESULT_PATH")"
# 账本是累计的:只剔除本场景(S17)旧记录,保留其他场景(如 S16)的历史证据。
if [[ -f "$RESULT_PATH" ]]; then
  grep -vE '"scenario": ?"S17"' "$RESULT_PATH" >"${RESULT_PATH}.keep" || true
  mv "${RESULT_PATH}.keep" "$RESULT_PATH"
else
  : >"$RESULT_PATH"
fi
: >"$PROMPTS"
: >"$EVENTS"

timestamp() { date -u +"%Y-%m-%dT%H:%M:%SZ"; }

record() {
  local id="$1" status="$2" error="${3:-}"
  if [[ "$status" == "pass" ]]; then
    printf '{"id":"%s","status":"pass","timestamp":"%s","scenario":"S17"}\n' \
      "$id" "$(timestamp)" >>"$RESULT_PATH"
    echo "PASS $id"
  else
    local encoded
    encoded="$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$error")"
    printf '{"id":"%s","status":"fail","timestamp":"%s","scenario":"S17","error":%s}\n' \
      "$id" "$(timestamp)" "$encoded" >>"$RESULT_PATH"
    echo "FAIL $id: $error"
    FAILURES=$((FAILURES + 1))
  fi
}

wait_for() {
  local attempts="$1"
  shift
  local index
  for ((index=0; index<attempts; index++)); do
    if "$@"; then
      return 0
    fi
    sleep 1
  done
  return 1
}

cleanup() {
  systemctl --user disable --now "$UNIT" >/dev/null 2>&1 || true
  rm -f "${XDG_CONFIG_HOME}/systemd/user/octos-watchdog@.service"
  # 摘除 link 到真实 unit 目录的模板符号链接及 enable 生成的实例链接。
  rm -f "${REAL_UNIT_DIR}/octos-watchdog@.service" \
    "${REAL_UNIT_DIR}/octos-watchdog@${PROJECT_ID}.service" \
    "${REAL_UNIT_DIR}/default.target.wants/octos-watchdog@${PROJECT_ID}.service"
  systemctl --user daemon-reload >/dev/null 2>&1 || true
  if [[ "$FAILURES" -eq 0 ]]; then
    rm -rf "$TMP"
  else
    echo "失败现场保留：$TMP" >&2
  fi
}
trap cleanup EXIT

if [[ ! -x "$OCTOS_BIN" ]]; then
  record SMOKE-S17-01 fail "octos 二进制不可执行：$OCTOS_BIN"
  for id in 02 03 04 05 06; do
    record "SMOKE-S17-${id}" fail "前置二进制不可用"
  done
  exit 1
fi
# unit 带 PrivateTmp=yes：/tmp、/var/tmp 在服务命名空间内不可见。openlogos smoke
# 沙箱会把 workspace 复制到 /tmp 下运行，因此必须把服务二进制转进 /run 隔离现场。
cp "$OCTOS_BIN" "${TMP}/octos"
chmod 755 "${TMP}/octos"
OCTOS_BIN="${TMP}/octos"
if ! systemctl --user show-environment >/dev/null 2>&1; then
  record SMOKE-S17-01 fail "当前环境无可用的 systemd --user bus"
  for id in 02 03 04 05 06; do
    record "SMOKE-S17-${id}" fail "前置 systemd user 环境不可用"
  done
  exit 1
fi

cat >"$PROJECT/.octos/OUTER_LOOP_REVIEW.md" <<'BOARD'
# OLP

## Active

### #1 已完成基线
ACK(done): #1 baseline
BOARD

git -C "$PROJECT" init -q
git -C "$PROJECT" config user.email smoke@example.invalid
git -C "$PROJECT" config user.name "S17 Smoke"
git -C "$PROJECT" add .octos/OUTER_LOOP_REVIEW.md
git -C "$PROJECT" commit -qm baseline

cat >"$TMP/bin/herdr" <<HERDR
#!/usr/bin/env bash
if [[ "\${1:-} \${2:-}" == "agent list" ]]; then
  printf '%s\n' '{"result":{"agents":[{"agent":"octoscode","agent_status":"idle","cwd":"$PROJECT","pane_id":"inner:p1"},{"agent":"claude","agent_status":"idle","cwd":"$PROJECT","pane_id":"outer:p2"}]}}'
elif [[ "\${1:-} \${2:-}" == "agent prompt" ]]; then
  printf '%s\n' "\$*" >>"$PROMPTS"
  printf '%s\n' '{"status":"accepted"}'
else
  exit 2
fi
HERDR
cat >"$TMP/bin/octoscode" <<'OCTOSCODE'
#!/usr/bin/env bash
if [[ "${1:-} ${2:-}" == "outer-duty check" ]]; then
  printf 'HELD\n{"holder":"smoke-outer"}\n'
else
  exit 2
fi
OCTOSCODE
chmod 700 "$TMP/bin/herdr" "$TMP/bin/octoscode"
export PATH="$TMP/bin:$PATH"

if ! "$ROOT/deploy/scripts/install-watchdog-user-service.sh" --project "$PROJECT" --octos-bin "$OCTOS_BIN" >/dev/null; then
  record SMOKE-S17-01 fail "安装脚本失败"
  for id in 02 03 04 05 06; do
    record "SMOKE-S17-${id}" fail "前置安装失败"
  done
  exit 1
fi

# 隔离 XDG_CONFIG_HOME 对 manager 不可见：通过 link 将模板挂进真实 unit 搜索路径。
if ! systemctl --user link "${XDG_CONFIG_HOME}/systemd/user/octos-watchdog@.service" >/dev/null 2>&1; then
  record SMOKE-S17-01 fail "systemctl --user link 失败"
  for id in 02 03 04 05 06; do
    record "SMOKE-S17-${id}" fail "前置 link 失败"
  done
  exit 1
fi
systemctl --user daemon-reload >/dev/null 2>&1

cat >"$CONFIG" <<CONFIG
version = 1
project = "$PROJECT"
poll_interval_secs = 1
idle_after_secs = 0
observation_window_secs = 1
max_no_progress_retries = 3

[sources]
board = ".octos/OUTER_LOOP_REVIEW.md"
events = "$EVENTS"

[agents]
inner_kind = "octoscode"
outer_kind = "claude"

[alerts]
stderr = true
file = "$TMP/alerts.jsonl"
CONFIG
chmod 600 "$CONFIG"

systemctl --user enable --now "$UNIT" >/dev/null 2>&1
if wait_for 15 test -s "$STATE"; then
  BEFORE_PID="$(systemctl --user show "$UNIT" -p MainPID --value)"
  BEFORE_STATUS="$($OCTOS_BIN watchdog status --config "$CONFIG" --json)"
  kill -KILL "$BEFORE_PID"
  if wait_for 15 bash -c 'pid="$(systemctl --user show "$1" -p MainPID --value)"; [[ "$pid" != "0" && "$pid" != "$2" ]]' _ "$UNIT" "$BEFORE_PID"; then
    AFTER_STATUS="$($OCTOS_BIN watchdog status --config "$CONFIG" --json)"
    if python3 -c 'import json,sys; a=json.loads(sys.argv[1]); b=json.loads(sys.argv[2]); assert a["board_cursor"]==b["board_cursor"] and a["event_cursor"]==b["event_cursor"]' "$BEFORE_STATUS" "$AFTER_STATUS"; then
      record SMOKE-S17-01 pass
    else
      record SMOKE-S17-01 fail "服务重启后 cursor 不一致"
    fi
  else
    record SMOKE-S17-01 fail "Restart=always 未拉起新主进程"
  fi
else
  record SMOKE-S17-01 fail "服务未生成状态文件"
fi

printf 'ACK(done): #1 smoke-new\n' >>"$PROJECT/.octos/OUTER_LOOP_REVIEW.md"
if wait_for 10 bash -c '[[ "$(grep -c "outer:p2.*BoardAck" "$1" 2>/dev/null || true)" -eq 1 ]]' _ "$PROMPTS"; then
  sleep 2
  if [[ "$(grep -c 'outer:p2.*BoardAck' "$PROMPTS" || true)" -eq 1 ]]; then
    record SMOKE-S17-02 pass
  else
    record SMOKE-S17-02 fail "ACK 被重复投递"
  fi
else
  record SMOKE-S17-02 fail "ACK 未投递到 HELD 外环"
fi

CHECKPOINT="$TMP/checkpoint"
printf 'keep' >"$CHECKPOINT"
printf '%s\n' '{"type":"goal_transition","to":"budget_limited","goal_id":"g-smoke","checkpoint":"'$CHECKPOINT'"}' >>"$EVENTS"
if wait_for 10 grep -q 'outer:p2.*BudgetLimited' "$PROMPTS" && [[ "$(cat "$CHECKPOINT")" == "keep" ]]; then
  record SMOKE-S17-03 pass
else
  record SMOKE-S17-03 fail "budget_limited 门铃或 checkpoint 保留失败"
fi

printf '\n### #2 smoke pending\n尚未 ACK\n' >>"$PROJECT/.octos/OUTER_LOOP_REVIEW.md"
if wait_for 10 grep -q 'inner:p1.*#2' "$PROMPTS"; then
  record SMOKE-S17-04 pass
else
  record SMOKE-S17-04 fail "idle 内环未收到最小未 ACK 条目"
fi

if wait_for 12 grep -q 'inner_no_progress_fused' "$TMP/alerts.jsonl"; then
  COUNT="$(grep -c 'inner:p1.*#2' "$PROMPTS" || true)"
  sleep 2
  AFTER_COUNT="$(grep -c 'inner:p1.*#2' "$PROMPTS" || true)"
  if [[ "$COUNT" -eq 3 && "$AFTER_COUNT" -eq 3 ]]; then
    record SMOKE-S17-05 pass
  else
    record SMOKE-S17-05 fail "期望正好三次 prompt，实际 ${COUNT}/${AFTER_COUNT}"
  fi
else
  record SMOKE-S17-05 fail "三次无进展后未熔断"
fi

OUTER_BEFORE="$(grep -c 'outer:p2' "$PROMPTS" || true)"
systemctl --user restart "$UNIT"
sleep 3
OUTER_AFTER="$(grep -c 'outer:p2' "$PROMPTS" || true)"
if [[ "$OUTER_BEFORE" -eq "$OUTER_AFTER" ]]; then
  record SMOKE-S17-06 pass
else
  record SMOKE-S17-06 fail "重启后重放已 delivered 信号"
fi

if [[ "$FAILURES" -eq 0 ]]; then
  touch "$(dirname "$RESULT_PATH")/SMOKE_PASS"
  exit 0
fi
rm -f "$(dirname "$RESULT_PATH")/SMOKE_PASS"
exit 1
