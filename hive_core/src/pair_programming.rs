//! Pair programming interactivo vía WebSocket.
//!
//! Permite al humano ver el progreso del enjambre en tiempo real y
//! tomar decisiones durante el ciclo (aprobar/rechazar MRs, intervenir).
//! Inspirado en Cursor y Windsurf que ofrecen esta capacidad.

#[cfg(feature = "multiagent")]
use anyhow::Result;
#[cfg(feature = "multiagent")]
use futures_util::{SinkExt, StreamExt};
#[cfg(feature = "multiagent")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "multiagent")]
use tokio::net::TcpListener;
#[cfg(feature = "multiagent")]
use tokio_tungstenite::accept_async;
#[cfg(feature = "multiagent")]
use tracing::info;

/// Evento enviado al cliente durante pair programming.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairEvent {
    pub event_type: PairEventType,
    pub timestamp: i64,
    pub worker_id: Option<String>,
    pub specialist: Option<String>,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// Tipo de evento de pair programming.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PairEventType {
    /// Ciclo iniciado.
    CycleStarted,
    /// Obrera inició tarea.
    WorkerStarted,
    /// Obrera creó archivo.
    FileCreated,
    /// Obrera modificó archivo.
    FileModified,
    /// MR creado.
    MergeRequestCreated,
    /// Consejo revisando.
    CouncilReviewing,
    /// Consejo aprobó.
    CouncilApproved,
    /// Consejo rechazó.
    CouncilRejected,
    /// Merge realizado.
    Merged,
    /// Error en obrera.
    WorkerError,
    /// Ciclo completado.
    CycleCompleted,
    /// Petición de intervención humana.
    HumanInterventionNeeded,
}

/// Comando enviado por el humano durante pair programming.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanCommand {
    pub command_type: HumanCommandType,
    pub target_worker: Option<String>,
    pub message: Option<String>,
    pub data: Option<serde_json::Value>,
}

/// Tipo de comando humano.
#[cfg(feature = "multiagent")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HumanCommandType {
    /// Aprobar MR manualmente.
    Approve,
    /// Rechazar MR con feedback.
    Reject,
    /// Pausar una obrera.
    PauseWorker,
    /// Reanudar una obrera.
    ResumeWorker,
    /// Cancelar una tarea.
    CancelTask,
    /// Instrucción libre a una obrera.
    Instruct,
    /// Solicitar estado actual.
    StatusRequest,
}

/// Servidor WebSocket para pair programming.
#[cfg(feature = "multiagent")]
pub struct PairProgrammingServer {
    port: u16,
    #[allow(clippy::type_complexity)]
    event_tx: tokio::sync::broadcast::Sender<PairEvent>,
    command_rx: tokio::sync::mpsc::Receiver<HumanCommand>,
    command_tx: tokio::sync::mpsc::Sender<HumanCommand>,
}

#[cfg(feature = "multiagent")]
impl PairProgrammingServer {
    pub fn new(port: u16) -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        let (command_tx, command_rx) = tokio::sync::mpsc::channel(64);
        Self {
            port,
            event_tx,
            command_rx,
            command_tx,
        }
    }

    /// Emite un evento a todos los clientes conectados.
    pub fn emit(&self, event: PairEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Devuelve un sender para comandos (para uso externo).
    pub fn command_sender(&self) -> tokio::sync::mpsc::Sender<HumanCommand> {
        self.command_tx.clone()
    }

    /// Intenta recibir un comando humano (no bloqueante).
    pub fn try_recv_command(&mut self) -> Option<HumanCommand> {
        self.command_rx.try_recv().ok()
    }

    /// Inicia el servidor WebSocket.
    pub async fn start(&self) -> Result<()> {
        let addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&addr).await?;
        info!(addr = %addr, "servidor pair programming WebSocket escuchando");

        let event_tx = self.event_tx.clone();
        let command_tx = self.command_tx.clone();

        loop {
            let (stream, peer) = listener.accept().await?;
            info!(peer = %peer, "cliente pair programming conectado");

            let mut event_rx = event_tx.subscribe();
            let cmd_tx = command_tx.clone();

            tokio::spawn(async move {
                let ws = match accept_async(stream).await {
                    Ok(ws) => ws,
                    Err(e) => {
                        info!(error = %e, "error aceptando WebSocket");
                        return;
                    }
                };

                let (mut ws_sink, mut ws_stream) = ws.split();

                // Tarea para enviar eventos al cliente
                let send_task = tokio::spawn(async move {
                    while let Ok(event) = event_rx.recv().await {
                        let json = serde_json::to_string(&event).unwrap_or_default();
                        if ws_sink
                            .send(tokio_tungstenite::tungstenite::Message::Text(json.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });

                // Tarea para recibir comandos del cliente y reenviarlos al command_tx
                let recv_task = tokio::spawn(async move {
                    while let Some(Ok(msg)) = ws_stream.next().await {
                        if let tokio_tungstenite::tungstenite::Message::Text(text) = msg {
                            match serde_json::from_str::<HumanCommand>(&text) {
                                Ok(cmd) => {
                                    info!(?cmd, "comando humano recibido vía WebSocket");
                                    let _ = cmd_tx.send(cmd).await;
                                }
                                Err(e) => {
                                    info!(error = %e, "comando WS inválido, ignorando");
                                }
                            }
                        }
                    }
                });

                let _ = tokio::join!(send_task, recv_task);
            });
        }
    }
}

// Stubs sin la feature
#[cfg(not(feature = "multiagent"))]
pub struct PairProgrammingServer;

#[cfg(not(feature = "multiagent"))]
impl PairProgrammingServer {
    pub fn new(_port: u16) -> Self {
        Self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "multiagent")]
    #[test]
    fn pair_event_serializable() {
        let event = PairEvent {
            event_type: PairEventType::CycleStarted,
            timestamp: chrono::Utc::now().timestamp(),
            worker_id: None,
            specialist: None,
            message: "Ciclo iniciado".into(),
            data: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("CycleStarted"));
    }

    #[cfg(feature = "multiagent")]
    #[test]
    fn human_command_serializable() {
        let cmd = HumanCommand {
            command_type: HumanCommandType::Approve,
            target_worker: Some("worker-1".into()),
            message: Some("Looks good!".into()),
            data: None,
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("Approve"));
    }
}
