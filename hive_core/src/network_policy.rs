//! Network policies para el sandbox Docker.
//! Controla qué conexiones pueden hacer los containers.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    pub allow_outbound: Vec<String>,
    pub deny_outbound: Vec<String>,
    pub allow_inbound: Vec<String>,
    pub network_mode: String,
    pub dns_servers: Vec<String>,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            allow_outbound: vec!["127.0.0.1".into()],
            deny_outbound: vec!["0.0.0.0/0".into()],
            allow_inbound: vec![],
            network_mode: "none".into(),
            dns_servers: vec![],
        }
    }
}

pub struct NetworkPolicyManager;

impl NetworkPolicyManager {
    pub fn restrictive() -> NetworkPolicy {
        NetworkPolicy {
            network_mode: "none".into(),
            ..Default::default()
        }
    }

    pub fn allow_localhost() -> NetworkPolicy {
        NetworkPolicy {
            network_mode: "bridge".into(),
            allow_outbound: vec!["127.0.0.1:*".into()],
            deny_outbound: vec![],
            ..Default::default()
        }
    }

    pub fn allow_api_access(hosts: &[&str]) -> NetworkPolicy {
        NetworkPolicy {
            network_mode: "bridge".into(),
            allow_outbound: hosts.iter().map(|h| h.to_string()).collect(),
            ..Default::default()
        }
    }

    pub fn to_docker_args(policy: &NetworkPolicy) -> Vec<String> {
        let mut args = vec!["--network".into(), policy.network_mode.clone()];
        for dns in &policy.dns_servers {
            args.push("--dns".into());
            args.push(dns.clone());
        }
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restrictive_blocks_all() {
        let p = NetworkPolicyManager::restrictive();
        assert_eq!(p.network_mode, "none");
    }

    #[test]
    fn to_docker_args_works() {
        let p = NetworkPolicyManager::restrictive();
        let args = NetworkPolicyManager::to_docker_args(&p);
        assert!(args.contains(&"none".to_string()));
    }
}
