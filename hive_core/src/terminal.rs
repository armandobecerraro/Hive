//! Terminal interactivo para obreras.
//!
//! Permite a las obreras interactuar con REPLs, procesos largos y debugging.
//! Inspirado en Devin que tiene un terminal persistente.

use serde::{Deserialize, Serialize};

/// Resultado de ejecución de terminal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub timed_out: bool,
}

/// Terminal persistente que mantiene estado entre comandos.
pub struct InteractiveTerminal {
    cwd: String,
    env: std::collections::HashMap<String, String>,
    history: Vec<String>,
    timeout_secs: u64,
}

impl InteractiveTerminal {
    pub fn new(cwd: &str, timeout_secs: u64) -> Self {
        Self {
            cwd: cwd.to_string(),
            env: std::collections::HashMap::new(),
            history: Vec::new(),
            timeout_secs,
        }
    }

    /// Ejecuta un comando manteniendo el cwd.
    pub fn execute(&mut self, command: &str) -> TerminalResult {
        self.history.push(command.to_string());
        let start = std::time::Instant::now();

        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(command).current_dir(&self.cwd);

        for (k, v) in &self.env {
            cmd.env(k, v);
        }

        let output = match tokio::runtime::Runtime::new() {
            Ok(rt) => {
                match rt.block_on(async {
                    tokio::time::timeout(std::time::Duration::from_secs(self.timeout_secs), async {
                        cmd.output()
                    })
                    .await
                }) {
                    Ok(Ok(out)) => out,
                    Ok(Err(e)) => {
                        return TerminalResult {
                            exit_code: None,
                            stdout: String::new(),
                            stderr: e.to_string(),
                            duration_ms: start.elapsed().as_millis() as u64,
                            timed_out: false,
                        };
                    }
                    Err(_) => {
                        return TerminalResult {
                            exit_code: None,
                            stdout: String::new(),
                            stderr: "timeout".into(),
                            duration_ms: start.elapsed().as_millis() as u64,
                            timed_out: true,
                        };
                    }
                }
            }
            Err(e) => {
                return TerminalResult {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: e.to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    timed_out: false,
                };
            }
        };

        TerminalResult {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms: start.elapsed().as_millis() as u64,
            timed_out: false,
        }
    }

    /// Cambia el directorio de trabajo.
    pub fn cd(&mut self, dir: &str) {
        if dir.starts_with('/') {
            self.cwd = dir.to_string();
        } else {
            self.cwd = format!("{}/{dir}", self.cwd);
        }
    }

    /// Establece una variable de entorno.
    pub fn set_env(&mut self, key: &str, value: &str) {
        self.env.insert(key.to_string(), value.to_string());
    }

    /// Obtiene el historial de comandos.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Obtiene el cwd actual.
    pub fn cwd(&self) -> &str {
        &self.cwd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_executes_echo() {
        let mut term = InteractiveTerminal::new("/tmp", 10);
        let result = term.execute("echo hello");
        assert_eq!(result.exit_code, Some(0));
        assert!(result.stdout.contains("hello"));
    }

    #[test]
    fn terminal_tracks_history() {
        let mut term = InteractiveTerminal::new("/tmp", 10);
        term.execute("echo 1");
        term.execute("echo 2");
        assert_eq!(term.history().len(), 2);
    }

    #[test]
    fn terminal_set_env() {
        let mut term = InteractiveTerminal::new("/tmp", 10);
        term.set_env("TEST_VAR", "hello");
        let result = term.execute("echo $TEST_VAR");
        assert!(result.stdout.contains("hello"));
    }
}
