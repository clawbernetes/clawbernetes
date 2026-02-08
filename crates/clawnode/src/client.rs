//! OpenClaw Gateway WebSocket client
//!
//! Implements OpenClaw's node protocol for GPU node integration.

use crate::commands::{handle_command, CommandRequest};
use crate::identity::{DeviceIdentity, DeviceParams};
use crate::SharedState;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{interval, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;

const PROTOCOL_VERSION: u32 = 3;  // Must match gateway's PROTOCOL_VERSION
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
    #[serde(rename = "type")]
    pub frame_type: String,  // Always "req"
    pub id: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl RequestFrame {
    pub fn new(id: String, method: String, params: Option<Value>) -> Self {
        Self {
            frame_type: "req".to_string(),
            id,
            method,
            params,
        }
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<DeviceParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    #[cfg(feature = "network")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mesh: Option<crate::mesh::MeshInfo>,
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
    #[serde(default, rename = "paramsJSON")]
    pub params_json: Option<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

/// Gateway WebSocket client
pub struct GatewayClient {
    state: SharedState,
    identity: DeviceIdentity,
    outgoing_tx: Option<mpsc::Sender<RequestFrame>>,
}

impl GatewayClient {
    pub fn new(state: SharedState, identity_path: PathBuf) -> Self {
        let identity = DeviceIdentity::load_or_create(&identity_path)
            .unwrap_or_else(|e| {
                warn!(error = %e, "failed to load identity, generating new one");
                DeviceIdentity::generate()
            });
        
        info!(device_id = %identity.device_id, "using device identity");
        
        Self {
            state,
            identity,
            outgoing_tx: None,
        }
    }
    
    pub fn with_identity(state: SharedState, identity: DeviceIdentity) -> Self {
        Self {
            state,
            identity,
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

        // These may be reassigned if we need to reconnect after challenge
        let (write, read) = ws_stream.split();
        let mut write = write;
        let mut read = read;
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<RequestFrame>(32);
        self.outgoing_tx = Some(outgoing_tx.clone());

        // Generate node identity
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let platform = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        
        let caps = self.state.capabilities.clone();
        let commands = self.state.commands.clone();
        let node_id = self.identity.device_id.clone();

        // For remote connections, gateway sends a challenge first
        // We need to: 1) wait for challenge, 2) sign with challenge nonce, 3) send connect
        
        // Wait for challenge (gateway sends this immediately on connection)
        info!("waiting for challenge from gateway...");
        let challenge_response = timeout(Duration::from_secs(10), read.next())
            .await
            .map_err(|_| "challenge timeout")?
            .ok_or("connection closed waiting for challenge")??;

        let challenge_nonce = match &challenge_response {
            Message::Text(text) => {
                let frame: Value = serde_json::from_str(text)?;
                info!("received from gateway: {}", text);
                
                if let Some(event) = frame.get("event").and_then(|e| e.as_str()) {
                    if event == "connect.challenge" {
                        frame.get("payload")
                            .and_then(|p| p.get("nonce"))
                            .and_then(|n| n.as_str())
                            .map(|s| s.to_string())
                    } else {
                        warn!("unexpected event: {}", event);
                        None
                    }
                } else if let Some(error) = frame.get("error") {
                    let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("unknown");
                    return Err(format!("gateway error on connect: {}", msg).into());
                } else {
                    warn!("unexpected frame type");
                    None
                }
            }
            Message::Close(frame) => {
                let reason = frame.as_ref().map(|f| f.reason.to_string()).unwrap_or_default();
                return Err(format!("gateway closed connection: {}", reason).into());
            }
            other => {
                warn!("unexpected message type: {:?}", other);
                None
            }
        };
        
        let challenge_nonce = challenge_nonce.ok_or("no challenge nonce received")?;
        info!("got challenge nonce: {}", challenge_nonce);
        
        // Build device params with the challenge nonce
        let device_params = self.identity.device_params(
            "node-host",
            "node",
            "node",
            &["node".to_string()],
            auth_token,
            Some(&challenge_nonce),
        );
        
        // Build mesh info from active mesh manager (if networking is initialized)
        #[cfg(feature = "network")]
        let mesh_info = {
            let mgr = self.state.mesh_manager.read().await;
            mgr.as_ref().map(|m| crate::mesh::MeshInfo {
                mesh_ip: m.mesh_ip().to_string(),
                wireguard_pubkey: m.public_key().to_string(),
                wireguard_port: m.listen_port(),
                region: format!("{}", m.region()),
                endpoint: None,
            })
        };

        let connect_params = ConnectParams {
            min_protocol: PROTOCOL_VERSION,
            max_protocol: PROTOCOL_VERSION,
            client: ClientInfo {
                id: "node-host".to_string(),  // Must match GATEWAY_CLIENT_IDS.NODE_HOST
                display_name: hostname.clone(),
                version: CLIENT_VERSION.to_string(),
                platform: platform.clone(),
                mode: "node".to_string(),
                instance_id: Uuid::new_v4().to_string(),
            },
            caps: caps.clone(),
            commands: commands.clone(),
            auth: auth_token.map(|t| AuthParams { token: Some(t.to_string()) }),
            device: Some(device_params),
            role: Some("node".to_string()),
            scopes: Some(vec!["node".to_string()]),
            #[cfg(feature = "network")]
            mesh: mesh_info,
        };

        // Send connect with device signature - must be a proper RequestFrame
        let connect_frame = json!({
            "type": "req",
            "id": Uuid::new_v4().to_string(),
            "method": "connect",
            "params": &connect_params
        });
        info!("sending signed connect...");
        debug!("connect frame: {}", connect_frame);
        
        match write.send(Message::Text(connect_frame.to_string().into())).await {
            Ok(_) => info!("sent connect frame successfully"),
            Err(e) => {
                error!("failed to send connect: {}", e);
                return Err(format!("failed to send connect: {}", e).into());
            }
        }
        
        // Flush to ensure message is sent
        if let Err(e) = write.flush().await {
            warn!("flush error (may be ok): {}", e);
        }

        // Wait for hello response
        info!("waiting for hello response...");
        let response = match timeout(Duration::from_secs(10), read.next()).await {
            Ok(Some(Ok(msg))) => msg,
            Ok(Some(Err(e))) => {
                error!("websocket read error: {}", e);
                return Err(format!("websocket error: {}", e).into());
            }
            Ok(None) => {
                error!("connection closed while waiting for hello");
                return Err("connection closed by gateway".into());
            }
            Err(_) => {
                error!("hello timeout");
                return Err("hello timeout".into());
            }
        };

        let mut already_paired = false;
        if let Message::Text(text) = response {
            let frame: Value = serde_json::from_str(&text)?;
            debug!("received after connect: {}", text);
            
            // Check for hello-ok in payload.type
            let is_hello_ok = frame.get("ok").and_then(|v| v.as_bool()).unwrap_or(false)
                && frame.get("payload")
                    .and_then(|p| p.get("type"))
                    .and_then(|t| t.as_str())
                    .map(|t| t == "hello-ok")
                    .unwrap_or(false);
            
            if is_hello_ok {
                info!("connected to gateway successfully!");
                already_paired = true;  // We got hello-ok, so we're already paired
                // Extract device token if provided
                if let Some(token) = frame.get("payload")
                    .and_then(|p| p.get("auth"))
                    .and_then(|a| a.get("deviceToken"))
                    .and_then(|t| t.as_str())
                {
                    info!("received device token");
                    debug!("device token: {}", token);
                }
            } else if let Some(error) = frame.get("error") {
                let msg = error.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                return Err(format!("gateway error: {}", msg).into());
            } else if frame.get("event").is_some() {
                warn!("gateway sent another challenge - signature rejected");
                return Err("device signature rejected".into());
            } else {
                warn!("unexpected response format");
            }
        }

        // Skip pairing request if we're already paired (got hello-ok)
        if already_paired {
            info!("already paired, skipping pairing request");
        } else {
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
        let pair_frame = RequestFrame::new(
            request_id.clone(),
            "node.pair.request".to_string(),
            Some(serde_json::to_value(&pair_request)?),
        );

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
        }  // end else (not already_paired)

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
                    let mut payload = json!({
                        "nodeId": node_id_clone,
                    });

                    // Enrich heartbeat with mesh status if available
                    #[cfg(feature = "network")]
                    {
                        let mgr = self.state.mesh_manager.read().await;
                        if let Some(ref m) = *mgr {
                            let status = m.status().await;
                            if let Some(obj) = payload.as_object_mut() {
                                obj.insert("meshIp".to_string(), json!(status.mesh_ip));
                                obj.insert("peerCount".to_string(), json!(status.peers.len()));
                            }
                        }
                    }

                    let heartbeat = RequestFrame::new(
                        Uuid::new_v4().to_string(),
                        "node.event".to_string(),
                        Some(json!({
                            "event": "heartbeat",
                            "payload": payload,
                        })),
                    );
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
                #[cfg(feature = "network")]
                "mesh.peer.join" => {
                    if let Some(payload) = frame.get("payload") {
                        match serde_json::from_value::<crate::mesh::PeerInfo>(payload.clone()) {
                            Ok(peer) => {
                                let mgr = self.state.mesh_manager.read().await;
                                if let Some(ref m) = *mgr {
                                    if let Err(e) = m.add_remote_peer(peer).await {
                                        warn!(error = %e, "failed to add mesh peer");
                                    }
                                }
                            }
                            Err(e) => warn!(error = %e, "invalid mesh.peer.join payload"),
                        }
                    }
                }
                #[cfg(feature = "network")]
                "mesh.peer.leave" => {
                    if let Some(payload) = frame.get("payload") {
                        if let Some(peer_node_id) = payload.get("nodeId").and_then(|v| v.as_str()) {
                            let mgr = self.state.mesh_manager.read().await;
                            if let Some(ref m) = *mgr {
                                let _ = m.remove_remote_peer(peer_node_id).await;
                            }
                        }
                    }
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

        let response = RequestFrame::new(
            Uuid::new_v4().to_string(),
            "node.invoke.result".to_string(),
            Some(serde_json::to_value(&result_params)?),
        );

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

    let frame = RequestFrame::new(
        Uuid::new_v4().to_string(),
        "node.invoke.result".to_string(),
        Some(serde_json::to_value(&result_params).unwrap()),
    );

    tx.send(frame).await
}
