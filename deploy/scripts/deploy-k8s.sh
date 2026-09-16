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
echo "For cluster-mode tests:"
echo "  /home/kyle/octoscode-test/target/x86_64-unknown-linux-musl/release/octoscode \\"
echo "    --endpoint ws://127.0.0.1:8082/api/ui-protocol/ws \\"
echo "    --auth-token test-cluster-token-12345 \\"
echo "    --cwd /workspace"