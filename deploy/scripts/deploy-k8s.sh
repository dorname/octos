#!/bin/bash
# Deploy octos to local k8s cluster (docker-desktop).
#
# Usage: ./deploy/scripts/deploy-k8s.sh [variant]
#   variant=baseline    — single-node, shared docker daemon (default)
#   variant=hostpath     — hostPath mount for binary (dev only)
#   variant=cluster      — full cluster with ConfigMap + Secret
#
# Prerequisites:
#   - kubectl configured (docker-desktop / minikube / kind)
#   - docker images: rust:1.88-alpine, alpine:3.21, postgres:16-alpine
#     (or modify the manifest to use a different registry)

set -e

VARIANT="${1:-baseline}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Pick the right manifest based on variant
case "$VARIANT" in
  baseline)   MANIFEST="$ROOT_DIR/deploy/k8s/01-baseline.yaml" ;;
  hostpath)   MANIFEST="$ROOT_DIR/deploy/k8s/02-hostpath-dev.yaml" ;;
  cluster)    MANIFEST="$ROOT_DIR/deploy/k8s/03-cluster-with-config.yaml" ;;
  *)
    echo "Unknown variant: $VARIANT"
    echo "Usage: $0 [baseline|hostpath|cluster]"
    exit 1
    ;;
esac

echo "=== Deploying variant: $VARIANT ==="
echo "Manifest: $MANIFEST"
echo ""


# ---- cluster variant: refuse to deploy with placeholder LLM secret (#issue2) ----
if [[ "$VARIANT" == "cluster" ]]; then
  LIVE_KEY="$(kubectl -n octos get secret llm-credentials \
    -o jsonpath='{.data.ANTHROPIC_API_KEY}' 2>/dev/null | base64 -d 2>/dev/null || true)"
  if [[ -z "$LIVE_KEY" ]]; then
    echo "ERROR: secret llm-credentials not found in namespace octos." >&2
    echo "  Create it first: kubectl create secret generic llm-credentials" >&2
    echo "    --from-literal=ANTHROPIC_API_KEY=<your-real-key> -n octos" >&2
    exit 1
  fi
  if [[ "$LIVE_KEY" == "REPLACE_ME" ]]; then
    echo "ERROR: secret llm-credentials still holds the REPLACE_ME placeholder." >&2
    echo "  serve would boot healthy but every LLM call would 401." >&2
    echo "  Replace it: kubectl create secret generic llm-credentials" >&2
    echo "    --from-literal=ANTHROPIC_API_KEY=<your-real-key> -n octos" >&2
    echo "    --dry-run=client -o yaml | kubectl apply -f -" >&2
    exit 1
  fi
  KEYLEN=${#LIVE_KEY}; echo "Secret llm-credentials: real key present, len=$KEYLEN."
fi

echo "=== Step 1: namespace + PG ==="
kubectl apply -f "$MANIFEST"

echo ""
echo "=== Step 2: Waiting for PG ==="
kubectl wait --for=condition=ready pod -l app=pg -n octos --timeout=90s

echo ""
echo "=== Step 3: Waiting for octos pods ==="
kubectl wait --for=condition=ready pod -l app=octos -n octos --timeout=120s

echo ""
echo "=== Verifying ==="
kubectl get pods -n octos
kubectl get svc -n octos

echo ""
echo "=== Next steps ==="
echo "  kubectl port-forward -n octos svc/octos 8080:8080"
echo "  curl http://127.0.0.1:8080/health"
echo ""
echo "# For cluster-mode tests (binary MUST be built with --features postgres; #2436):"
echo "  # cargo build --release --target x86_64-unknown-linux-musl -p octos-cli --no-default-features --features api,postgres"
echo "  octoscode \\"
echo "    --endpoint ws://127.0.0.1:8080/api/ui-protocol/ws \\"
echo "    --auth-token test-cluster-token-12345 \\"
echo "    --profile-id cluster-worker \\"
echo "    --cwd /workspace"