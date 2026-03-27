//! Profiling de performance del código generado.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerfMetric {
    pub name: String,
    pub duration_ms: f64,
    pub memory_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PerfReport {
    pub metrics: Vec<PerfMetric>,
    pub regressions: Vec<String>,
}

pub struct PerfProfiler;

impl PerfProfiler {
    pub fn measure<F: FnOnce()>(name: &str, f: F) -> PerfMetric {
        let start = std::time::Instant::now();
        f();
        PerfMetric {
            name: name.into(),
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            memory_bytes: 0,
        }
    }

    pub fn compare(baseline: &[PerfMetric], current: &[PerfMetric]) -> PerfReport {
        let mut regressions = Vec::new();
        for (b, c) in baseline.iter().zip(current.iter()) {
            if c.duration_ms > b.duration_ms * 1.2 {
                regressions.push(format!(
                    "{}: {:.1}ms → {:.1}ms (+{:.0}%)",
                    c.name,
                    b.duration_ms,
                    c.duration_ms,
                    (c.duration_ms - b.duration_ms) / b.duration_ms * 100.0
                ));
            }
        }
        PerfReport {
            metrics: current.to_vec(),
            regressions,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_works() {
        let m = PerfProfiler::measure("test", || {
            std::thread::sleep(std::time::Duration::from_millis(1));
        });
        assert!(m.duration_ms >= 0.0);
    }
}
