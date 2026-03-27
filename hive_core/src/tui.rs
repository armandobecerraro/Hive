//! TUI (Terminal UI) con paneles, progreso y colores.
//! Inspirado en ratatui e indicatif.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiProgress {
    pub task: String,
    pub percent: u8,
    pub eta_secs: Option<u64>,
    pub status: ProgressStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ProgressStatus {
    Running,
    Done,
    Failed,
    Pending,
}

pub struct TuiRenderer;

impl TuiRenderer {
    pub fn render_progress(progress: &[TuiProgress]) -> String {
        let mut out = String::from("╔══════════════════════════════════════════════════╗\n");
        out.push_str("║           THE HIVE — PROGRESS                   ║\n");
        out.push_str("╠══════════════════════════════════════════════════╣\n");
        for p in progress {
            let bar_len = 20;
            let filled = (p.percent as usize * bar_len) / 100;
            let bar: String = "█".repeat(filled) + &"░".repeat(bar_len - filled);
            let icon = match p.status {
                ProgressStatus::Running => "🔄",
                ProgressStatus::Done => "✅",
                ProgressStatus::Failed => "❌",
                ProgressStatus::Pending => "⏳",
            };
            let eta = p.eta_secs.map(|s| format!("{s}s")).unwrap_or("--".into());
            out.push_str(&format!(
                "║ {icon} {:.<20} [{bar}] {:>3}% ETA: {}\n",
                p.task, p.percent, eta
            ));
        }
        out.push_str("╚══════════════════════════════════════════════════╝\n");
        out
    }

    pub fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
        let widths: Vec<usize> = headers
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let max_data = rows
                    .iter()
                    .map(|r| r.get(i).map(|s| s.len()).unwrap_or(0))
                    .max()
                    .unwrap_or(0);
                h.len().max(max_data).max(8)
            })
            .collect();

        let mut out = String::new();
        // Header
        out.push('┌');
        for (i, w) in widths.iter().enumerate() {
            out.push_str(&"─".repeat(w + 2));
            out.push(if i < widths.len() - 1 { '┬' } else { '┐' });
        }
        out.push('\n');
        out.push('│');
        for (i, h) in headers.iter().enumerate() {
            out.push_str(&format!(" {:<width$} │", h, width = widths[i]));
        }
        out.push('\n');
        out.push('├');
        for (i, w) in widths.iter().enumerate() {
            out.push_str(&"─".repeat(w + 2));
            out.push(if i < widths.len() - 1 { '┼' } else { '┤' });
        }
        out.push('\n');
        // Rows
        for row in rows {
            out.push('│');
            for (i, w) in widths.iter().enumerate() {
                let cell = row.get(i).map(|s| s.as_str()).unwrap_or("");
                out.push_str(&format!(" {:<width$} │", cell, width = w));
            }
            out.push('\n');
        }
        out.push('└');
        for (i, w) in widths.iter().enumerate() {
            out.push_str(&"─".repeat(w + 2));
            out.push(if i < widths.len() - 1 { '┴' } else { '┘' });
        }
        out.push('\n');
        out
    }

    pub fn render_spinner(message: &str) -> String {
        format!("⠋ {message}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_progress_works() {
        let progress = vec![
            TuiProgress {
                task: "Analysis".into(),
                percent: 100,
                eta_secs: None,
                status: ProgressStatus::Done,
            },
            TuiProgress {
                task: "Worker Rust".into(),
                percent: 50,
                eta_secs: Some(30),
                status: ProgressStatus::Running,
            },
        ];
        let output = TuiRenderer::render_progress(&progress);
        assert!(output.contains("THE HIVE"));
        assert!(output.contains("✅"));
        assert!(output.contains("🔄"));
    }

    #[test]
    fn render_table_works() {
        let headers = ["Module", "Tests", "Status"];
        let rows = vec![
            vec!["agent".into(), "10".into(), "ok".into()],
            vec!["council".into(), "8".into(), "ok".into()],
        ];
        let table = TuiRenderer::render_table(&headers, &rows);
        assert!(table.contains("Module"));
        assert!(table.contains("agent"));
    }
}
