use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "macos")]
use std::str;

/// Monitors system resources to determine if new agents can be spawned
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMonitor {
    max_agents: usize,
    memory_threshold_mb: u64,
    cpu_threshold_percent: f32,
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceMonitor {
    /// Creates a new ResourceMonitor with default thresholds
    pub fn new() -> Self {
        Self {
            max_agents: 10,
            memory_threshold_mb: 1024, // 1GB
            cpu_threshold_percent: 80.0,
        }
    }

    /// Checks if a new agent can be spawned based on system resources
    pub fn can_spawn_agent(&self) -> Result<bool, Box<dyn std::error::Error>> {
        // Check memory usage
        let memory_usage = self.get_memory_usage()?;
        let memory_percent = (memory_usage.used as f64 / memory_usage.total as f64) * 100.0;

        // Check CPU usage
        let cpu_usage = self.get_cpu_usage()?;

        // Check if we're below thresholds
        let memory_ok = memory_percent < 80.0; // Less than 80% memory used
        let cpu_ok = cpu_usage < self.cpu_threshold_percent;

        // In a real implementation, you'd also check the number of active agents
        // For now, we'll just check system resources

        Ok(memory_ok && cpu_ok)
    }

    /// Gets current memory usage
    fn get_memory_usage(&self) -> Result<MemoryInfo, Box<dyn std::error::Error>> {
        #[cfg(target_os = "macos")]
        {
            // Use vm_stat on macOS
            let output = Command::new("vm_stat").output()?.stdout;
            let output_str = str::from_utf8(&output)?;

            // Parse vm_stat output (simplified)
            let pages_free = self.parse_vm_stat_value(output_str, "Pages free:");
            let pages_active = self.parse_vm_stat_value(output_str, "Pages active:");
            let pages_inactive = self.parse_vm_stat_value(output_str, "Pages inactive:");
            let pages_speculative = self.parse_vm_stat_value(output_str, "Pages speculative:");
            let pages_wired = self.parse_vm_stat_value(output_str, "Pages wired down:");

            // Page size is 4096 bytes on macOS
            let page_size = 4096;

            let free = pages_free * page_size;
            let used =
                (pages_active + pages_inactive + pages_speculative + pages_wired) * page_size;
            let total = free + used;

            Ok(MemoryInfo { total, used, free })
        }

        #[cfg(not(target_os = "macos"))]
        {
            // Fallback for other systems
            Ok(MemoryInfo {
                total: 16 * 1024 * 1024 * 1024, // 16GB
                used: 8 * 1024 * 1024 * 1024,   // 8GB
                free: 8 * 1024 * 1024 * 1024,   // 8GB
            })
        }
    }

    /// Parses a value from vm_stat output (macOS); también cubierto por tests en Linux.
    #[allow(dead_code)] // solo se llama desde ramas `macos` y desde `#[cfg(test)]`
    fn parse_vm_stat_value(&self, output: &str, key: &str) -> u64 {
        output
            .lines()
            .find(|line| line.contains(key))
            .and_then(|line| {
                line.split(':')
                    .nth(1)
                    .and_then(|val| val.trim().split('.').next())
                    .and_then(|val| val.trim().parse::<u64>().ok())
            })
            .unwrap_or(0)
    }

    /// Gets current CPU usage percentage
    fn get_cpu_usage(&self) -> Result<f32, Box<dyn std::error::Error>> {
        #[cfg(target_os = "macos")]
        {
            // Use top command on macOS
            let output = Command::new("top")
                .args(["-l", "1", "-n", "0"])
                .output()?
                .stdout;
            let output_str = str::from_utf8(&output)?;

            // Parse CPU usage from top output
            for line in output_str.lines() {
                if line.contains("CPU usage:") {
                    let parts: Vec<&str> = line.split(':').collect();
                    if parts.len() > 1 {
                        let cpu_str = parts[1];
                        // Extract user percentage
                        if let Some(user_part) = cpu_str.split('%').next() {
                            if let Some(num_str) = user_part.split(' ').next_back() {
                                if let Ok(usage) = num_str.parse::<f32>() {
                                    return Ok(usage);
                                }
                            }
                        }
                    }
                }
            }

            Ok(50.0) // Default fallback
        }

        #[cfg(not(target_os = "macos"))]
        {
            // Fallback for other systems
            Ok(50.0)
        }
    }

    /// Gets the recommended maximum number of agents based on system resources
    pub fn get_recommended_max_agents(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let memory_info = self.get_memory_usage()?;
        let available_memory_mb = memory_info.free / (1024 * 1024);

        // Each agent needs approximately 100MB
        let agents_by_memory = (available_memory_mb / 100) as usize;

        // Limit by CPU
        let cpu_usage = self.get_cpu_usage()?;
        let cpu_available = 100.0 - cpu_usage;
        let agents_by_cpu = (cpu_available / 10.0) as usize; // Each agent uses ~10% CPU

        // Take the minimum of the two
        let recommended = std::cmp::min(agents_by_memory, agents_by_cpu);

        // Ensure at least 1 agent can be spawned
        Ok(std::cmp::max(
            1,
            std::cmp::min(recommended, self.max_agents),
        ))
    }
}

/// Memory information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MemoryInfo {
    total: u64,
    used: u64,
    free: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_monitor_creation() {
        let monitor = ResourceMonitor::new();
        assert_eq!(monitor.max_agents, 10);
        assert_eq!(monitor.memory_threshold_mb, 1024);
        assert_eq!(monitor.cpu_threshold_percent, 80.0);
    }

    #[test]
    fn test_can_spawn_agent_simulation() {
        let monitor = ResourceMonitor::new();
        // This test just ensures the function doesn't panic
        let result = monitor.can_spawn_agent();
        assert!(result.is_ok());
    }

    #[test]
    fn parse_vm_stat_value_extracts_number() {
        let m = ResourceMonitor::new();
        let s = "Pages free:     12345.\nPages active:   100.\n";
        assert_eq!(m.parse_vm_stat_value(s, "Pages free:"), 12345);
        assert_eq!(m.parse_vm_stat_value(s, "missing:"), 0);
    }

    #[test]
    fn recommended_max_agents_nonzero() {
        let m = ResourceMonitor::new();
        let n = m.get_recommended_max_agents().expect("ok");
        assert!(n >= 1);
        assert!(n <= 10);
    }
}
