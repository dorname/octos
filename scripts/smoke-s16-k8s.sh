#!/usr/bin/env bash
# SMOKE-S16-* runner for local docker-desktop octos namespace.
# Writes OpenLogos smoke-results.jsonl lines.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESULT_PATH="${OPENLOGOS_SMOKE_RESULT_PATH:-$ROOT/logos/resources/verify/smoke-results.jsonl}"
NS="${OCTOS_SMOKE_NS:-octos}"
KUBECTL=(kubectl)
if [[ -n "${OCTOS_SMOKE_K8S_SERVER:-}" ]]; then
  KUBECTL=(kubectl --server="$OCTOS_SMOKE_K8S_SERVER")
elif [[ -S /var/run/docker.sock ]] || [[ -n "${KUBERNETES_SERVICE_HOST:-}" ]]; then
  # Prefer explicit local API when present (WSL docker-desktop).
  if curl -sk --max-time 1 https://127.0.0.1:6443/livez >/dev/null 2>&1; then
    KUBECTL=(kubectl --server=https://127.0.0.1:6443)
  fi
fi

mkdir -p "$(dirname "$RESULT_PATH")"
: >"$RESULT_PATH"

ts() { date -u +"%Y-%m-%dT%H:%M:%SZ"; }

write_result() {
  local id="$1" status="$2" scenario="$3" error="${4:-}"
  if [[ "$status" == "fail" ]]; then
    printf '{"id":"%s","status":"%s","timestamp":"%s","scenario":"%s","error":%s}\n' \
      "$id" "$status" "$(ts)" "$scenario" "$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$error")" \
      >>"$RESULT_PATH"
  else
    printf '{"id":"%s","status":"%s","timestamp":"%s","scenario":"%s"}\n' \
      "$id" "$status" "$(ts)" "$scenario" >>"$RESULT_PATH"
  fi
}

ensure_port_forward() {
  if curl -sf --max-time 2 http://127.0.0.1:50080/health >/dev/null 2>&1; then
    return 0
  fi
  pkill -f 'port-forward svc/octos 50080:8080' >/dev/null 2>&1 || true
  "${KUBECTL[@]}" -n "$NS" port-forward svc/octos 50080:8080 >/tmp/octos-smoke-pf.log 2>&1 &
  local i
  for i in $(seq 1 30); do
    if curl -sf --max-time 1 http://127.0.0.1:50080/health >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

echo "=== SMOKE-S16 against ns=$NS ==="

# SMOKE-S16-01
if "${KUBECTL[@]}" -n "$NS" get deploy/octos >/dev/null 2>&1 \
  && "${KUBECTL[@]}" -n "$NS" wait --for=condition=available deploy/octos --timeout=90s >/dev/null 2>&1 \
  && ensure_port_forward \
  && curl -sf --max-time 5 http://127.0.0.1:50080/health | grep -q '"status":"healthy"'; then
  write_result SMOKE-S16-01 pass core-S16
  echo "PASS SMOKE-S16-01"
else
  write_result SMOKE-S16-01 fail core-S16 "octos Service/health unreachable"
  echo "FAIL SMOKE-S16-01"
fi

# SMOKE-S16-02
if curl -sf --max-time 5 http://127.0.0.1:50080/api/version | grep -q '"service":"octos"'; then
  write_result SMOKE-S16-02 pass core-S16
  echo "PASS SMOKE-S16-02"
else
  write_result SMOKE-S16-02 fail core-S16 "GET /api/version did not return service=octos"
  echo "FAIL SMOKE-S16-02"
fi

# SMOKE-S16-03
TABLES="$("${KUBECTL[@]}" -n "$NS" exec deploy/pg -- psql -U postgres -d octos -Atc \
  "SELECT string_agg(tablename, ',' ORDER BY tablename) FROM pg_tables WHERE schemaname='public';" 2>/dev/null || true)"
need_ok=1
for t in sessions session_events approvals run_leases schedules; do
  if [[ ",$TABLES," != *",$t,"* ]]; then
    need_ok=0
  fi
done
if [[ "$need_ok" -eq 1 && -n "$TABLES" ]]; then
  write_result SMOKE-S16-03 pass core-S16
  echo "PASS SMOKE-S16-03 ($TABLES)"
else
  write_result SMOKE-S16-03 fail core-S16 "missing required PG tables; got: ${TABLES:-<empty>}"
  echo "FAIL SMOKE-S16-03"
fi

# SMOKE-S16-04 — delete pod, wait Ready, health again
OLD="$("${KUBECTL[@]}" -n "$NS" get pod -l app=octos -o jsonpath='{.items[0].metadata.name}' 2>/dev/null || true)"
if [[ -z "$OLD" ]]; then
  write_result SMOKE-S16-04 fail core-S16 "no octos pod to recreate"
  echo "FAIL SMOKE-S16-04"
else
  "${KUBECTL[@]}" -n "$NS" delete pod "$OLD" --wait=false >/dev/null
  if "${KUBECTL[@]}" -n "$NS" rollout status deploy/octos --timeout=180s >/dev/null \
    && ensure_port_forward \
    && curl -sf --max-time 5 http://127.0.0.1:50080/health | grep -q '"status":"healthy"'; then
    write_result SMOKE-S16-04 pass core-S16
    echo "PASS SMOKE-S16-04 (recreated from $OLD)"
  else
    write_result SMOKE-S16-04 fail core-S16 "pod recreate did not restore Ready/health"
    echo "FAIL SMOKE-S16-04"
  fi
fi

echo "Results → $RESULT_PATH"
cat "$RESULT_PATH"
