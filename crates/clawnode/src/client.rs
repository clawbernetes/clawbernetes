//! Gateway WebSocket client
//!
//! Connects to OpenClaw gateway and handles bidirectional communication.

use crate::commands::{handle_command, CommandRequest};
use crate::SharedState;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use url::Url;

/// Messages sent to the gateway
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum OutgoingMessage {
    #[serde(rename = "node.register")]
    Register {
        #[serde(rename = "nodeId")]
        node_id: String,
        #[serde(rename = "displayName")]
        display_name: String,
        platform: String,
        version: String,
        caps: Vec<String>,
        commands: Vec<String>,
        token: Option<String>,
    },
    
    #[serde(rename = "node.heartbeat")]
    Heartbeat {
        #[serde(rename = "nodeId")]
        node_id: String,
    },
    
    #[serde(rename = "node.invoke.result")]
    InvokeResult {
        #[serde(rename = "invokeId")]
        invoke_id: String,
        success: bool,
        payload: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// Messages received from the gateway
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum IncomingMessage {
    #[serde(rename = "node.registered")]
    Registered {
        #[serde(rename = "nodeId")]
        node_id: String,
        approved: bool,
    },
    
    #[serde(rename = "node.approved")]
    Approved {
        #[serde(rename = "nodeId")]
        node_id: String,
    },
    
    #[serde(rename = "node.invoke")]
    Invoke {
        #[serde(rename = "invokeId")]
        invoke_id: String,
        command: String,
        params: Value,
        #[serde(rename = "idempotencyKey")]
        idempotency_key: Option<String>,
    },
    
    #[serde(rename = "node.heartbeat.ack")]
    HeartbeatAck {},
    
    #[serde(rename = "error")]
    Error {
        message: String,
    },
}

/// Gateway WebSocket client
pub struct GatewayClient {
    state: SharedState,
    outgoing_tx: Option<mpsc::Sender<OutgoingMessage>>,
}

impl GatewayClient {
    pub fn new(state: SharedState) -> Self {
        Self {
            state,
            outgoing_tx: None,
        }
    }
    
    /// Connect to the gateway and run the event loop
    pub async fn run(&mut self) -> anyhow::Result<()> {
        loop {
            match self.connect_and_run().await {
                Ok(()) => {
                    info!("connection closed gracefully");
                }
                Err(e) => {
                    error!(error = %e, "connection error");
                }
            }
            
            // Update state
            {
                let mut state = self.state.write().await;
                state.connected = false;
            }
            
            // Reconnect delay
            let delay = {
                let state = self.state.read().await;
                state.config.reconnect_delay_secs
            };
            
            info!(delay_secs = delay, "reconnecting...");
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
    }
    
    async fn connect_and_run(&mut self) -> anyhow::Result<()> {
        let (gateway_url, token, hostname) = {
            let state = self.state.read().await;
            (
                state.config.gateway.clone(),
                state.config.token.clone(),
                state.config.hostname.clone(),
            )
        };
        
        // Parse and modify URL for WebSocket node endpoint
        let mut url = Url::parse(&gateway_url)?;
        url.set_path("/ws/node");
        
        info!(url = %url, "connecting to gateway");
        
        let (ws_stream, _) = connect_async(url.as_str()).await?;
        let (mut write, mut read) = ws_stream.split();
        
        info!("connected to gateway");
        
        // Create channel for outgoing messages
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<OutgoingMessage>(32);
        self.outgoing_tx = Some(outgoing_tx.clone());
        
        // Register with gateway
        let (caps, commands) = {
            let state = self.state.read().await;
            (
                state.gpu_manager.capabilities(),
                state.gpu_manager.commands(),
            )
        };
        
        let register_msg = OutgoingMessage::Register {
            node_id: hostname.clone(),
            display_name: hostname.clone(),
            platform: std::env::consts::OS.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            caps,
            commands,
            token,
        };
        
        let json = serde_json::to_string(&register_msg)?;
        write.send(Message::Text(json.into())).await?;
        debug!("sent registration");
        
        // Update connected state
        {
            let mut state = self.state.write().await;
            state.connected = true;
        }
        
        // Set up heartbeat
        let heartbeat_interval = {
            let state = self.state.read().await;
            state.config.heartbeat_interval_secs
        };
        let mut heartbeat = interval(Duration::from_secs(heartbeat_interval));
        
        // Clone for heartbeat task
        let heartbeat_tx = outgoing_tx.clone();
        let heartbeat_hostname = hostname.clone();
        
        // Main event loop
        loop {
            tokio::select! {
                // Handle incoming messages
                Some(msg) = read.next() => {
                    match msg {
                        Ok(Message::Text(text)) => {
                            self.handle_incoming(&text, &outgoing_tx).await?;
                        }
                        Ok(Message::Ping(data)) => {
                            write.send(Message::Pong(data)).await?;
                        }
                        Ok(Message::Close(_)) => {
                            info!("received close frame");
                            break;
                        }
                        Err(e) => {
                            error!(error = %e, "websocket error");
                            break;
                        }
                        _ => {}
                    }
                }
                
                // Handle outgoing messages
                Some(msg) = outgoing_rx.recv() => {
                    let json = serde_json::to_string(&msg)?;
                    write.send(Message::Text(json.into())).await?;
                }
                
                // Send heartbeat
                _ = heartbeat.tick() => {
                    let msg = OutgoingMessage::Heartbeat {
                        node_id: heartbeat_hostname.clone(),
                    };
                    if heartbeat_tx.send(msg).await.is_err() {
                        break;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    async fn handle_incoming(
        &self,
        text: &str,
        outgoing_tx: &mpsc::Sender<OutgoingMessage>,
    ) -> anyhow::Result<()> {
        let msg: IncomingMessage = match serde_json::from_str(text) {
            Ok(m) => m,
            Err(e) => {
                warn!(error = %e, text = %text, "failed to parse incoming message");
                return Ok(());
            }
        };
        
        match msg {
            IncomingMessage::Registered { node_id, approved } => {
                info!(node_id = %node_id, approved = approved, "registered with gateway");
                
                let mut state = self.state.write().await;
                state.node_id = Some(node_id);
                state.approved = approved;
                
                if !approved {
                    warn!("node not yet approved - waiting for manual approval");
                }
            }
            
            IncomingMessage::Approved { node_id } => {
                info!(node_id = %node_id, "node approved");
                
                let mut state = self.state.write().await;
                state.approved = true;
            }
            
            IncomingMessage::Invoke {
                invoke_id,
                command,
                params,
                idempotency_key,
            } => {
                debug!(
                    invoke_id = %invoke_id,
                    command = %command,
                    "received invoke"
                );
                
                let request = CommandRequest {
                    command,
                    params,
                    idempotency_key,
                };
                
                let response = handle_command(self.state.clone(), request).await;
                
                let result_msg = OutgoingMessage::InvokeResult {
                    invoke_id,
                    success: response.success,
                    payload: response.payload,
                    error: response.error,
                };
                
                outgoing_tx.send(result_msg).await?;
            }
            
            IncomingMessage::HeartbeatAck {} => {
                debug!("heartbeat ack");
            }
            
            IncomingMessage::Error { message } => {
                error!(message = %message, "gateway error");
            }
        }
        
        Ok(())
    }
}
