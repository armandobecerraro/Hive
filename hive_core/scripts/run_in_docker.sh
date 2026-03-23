#!/usr/bin/env bash
# Construye la imagen de producción y ejecuta La Reina sobre un directorio bajo ./workspace
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

COMPOSE=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  COMPOSE=(docker-compose)
fi

TARGET="${1:-test_repo}"
mkdir -p "workspace/${TARGET}"

echo "==> Build hive-prod"
"${COMPOSE[@]}" --profile prod build hive-prod

echo "==> Ejecutando hive_core sobre /workspace/${TARGET}"
exec "${COMPOSE[@]}" --profile prod run --rm hive-prod "/workspace/${TARGET}"
