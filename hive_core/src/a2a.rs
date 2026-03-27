//! Agent-to-Agent (A2A) Protocol — comunicación entre agentes.
//!
//! Implementa el protocolo A2A de Google para descubrimiento y colaboración entre agentes.
//! Cada obrera expone un AgentCard que describe sus capacidades.

#[cfg(feature = "multiagent")]
use anyhow::Result;
#[cfg(feature = "multiagent")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "multiagent")]
use std::collections::HashMap;
#[cfg(feature = "multiagent")]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(feature = "multiagent")]
use tokio::net::TcpListener;
#[cfg(feature = "multiagent")]
use tracing::info;
#[cfg(feature = "multiagent")]
use uuid::Uuid;

/// AgentCard: describe las capacidades de un agente (formato A2A).
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    /// Nombre del agente.
    pub name: String,
    /// Descripción de lo que hace.
    pub description: String,
    /// URL donde está disponible.
    pub url: String,
    /// Versión del protocolo A2A.
    pub protocol_version: String,
    /// Capacidades que ofrece.
    pub capabilities: Vec<AgentCapability>,
    /// Idiomas/tecnologías que maneja.
    pub skills: Vec<String>,
    /// Estado actual.
    pub status: AgentStatus,
    /// Endpoint de salud.
    pub health_endpoint: Option<String>,
}

/// Capacidad individual de un agente.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapability {
    pub name: String,
    pub description: String,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
}

/// Estado del agente.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentStatus {
    Available,
    Busy,
    Offline,
}

/// Mensaje entre agentes (A2A message).
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2AMessage {
    pub id: Uuid,
    pub from: String,
    pub to: String,
    pub message_type: A2AMessageType,
    pub payload: serde_json::Value,
    pub timestamp: i64,
}

/// Tipo de mensaje A2A.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum A2AMessageType {
    /// Solicitar ayuda a otro agente.
    Request,
    /// Respuesta a una solicitud.
    Response,
    /// Notificación sin respuesta esperada.
    Notification,
    /// Descubrimiento de agentes.
    Discovery,
    /// Delegar tarea.
    Delegation,
}

/// Registro local de agentes descubiertos.
#[cfg(feature = "multiagent")]
pub struct AgentRegistry {
    agents: HashMap<String, AgentCard>,
    local_card: AgentCard,
}

#[cfg(feature = "multiagent")]
impl AgentRegistry {
    pub fn new(local_card: AgentCard) -> Self {
        Self {
            agents: HashMap::new(),
            local_card,
        }
    }

    /// Registra un agente remoto.
    pub fn register(&mut self, card: AgentCard) {
        info!(name = %card.name, url = %card.url, "agente registrado");
        self.agents.insert(card.name.clone(), card);
    }

    /// Elimina un agente por nombre.
    pub fn unregister(&mut self, name: &str) {
        self.agents.remove(name);
    }

    /// Busca agentes por skill.
    pub fn find_by_skill(&self, skill: &str) -> Vec<&AgentCard> {
        self.agents
            .values()
            .filter(|a| a.skills.iter().any(|s| s.contains(skill)))
            .collect()
    }

    /// Busca agentes por capability.
    pub fn find_by_capability(&self, capability: &str) -> Vec<&AgentCard> {
        self.agents
            .values()
            .filter(|a| a.capabilities.iter().any(|c| c.name.contains(capability)))
            .collect()
    }

    /// Lista todos los agentes disponibles.
    pub fn list_available(&self) -> Vec<&AgentCard> {
        self.agents
            .values()
            .filter(|a| a.status == AgentStatus::Available)
            .collect()
    }

    /// Devuelve el card local.
    pub fn local_card(&self) -> &AgentCard {
        &self.local_card
    }

    /// Serializa el card local como JSON.
    pub fn local_card_json(&self) -> String {
        serde_json::to_string_pretty(&self.local_card).unwrap_or_default()
    }
}

/// Servidor A2A que escucha mensajes de otros agentes.
#[cfg(feature = "multiagent")]
pub struct A2AServer {
    registry: AgentRegistry,
    port: u16,
}

#[cfg(feature = "multiagent")]
impl A2AServer {
    pub fn new(registry: AgentRegistry, port: u16) -> Self {
        Self { registry, port }
    }

    /// Inicia el servidor A2A (escucha TCP).
    pub async fn start(&mut self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!(addr = %addr, "servidor A2A escuchando");

        loop {
            let (mut socket, peer) = listener.accept().await?;
            info!(peer = %peer, "conexión A2A entrante");

            let registry_json = self.registry.local_card_json();

            tokio::spawn(async move {
                let mut buf = vec![0u8; 4096];
                match socket.read(&mut buf).await {
                    Ok(0) => (),
                    Ok(n) => {
                        let request = String::from_utf8_lossy(&buf[..n]);
                        let response = if request.contains("Discovery") {
                            registry_json.clone()
                        } else {
                            serde_json::json!({
                                "status": "received",
                                "message": "A2A message processed"
                            })
                            .to_string()
                        };
                        let _ = socket.write_all(response.as_bytes()).await;
                    }
                    Err(_) => {}
                }
            });
        }
    }
}

// Stubs sin la feature
#[cfg(not(feature = "multiagent"))]
pub struct AgentRegistry;

#[cfg(not(feature = "multiagent"))]
impl AgentRegistry {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "multiagent")]
    fn sample_card() -> AgentCard {
        AgentCard {
            name: "TestAgent".into(),
            description: "Agente de prueba".into(),
            url: "http://localhost:9100".into(),
            protocol_version: "A2A/1.0".into(),
            capabilities: vec![AgentCapability {
                name: "code_review".into(),
                description: "Revisar código".into(),
                input_schema: None,
                output_schema: None,
            }],
            skills: vec!["rust".into(), "python".into()],
            status: AgentStatus::Available,
            health_endpoint: Some("/health".into()),
        }
    }

    #[cfg(feature = "multiagent")]
    #[test]
    fn agent_registry_busqueda_por_skill() {
        let mut reg = AgentRegistry::new(sample_card());
        reg.register(sample_card());
        let found = reg.find_by_skill("rust");
        assert!(!found.is_empty());
    }

    #[cfg(feature = "multiagent")]
    #[test]
    fn agent_registry_busqueda_por_capability() {
        let mut reg = AgentRegistry::new(sample_card());
        reg.register(sample_card());
        let found = reg.find_by_capability("code_review");
        assert!(!found.is_empty());
    }

    #[cfg(feature = "multiagent")]
    #[test]
    fn agent_card_serializable() {
        let card = sample_card();
        let json = serde_json::to_string(&card).unwrap();
        let back: AgentCard = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "TestAgent");
    }
}
