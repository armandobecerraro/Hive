#!/usr/bin/env bash
# Verificación local: Rust (hive_core) + extensión VS Code.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> hive_core: cargo fmt --check"
(cd hive_core && cargo fmt --all -- --check)

echo "==> hive_core: cargo clippy -D warnings"
(cd hive_core && cargo clippy --all-targets -- -D warnings)

echo "==> hive_core: cargo test"
(cd hive_core && cargo test)

if command -v cargo >/dev/null 2>&1 && cargo llvm-cov --help >/dev/null 2>&1; then
  echo "==> hive_core: cobertura ≥ 94 % líneas"
  (cd hive_core && cargo llvm-cov test \
    --fail-under-lines 94 \
    --ignore-filename-regex '^(.*/)?src/main\.rs$' \
    --summary-only)
else
  echo "==> hive_core: omitiendo llvm-cov (instala cargo-llvm-cov y llvm-tools-preview)"
fi

echo "==> editors/vscode-hive: npm ci / test"
(cd editors/vscode-hive && npm ci && npm test)

echo "==> Listo: todas las comprobaciones pasaron."
