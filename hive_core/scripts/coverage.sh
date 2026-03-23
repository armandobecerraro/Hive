#!/usr/bin/env bash
# Cobertura ≥ 95 %: usa Docker si está disponible (recomendado); si no, llvm-cov local.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
exec "$ROOT/scripts/docker-check.sh" coverage
