# Scripts

## `verify-all.sh`

Comprobación local del monorepo:

- `hive_core`: `cargo fmt --check`, `clippy -D warnings`, `test`, y (si existe) `cargo llvm-cov` al 85 % de líneas.
- `editors/vscode-hive`: `npm ci` y `npm test` (compila TypeScript + pruebas Node del `hiveRunner`).

```bash
./scripts/verify-all.sh
```

En CI (GitHub Actions), el flujo equivalente está en [`.github/workflows/ci.yml`](../.github/workflows/ci.yml).
