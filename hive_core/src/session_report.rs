//! Resumen legible para el humano tras cada ciclo: visor de cambios + siguiente paso (iteración).

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use tracing::info;

const FILENAME: &str = "HIVE_SESSION.md";

/// Escribe `HIVE_SESSION.md` en la raíz del repo objetivo con diff vs `main` y cómo continuar.
pub fn write_session_handoff(repo_root: &Path, integration_branch: &str) -> Result<()> {
    let stat = git_diff_stat(repo_root);
    let names = git_diff_names(repo_root);
    let utc = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    let body = format!(
        r#"# Hive — resumen del último ciclo

**Cuándo:** {utc}
**Rama de integración:** `{integration_branch}`

## Qué cambió en el repo (respecto a `main`)

Úsalo como **lista para revisar** (equivalente a un visor de cambios: mismas rutas).

### Estadísticas (`git diff main --stat`)

```text
{stat}
```

### Rutas tocadas

```text
{names}
```

## Próximo paso (tú mandas, como en Cursor)

1. **Revisa** en tu IDE o con `git diff main` lo que aparece arriba.
2. **Escribe** qué quieres que haga el enjambre en **`hive.client_feedback.md`** (en la raíz del repo), o define **`HIVE_CLIENT_FEEDBACK`** con el texto antes del próximo ciclo.
3. Vuelve a ejecutar Hive (`hive_core --once <repo>`). La Reina incorporará tu texto a la misión del ciclo siguiente.

Si prefieres un objetivo fijo, crea **`hive.request.json`** con `title` y `description`.

## Referencia rápida

| Archivo | Rol |
|---------|-----|
| `hive.request.json` | Pedido explícito (objetivo del ciclo). |
| `hive.client_feedback.md` | Tu respuesta / iteración: “haz X”, “corrige Y”. |
| `hive.json` | Historial interno del Consejo. |
| `docs/HIVE_IMPROVEMENTS.md` | Notas de revisión (si el modo las genera). |
"#,
        utc = utc,
        integration_branch = integration_branch,
        stat = stat,
        names = names,
    );
    let path = repo_root.join(FILENAME);
    std::fs::write(&path, &body).with_context(|| format!("escribir {}", path.display()))?;
    info!(
        path = %path.display(),
        "escrito HIVE_SESSION.md — revisa cambios y continúa con hive.client_feedback.md o hive.request.json"
    );
    Ok(())
}

fn git_diff_stat(repo: &Path) -> String {
    for base in ["main", "origin/main", "master"] {
        if let Ok(o) = Command::new("git")
            .current_dir(repo)
            .args(["diff", base, "--stat"])
            .output()
        {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout).to_string();
                if !s.trim().is_empty() {
                    return s;
                }
            }
        }
    }
    "(sin cambios frente a main, o `main`/`origin/main` no disponible en este clon)\n".to_string()
}

fn git_diff_names(repo: &Path) -> String {
    for base in ["main", "origin/main", "master"] {
        if let Ok(o) = Command::new("git")
            .current_dir(repo)
            .args(["diff", base, "--name-only"])
            .output()
        {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout).to_string();
                if !s.trim().is_empty() {
                    return s;
                }
            }
        }
    }
    "(ninguna)\n".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn session_handoff_writes_file() {
        let tmp = tempfile::tempdir().unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(tmp.path())
            .output()
            .expect("git init");
        Command::new("git")
            .args(["config", "user.email", "t@t.co"])
            .current_dir(tmp.path())
            .output()
            .expect("git config");
        Command::new("git")
            .args(["config", "user.name", "t"])
            .current_dir(tmp.path())
            .output()
            .expect("git config");
        std::fs::write(tmp.path().join("f.txt"), "x").unwrap();
        Command::new("git")
            .args(["add", "f.txt"])
            .current_dir(tmp.path())
            .output()
            .expect("git add");
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(tmp.path())
            .output()
            .expect("git commit");
        let _ = Command::new("git")
            .args(["branch", "-M", "main"])
            .current_dir(tmp.path())
            .output();
        std::fs::write(tmp.path().join("f.txt"), "xy").unwrap();
        write_session_handoff(tmp.path(), "hive/test").unwrap();
        let p = tmp.path().join(FILENAME);
        let s = std::fs::read_to_string(&p).unwrap();
        assert!(s.contains("Hive — resumen"));
        assert!(s.contains("hive/test"));
    }
}
