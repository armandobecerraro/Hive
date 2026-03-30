# Workspace local (no forma parte del repo Hive)

Esta carpeta es **solo para pruebas en tu máquina**: aquí Hive crea o coloniza proyectos (p. ej. `test_repo/` con Flutter).

- **No se versiona** el contenido (está en `.gitignore` en la raíz del monorepo Hive).
- El **repositorio Git del producto** vive aquí como carpeta anidada; los cambios de La Reina **no deben aparecer** en `git status` del proyecto Hive.
- Para inspeccionar el sandbox: `cd hive_core/workspace/test_repo` y usa el Git de ese directorio.
