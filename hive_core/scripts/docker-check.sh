#!/usr/bin/env bash
# Tests y cobertura ≥ 95 % dentro del servicio Docker `hive`.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MODE="${1:-all}"

COMPOSE=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  COMPOSE=(docker-compose)
fi

run_local() {
  echo "==> cargo test (local)" >&2
  cargo test --release
  echo "==> llvm-cov (local)" >&2
  if cargo llvm-cov --help >/dev/null 2>&1; then
    cargo llvm-cov test --release \
      --fail-under-lines 95 \
      --ignore-filename-regex '^(.*/)?src/main\.rs$' \
      --summary-only \
      --color always
  else
    echo "Instala: rustup component add llvm-tools-preview && cargo install cargo-llvm-cov" >&2
    exit 1
  fi
}

if ! command -v docker >/dev/null 2>&1; then
  echo "Docker no encontrado; usando toolchain local…" >&2
  run_local
  exit 0
fi

if ! docker info >/dev/null 2>&1; then
  echo "Docker no responde; usando toolchain local…" >&2
  run_local
  exit 0
fi

echo "==> Imagen rust-toolchain-tests (build)" >&2
"${COMPOSE[@]}" --profile dev build rust-toolchain-tests

case "$MODE" in
  test)
    "${COMPOSE[@]}" --profile dev run --rm rust-toolchain-tests cargo test --release --verbose
    ;;
  coverage)
    "${COMPOSE[@]}" --profile dev run --rm rust-toolchain-tests cargo llvm-cov test --release \
      --fail-under-lines 95 \
      --ignore-filename-regex '^(.*/)?src/main\.rs$' \
      --summary-only \
      --color always
    ;;
  all|*)
    "${COMPOSE[@]}" --profile dev run --rm rust-toolchain-tests cargo test --release --verbose
    "${COMPOSE[@]}" --profile dev run --rm rust-toolchain-tests cargo llvm-cov test --release \
      --fail-under-lines 95 \
      --ignore-filename-regex '^(.*/)?src/main\.rs$' \
      --summary-only \
      --color always
    ;;
esac

echo "" >&2
echo "Listo: umbral 95 % líneas (main.rs excluido)." >&2
