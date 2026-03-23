# Hive

Orquestación local (**La Reina**, obreras, Consejo) sobre repositorios Git. El núcleo Rust vive en [`hive_core/`](hive_core/README.md).

## Comprobar todo antes de un commit

```bash
./scripts/verify-all.sh
```

## CI

[`.github/workflows/ci.yml`](.github/workflows/ci.yml): tests Rust (macOS + Ubuntu + cobertura), fmt, clippy y extensión VS Code (`npm test`).

## Extensión de editor

[`editors/vscode-hive/`](editors/vscode-hive/README.md) — panel de misión compatible con VS Code, Cursor, Windsurf, etc.
