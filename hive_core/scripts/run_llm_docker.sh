#!/usr/bin/env bash
# Arranca Ollama, job ollama-model-download y ejecuta un ciclo de La Hive con LLM real.
# Uso: desde hive_core/
#   ./scripts/run_llm_docker.sh [ruta_bajo_workspace]
# Ejemplo: ./scripts/run_llm_docker.sh test_repo
# Opcional: export OLLAMA_MODEL=llama3.2:3b  (o define OLLAMA_MODEL en .env junto a docker-compose.yml)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

COMPOSE=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  COMPOSE=(docker-compose)
fi

TARGET="${1:-test_repo}"
mkdir -p "workspace/${TARGET}"

if ! command -v docker >/dev/null 2>&1; then
  echo "Docker no está instalado o no está en PATH." >&2
  exit 1
fi
if ! docker info >/dev/null 2>&1; then
  echo "Docker no responde (¿daemon arrancado?)." >&2
  exit 1
fi

echo "==> Build + un ciclo con LLM (repo montado en /workspace/${TARGET})" >&2
# llm: ollama + ollama-model-download; llm-once: hive-llm (un ciclo; hive-colmena es el daemon).
exec "${COMPOSE[@]}" --profile llm --profile llm-once run --rm --build hive-llm --once "/workspace/${TARGET}"
