//! OpenClaw Gateway WebSocket client
//!
//! Implements OpenClaw's node protocol for GPU node integration.

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
use uuid::Uuid;

const PROTOCOL_VERSION: u32 = 1;
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// OpenClaw gateway frame types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GatewayFrame {
    Request(RequestFrame),
    Response(ResponseFrame),
    Event(EventFrame),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestFrame {
    pub id: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFrame {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorShape>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFrame {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorShape {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

/// Connect params sent on connection
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectParams {
    pub min_protocol: u32,
    pub max_protocol: u32,
    pub client: ClientInfo,
    pub caps: Vec<String>,
    pub commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthParams>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub platform: String,
    pub mode: String,
    pub instance_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// Node pair request params
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodePairRequestParams {
    pub node_id: String,
    pub display_name: String,
    pub platform: String,
    pub version: String,
    pub caps: Vec<String>,
    pub commands: Vec<String>,
    pub silent: bool,
}

/// Node invoke result params
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInvokeResultParams {
    pub id: String,
    pub node_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<InvokeError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvokeError {
    pub code: String,
    pub message: String,
}

/// Incoming node invoke request event
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeInvokeRequestEvent {
    pub id: String,
    pub node_id: String,
    pub command: String,
    #[serde(default)]
    pub params_json: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// Gateway WebSocket client
pub struct GatewayClient {
    state: SharedState,
    outgoing_tx: Option<mpsc::Sender<RequestFrame>>,
}

impl GatewayClient {
    pub fn new(state: SharedState) -> Self {
        Self {
            state,
            outgoing_tx: None,
        }
    }

    /// Connect to gateway and run the event loop
    pub async fn connect(&mut self, gateway_url: &str, auth_token: Option<&str>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = Url::parse(gateway_url)?;
        info!("connecting to gateway: {}", url);

        // Connect WebSocket
        let (ws_stream, _) = timeout(Duration::from_secs(10), connect_async(url.as_str()))
            .await
            .map_err(|_| "connection timeout")??;

        let (mut write, mut read) = ws_stream.split();
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<RequestFrame>(32);
        self.outgoing_tx = Some(outgoing_tx.clone());

        // Generate node identity
        let node_id = self.generate_node_id();
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let platform = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        
        let caps = self.state.capabilities.clone();
        let commands = self.state.commands.clone();

        // Send connect frame
        let connect_params = ConnectParams {
            min_protocol: PROTOCOL_VERSION,
            max_protocol: PROTOCOL_VERSION,
            client: ClientInfo {
                id: "clawnode".to_string(),
                display_name: hostname.clone(),
                version: CLIENT_VERSION.to_string(),
                platform: platform.clone(),
                mode: "node".to_string(),
                instance_id: Uuid::new_v4().to_string(),
            },
            caps: caps.clone(),
            commands: commands.clone(),
            auth: auth_token.map(|t| AuthParams { token: Some(t.to_string()) }),
        };

        let connect_frame = json!({
            "connect": connect_params,
        });
        
        debug!("sending connect frame");
        write.send(Message::Text(connect_frame.to_string().into())).await?;

        // Wait for response (could be hello, challenge, or error)
        let response = timeout(Duration::from_secs(10), read.next())
            .await
            .map_err(|_| "hello timeout")?
            .ok_or("connection closed")??;

        if let Message::Text(text) = response {
            let frame: Value = serde_json::from_str(&text)?;
            debug!("received initial frame: {}", text);
            
            // Handle different response types
            if frame.get("hello").is_some() {
                info!("connected to gateway, received hello");
            } else if let Some(event) = frame.get("event").and_then(|e| e.as_str()) {
                // Gateway may send a challenge event
                if event == "connect.challenge" {
                    debug!("received connect.challenge, continuing with pairing");
                    // We don't have device signing, so we'll just proceed
                } else {
                    debug!("received event: {}", event);
                }
            } else if let Some(error) = frame.get("error") {
                let msg = error.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                return Err(format!("gateway error: {}", msg).into());
            } else {
                debug!("received frame: {}", text);
            }
        }

        // Request pairing
        let pair_request = NodePairRequestParams {
            node_id: node_id.clone(),
            display_name: hostname.clone(),
            platform: platform.clone(),
            version: CLIENT_VERSION.to_string(),
            caps,
            commands,
            silent: true, // Auto-approve for GPU nodes
        };

        let request_id = Uuid::new_v4().to_string();
        let pair_frame = RequestFrame {
            id: request_id.clone(),
            method: "node.pair.request".to_string(),
            params: Some(serde_json::to_value(&pair_request)?),
        };

        debug!("sending node.pair.request");
        write.send(Message::Text(serde_json::to_string(&pair_frame)?.into())).await?;

        // Wait for pair response (with timeout but don't fail if we get events)
        let mut paired = false;
        let pair_timeout = tokio::time::Instant::now() + Duration::from_secs(10);
        
        while tokio::time::Instant::now() < pair_timeout && !paired {
            match timeout(Duration::from_secs(2), read.next()).await {
                Ok(Some(Ok(Message::Text(text)))) => {
                    let frame: Value = serde_json::from_str(&text)?;
                    debug!("received during pairing: {}", text);
                    
                    // Check for response to our pair request
                    if frame.get("id").is_some() {
                        if let Some(result) = frame.get("result") {
                            if let Some(token) = result.get("token").and_then(|t| t.as_str()) {
                                info!("node paired successfully, token received");
                                self.state.node_token = Some(token.to_string());
                            }
                            paired = true;
                        } else if let Some(error) = frame.get("error") {
                            let msg = error.get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("pairing failed");
                            // If already paired or similar, continue
                            if msg.contains("already") || msg.contains("exists") {
                                info!("node already registered");
                                paired = true;
                            } else {
                                warn!("pairing error: {}", msg);
                                // Continue anyway for now
                                paired = true;
                            }
                        }
                    }
                    // Ignore other events during pairing
                }
                Ok(Some(Ok(Message::Ping(data)))) => {
                    let _ = write.send(Message::Pong(data)).await;
                }
                Ok(Some(Ok(Message::Close(_)))) => {
                    return Err("connection closed during pairing".into());
                }
                Ok(Some(Err(e))) => {
                    return Err(format!("websocket error: {}", e).into());
                }
                Ok(None) => {
                    return Err("connection closed".into());
                }
                Err(_) => {
                    // Timeout waiting for response, assume pairing succeeded
                    debug!("pairing response timeout, continuing");
                    paired = true;
                }
                _ => {}
            }
        }

        info!("node registered as {} ({})", hostname, node_id);

        // Main event loop
        let mut heartbeat_interval = interval(Duration::from_secs(30));
        let node_id_clone = node_id.clone();

        loop {
            tokio::select! {
                // Send outgoing messages
                Some(frame) = outgoing_rx.recv() => {
                    let json = serde_json::to_string(&frame)?;
                    debug!("sending: {}", json);
                    if let Err(e) = write.send(Message::Text(json.into())).await {
                        error!("send error: {}", e);
                        break;
                    }
                }

                // Handle incoming messages
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = self.handle_message(&text, &node_id_clone, &outgoing_tx).await {
                                error!("message handling error: {}", e);
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = write.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("gateway closed connection");
                            break;
                        }
                        Some(Err(e)) => {
                            error!("websocket error: {}", e);
                            break;
                        }
                        None => {
                            info!("connection closed");
                            break;
                        }
                        _ => {}
                    }
                }

                // Send heartbeat
                _ = heartbeat_interval.tick() => {
                    let heartbeat = RequestFrame {
                        id: Uuid::new_v4().to_string(),
                        method: "node.event".to_string(),
                        params: Some(json!({
                            "event": "heartbeat",
                            "payload": {
                                "nodeId": node_id_clone,
                            }
                        })),
                    };
                    if let Err(e) = outgoing_tx.send(heartbeat).await {
                        error!("heartbeat send error: {}", e);
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_message(
        &self,
        text: &str,
        node_id: &str,
        outgoing_tx: &mpsc::Sender<RequestFrame>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let frame: Value = serde_json::from_str(text)?;
        debug!("received: {}", text);

        // Handle event frames
        if let Some(event) = frame.get("event").and_then(|e| e.as_str()) {
            match event {
                "node.invoke.request" => {
                    if let Some(payload) = frame.get("payload") {
                        let invoke: NodeInvokeRequestEvent = serde_json::from_value(payload.clone())?;
                        self.handle_invoke(invoke, node_id, outgoing_tx).await?;
                    }
                }
                "tick" => {
                    // Gateway tick, ignore
                }
                _ => {
                    debug!("unhandled event: {}", event);
                }
            }
        }

        // Handle response frames (for our requests)
        if frame.get("id").is_some() && frame.get("result").is_some() {
            // Response to one of our requests
            debug!("received response");
        }

        // Handle error frames
        if let Some(error) = frame.get("error") {
            let msg = error.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
            warn!("gateway error: {}", msg);
        }

        Ok(())
    }

    async fn handle_invoke(
        &self,
        invoke: NodeInvokeRequestEvent,
        node_id: &str,
        outgoing_tx: &mpsc::Sender<RequestFrame>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("invoke request: {} (id={})", invoke.command, invoke.id);

        // Parse params
        let params: Value = invoke.params_json
            .as_ref()
            .map(|s| serde_json::from_str(s).unwrap_or(Value::Null))
            .unwrap_or(Value::Null);

        // Execute command
        let request = CommandRequest {
            command: invoke.command.clone(),
            params,
        };

        let result = handle_command(&self.state, request).await;

        // Send result back
        let result_params = match result {
            Ok(payload) => NodeInvokeResultParams {
                id: invoke.id,
                node_id: node_id.to_string(),
                ok: true,
                payload: Some(payload),
                payload_json: None,
                error: None,
            },
            Err(e) => NodeInvokeResultParams {
                id: invoke.id,
                node_id: node_id.to_string(),
                ok: false,
                payload: None,
                payload_json: None,
                error: Some(InvokeError {
                    code: "COMMAND_ERROR".to_string(),
                    message: e.to_string(),
                }),
            },
        };

        let response = RequestFrame {
            id: Uuid::new_v4().to_string(),
            method: "node.invoke.result".to_string(),
            params: Some(serde_json::to_value(&result_params)?),
        };

        outgoing_tx.send(response).await?;
        Ok(())
    }

    fn generate_node_id(&self) -> String {
        // Generate a stable node ID based on machine characteristics
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        
        // Use hostname
        if let Ok(hostname) = hostname::get() {
            hostname.to_string_lossy().hash(&mut hasher);
        }
        
        // Use MAC address if available (via machine-uid or similar)
        if let Ok(uid) = std::fs::read_to_string("/etc/machine-id") {
            uid.trim().hash(&mut hasher);
        } else if let Ok(uid) = std::fs::read_to_string("/var/lib/dbus/machine-id") {
            uid.trim().hash(&mut hasher);
        }

        format!("{:016x}", hasher.finish())
    }
}

/// Send a command result back to the gateway
pub async fn send_result(
    tx: &mpsc::Sender<RequestFrame>,
    invoke_id: &str,
    node_id: &str,
    success: bool,
    payload: Value,
    error: Option<String>,
) -> Result<(), mpsc::error::SendError<RequestFrame>> {
    let result_params = NodeInvokeResultParams {
        id: invoke_id.to_string(),
        node_id: node_id.to_string(),
        ok: success,
        payload: if success { Some(payload) } else { None },
        payload_json: None,
        error: error.map(|msg| InvokeError {
            code: "ERROR".to_string(),
            message: msg,
        }),
    };

    let frame = RequestFrame {
        id: Uuid::new_v4().to_string(),
        method: "node.invoke.result".to_string(),
        params: Some(serde_json::to_value(&result_params).unwrap()),
    };

    tx.send(frame).await
}
