#!/usr/bin/env bash
# Verificación: tests + cobertura (misma lógica que CI) vía scripts/docker-check.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [ ! -f Cargo.toml ]; then
  echo "Ejecuta desde el directorio hive_core" >&2
  exit 1
fi

exec "$ROOT/scripts/docker-check.sh" all
