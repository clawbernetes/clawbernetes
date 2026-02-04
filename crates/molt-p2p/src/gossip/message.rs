//! Gossip protocol message types with prost serialization.

use crate::error::P2pError;
use crate::protocol::PeerId;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique identifier for a gossip message.
///
/// Used for deduplication and tracking message propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId {
    bytes: [u8; 16],
}

impl MessageId {
    /// Creates a new random message ID.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bytes: Uuid::new_v4().into_bytes(),
        }
    }

    /// Creates a message ID from raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }

    /// Returns the raw bytes of the message ID.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", bs58::encode(&self.bytes).into_string())
    }
}

/// Filter criteria for gossip queries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryFilter {
    /// Filter by GPU model.
    pub gpu_model: Option<String>,
    /// Minimum VRAM in gigabytes.
    pub min_vram_gb: Option<u32>,
    /// Required job type.
    pub job_type: Option<String>,
    /// Maximum price per GPU-hour in cents.
    pub max_gpu_hour_cents: Option<u64>,
}

impl QueryFilter {
    /// Creates an empty filter that matches any announcement.
    #[must_use]
    pub const fn any() -> Self {
        Self {
            gpu_model: None,
            min_vram_gb: None,
            job_type: None,
            max_gpu_hour_cents: None,
        }
    }

    /// Sets the GPU model filter.
    #[must_use]
    pub fn with_gpu_model(mut self, model: impl Into<String>) -> Self {
        self.gpu_model = Some(model.into());
        self
    }

    /// Sets the minimum VRAM filter.
    #[must_use]
    pub const fn with_min_vram(mut self, vram_gb: u32) -> Self {
        self.min_vram_gb = Some(vram_gb);
        self
    }

    /// Sets the job type filter.
    #[must_use]
    pub fn with_job_type(mut self, job_type: impl Into<String>) -> Self {
        self.job_type = Some(job_type.into());
        self
    }

    /// Sets the maximum price filter.
    #[must_use]
    pub const fn with_max_price(mut self, cents: u64) -> Self {
        self.max_gpu_hour_cents = Some(cents);
        self
    }

    /// Returns true if this filter matches any announcement.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.gpu_model.is_none()
            && self.min_vram_gb.is_none()
            && self.job_type.is_none()
            && self.max_gpu_hour_cents.is_none()
    }
}

/// A query for capacity announcements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GossipQuery {
    /// Unique query identifier for correlation.
    pub query_id: MessageId,
    /// The peer making the query.
    pub from_peer: PeerId,
    /// Filter criteria.
    pub filter: QueryFilter,
    /// Maximum number of results desired.
    pub max_results: u32,
    /// Remaining hops for this query.
    pub ttl_hops: u8,
}

impl GossipQuery {
    /// Creates a new gossip query.
    #[must_use]
    pub fn new(from_peer: PeerId, filter: QueryFilter, max_results: u32, ttl_hops: u8) -> Self {
        Self {
            query_id: MessageId::new(),
            from_peer,
            filter,
            max_results,
            ttl_hops,
        }
    }

    /// Decrements the TTL and returns the new value, or None if expired.
    #[must_use]
    pub fn decrement_ttl(&self) -> Option<Self> {
        if self.ttl_hops == 0 {
            return None;
        }
        Some(Self {
            ttl_hops: self.ttl_hops - 1,
            ..self.clone()
        })
    }
}

/// Messages exchanged in the gossip protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMessage {
    /// Announce capacity to the network.
    Announce {
        /// Unique message ID for deduplication.
        message_id: MessageId,
        /// The capacity announcement.
        announcement: super::CapacityAnnouncement,
        /// Remaining hops for propagation.
        ttl_hops: u8,
    },

    /// Query for matching capacity announcements.
    Query(GossipQuery),

    /// Response to a query with matching announcements.
    Response {
        /// Correlates with the query ID.
        query_id: MessageId,
        /// Peer responding to the query.
        from_peer: PeerId,
        /// Matching announcements.
        announcements: Vec<super::CapacityAnnouncement>,
    },

    /// Heartbeat to maintain connection liveness.
    Heartbeat {
        /// The peer sending the heartbeat.
        from_peer: PeerId,
        /// Timestamp in milliseconds since Unix epoch.
        timestamp_ms: u64,
    },

    /// Request to sync recent announcements.
    SyncRequest {
        /// The peer requesting sync.
        from_peer: PeerId,
        /// Unix timestamp (seconds) - only send announcements newer than this.
        since_timestamp: i64,
    },

    /// Response to a sync request.
    SyncResponse {
        /// Announcements newer than the requested timestamp.
        announcements: Vec<super::CapacityAnnouncement>,
    },
}

impl GossipMessage {
    /// Creates a new announce message.
    #[must_use]
    pub fn announce(announcement: super::CapacityAnnouncement, ttl_hops: u8) -> Self {
        Self::Announce {
            message_id: MessageId::new(),
            announcement,
            ttl_hops,
        }
    }

    /// Creates a new query message.
    #[must_use]
    pub fn query(from_peer: PeerId, filter: QueryFilter, max_results: u32, ttl_hops: u8) -> Self {
        Self::Query(GossipQuery::new(from_peer, filter, max_results, ttl_hops))
    }

    /// Creates a new response message.
    #[must_use]
    pub fn response(
        query_id: MessageId,
        from_peer: PeerId,
        announcements: Vec<super::CapacityAnnouncement>,
    ) -> Self {
        Self::Response {
            query_id,
            from_peer,
            announcements,
        }
    }

    /// Creates a heartbeat message.
    #[must_use]
    pub fn heartbeat(from_peer: PeerId) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self::Heartbeat {
            from_peer,
            timestamp_ms,
        }
    }

    /// Creates a sync request message.
    #[must_use]
    pub fn sync_request(from_peer: PeerId, since_timestamp: i64) -> Self {
        Self::SyncRequest {
            from_peer,
            since_timestamp,
        }
    }

    /// Creates a sync response message.
    #[must_use]
    pub fn sync_response(announcements: Vec<super::CapacityAnnouncement>) -> Self {
        Self::SyncResponse { announcements }
    }

    /// Returns the message type as a string.
    #[must_use]
    pub const fn message_type(&self) -> &'static str {
        match self {
            Self::Announce { .. } => "Announce",
            Self::Query(_) => "Query",
            Self::Response { .. } => "Response",
            Self::Heartbeat { .. } => "Heartbeat",
            Self::SyncRequest { .. } => "SyncRequest",
            Self::SyncResponse { .. } => "SyncResponse",
        }
    }

    /// Returns the message ID if applicable.
    #[must_use]
    pub fn message_id(&self) -> Option<MessageId> {
        match self {
            Self::Announce { message_id, .. } => Some(*message_id),
            Self::Query(q) => Some(q.query_id),
            Self::Response { query_id, .. } => Some(*query_id),
            _ => None,
        }
    }
}

// ============ Prost Wire Format ============

/// Prost-encoded wrapper for gossip messages.
#[derive(Clone, PartialEq, Message)]
pub struct WireGossipMessage {
    /// Message type discriminator.
    #[prost(uint32, tag = "1")]
    pub msg_type: u32,
    /// JSON-encoded payload (for complex nested types).
    #[prost(bytes = "vec", tag = "2")]
    pub payload: Vec<u8>,
    /// Protocol version for forward compatibility.
    #[prost(uint32, tag = "3")]
    pub version: u32,
}

/// Message type constants for wire encoding.
pub mod wire_types {
    /// Announce message type.
    pub const ANNOUNCE: u32 = 1;
    /// Query message type.
    pub const QUERY: u32 = 2;
    /// Response message type.
    pub const RESPONSE: u32 = 3;
    /// Heartbeat message type.
    pub const HEARTBEAT: u32 = 4;
    /// Sync request message type.
    pub const SYNC_REQUEST: u32 = 5;
    /// Sync response message type.
    pub const SYNC_RESPONSE: u32 = 6;
}

/// Protocol version for wire format compatibility.
///
/// This enum defines the supported protocol versions and provides
/// compatibility checking between peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProtocolVersion {
    /// Version 1: Initial protocol with basic message types.
    /// Supports: `Announce`, `Query`, `Response`, `Heartbeat`, `SyncRequest`, `SyncResponse`
    V1 = 1,
}

impl ProtocolVersion {
    /// The current protocol version used for encoding.
    pub const CURRENT: Self = Self::V1;

    /// The minimum protocol version we can communicate with.
    pub const MIN_COMPATIBLE: Self = Self::V1;

    /// Creates a protocol version from a raw u32 value.
    ///
    /// # Errors
    ///
    /// Returns an error if the version is unknown or unsupported.
    pub fn from_u32(value: u32) -> Result<Self, P2pError> {
        match value {
            0 => Err(P2pError::Protocol(
                "Protocol version 0 is invalid".to_string(),
            )),
            1 => Ok(Self::V1),
            v if v > Self::CURRENT as u32 => {
                let max = Self::CURRENT as u32;
                Err(P2pError::Protocol(format!(
                    "Unsupported protocol version: {v} (max supported: {max})"
                )))
            }
            v => Err(P2pError::Protocol(format!(
                "Unknown protocol version: {v}"
            ))),
        }
    }

    /// Returns the raw u32 value of this version.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// Checks if this version is compatible with another version.
    ///
    /// Two versions are compatible if they can exchange messages.
    /// Currently, all supported versions are backward compatible.
    #[must_use]
    pub fn is_compatible_with(self, other: Self) -> bool {
        // Both versions must be at least MIN_COMPATIBLE
        self >= Self::MIN_COMPATIBLE && other >= Self::MIN_COMPATIBLE
    }

    /// Checks if a message type is valid for this protocol version.
    #[must_use]
    pub fn is_valid_message_type(self, msg_type: u32) -> bool {
        match self {
            Self::V1 => {
                matches!(
                    msg_type,
                    wire_types::ANNOUNCE
                        | wire_types::QUERY
                        | wire_types::RESPONSE
                        | wire_types::HEARTBEAT
                        | wire_types::SYNC_REQUEST
                        | wire_types::SYNC_RESPONSE
                )
            }
        }
    }

    /// Returns a human-readable description of message types supported by this version.
    #[must_use]
    pub const fn supported_message_types(self) -> &'static str {
        match self {
            Self::V1 => "Announce, Query, Response, Heartbeat, SyncRequest, SyncResponse",
        }
    }

    /// Returns the maximum known message type value for this version.
    #[must_use]
    pub const fn max_message_type(self) -> u32 {
        match self {
            Self::V1 => wire_types::SYNC_RESPONSE,
        }
    }
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self::CURRENT
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{}", self.as_u32())
    }
}

impl TryFrom<u32> for ProtocolVersion {
    type Error = P2pError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::from_u32(value)
    }
}

impl From<ProtocolVersion> for u32 {
    fn from(version: ProtocolVersion) -> Self {
        version.as_u32()
    }
}

/// Current wire protocol version (for backward compatibility with existing code).
#[allow(dead_code)]
pub const WIRE_VERSION: u32 = ProtocolVersion::CURRENT.as_u32();

impl GossipMessage {
    /// Encodes the message to prost wire format.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON serialization fails.
    pub fn encode_wire(&self) -> Result<Vec<u8>, P2pError> {
        self.encode_wire_with_version(ProtocolVersion::CURRENT)
    }

    /// Encodes the message to prost wire format with a specific protocol version.
    ///
    /// # Errors
    ///
    /// Returns an error if JSON serialization fails or the message type
    /// is not supported by the specified protocol version.
    pub fn encode_wire_with_version(&self, version: ProtocolVersion) -> Result<Vec<u8>, P2pError> {
        let msg_type = match self {
            Self::Announce { .. } => wire_types::ANNOUNCE,
            Self::Query(_) => wire_types::QUERY,
            Self::Response { .. } => wire_types::RESPONSE,
            Self::Heartbeat { .. } => wire_types::HEARTBEAT,
            Self::SyncRequest { .. } => wire_types::SYNC_REQUEST,
            Self::SyncResponse { .. } => wire_types::SYNC_RESPONSE,
        };

        // Validate message type is supported by the target version
        if !version.is_valid_message_type(msg_type) {
            return Err(P2pError::Protocol(format!(
                "Message type {} is not supported by protocol {}",
                self.message_type(),
                version
            )));
        }

        let payload = serde_json::to_vec(self)
            .map_err(|e| P2pError::Protocol(format!("Failed to serialize message: {e}")))?;

        let wire_msg = WireGossipMessage {
            msg_type,
            payload,
            version: version.as_u32(),
        };

        Ok(wire_msg.encode_to_vec())
    }

    /// Decodes a message from prost wire format.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The wire format is invalid
    /// - The protocol version is unsupported or incompatible
    /// - The message type is unknown for the protocol version
    /// - The payload deserialization fails
    pub fn decode_wire(bytes: &[u8]) -> Result<Self, P2pError> {
        let wire_msg = WireGossipMessage::decode(bytes)
            .map_err(|e| P2pError::Protocol(format!("Failed to decode wire message: {e}")))?;

        // Validate protocol version
        let version = ProtocolVersion::from_u32(wire_msg.version)?;

        // Ensure we're compatible with this version
        if !ProtocolVersion::CURRENT.is_compatible_with(version) {
            return Err(P2pError::Protocol(format!(
                "Protocol version {} is not compatible with current version {}",
                version,
                ProtocolVersion::CURRENT
            )));
        }

        // Validate message type for this protocol version
        if !version.is_valid_message_type(wire_msg.msg_type) {
            return Err(P2pError::Protocol(format!(
                "Unknown message type {} for protocol version {} (supported: {})",
                wire_msg.msg_type,
                version,
                version.supported_message_types()
            )));
        }

        // Decode the payload
        serde_json::from_slice(&wire_msg.payload)
            .map_err(|e| P2pError::Protocol(format!("Failed to deserialize message: {e}")))
    }

    /// Decodes a message and returns both the message and the protocol version.
    ///
    /// This is useful when you need to know the sender's protocol version
    /// for version negotiation or logging.
    ///
    /// # Errors
    ///
    /// Same as [`decode_wire`].
    pub fn decode_wire_with_version(bytes: &[u8]) -> Result<(Self, ProtocolVersion), P2pError> {
        let wire_msg = WireGossipMessage::decode(bytes)
            .map_err(|e| P2pError::Protocol(format!("Failed to decode wire message: {e}")))?;

        // Validate protocol version
        let version = ProtocolVersion::from_u32(wire_msg.version)?;

        // Ensure we're compatible with this version
        if !ProtocolVersion::CURRENT.is_compatible_with(version) {
            return Err(P2pError::Protocol(format!(
                "Protocol version {} is not compatible with current version {}",
                version,
                ProtocolVersion::CURRENT
            )));
        }

        // Validate message type for this protocol version
        if !version.is_valid_message_type(wire_msg.msg_type) {
            return Err(P2pError::Protocol(format!(
                "Unknown message type {} for protocol version {} (supported: {})",
                wire_msg.msg_type,
                version,
                version.supported_message_types()
            )));
        }

        // Decode the payload
        let message: Self = serde_json::from_slice(&wire_msg.payload)
            .map_err(|e| P2pError::Protocol(format!("Failed to deserialize message: {e}")))?;

        Ok((message, version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gossip::{GpuInfo, Pricing};
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use std::time::Duration;

    fn make_peer_id() -> PeerId {
        let signing_key = SigningKey::generate(&mut OsRng);
        PeerId::from_public_key(&signing_key.verifying_key())
    }

    fn make_announcement() -> super::super::CapacityAnnouncement {
        let signing_key = SigningKey::generate(&mut OsRng);
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());
        let mut announcement = super::super::CapacityAnnouncement::new(
            peer_id,
            vec![GpuInfo {
                model: "RTX 4090".to_string(),
                vram_gb: 24,
                count: 2,
            }],
            Pricing {
                gpu_hour_cents: 100,
                cpu_hour_cents: 10,
            },
            vec!["inference".to_string()],
            Duration::from_secs(300),
        );
        announcement.sign(&signing_key);
        announcement
    }

    // ========== MessageId Tests ==========

    #[test]
    fn message_id_creation() {
        let id1 = MessageId::new();
        let id2 = MessageId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn message_id_from_bytes_roundtrip() {
        let bytes = [1u8; 16];
        let id = MessageId::from_bytes(bytes);
        assert_eq!(*id.as_bytes(), bytes);
    }

    #[test]
    fn message_id_display() {
        let id = MessageId::from_bytes([0u8; 16]);
        let displayed = id.to_string();
        assert!(!displayed.is_empty());
    }

    #[test]
    fn message_id_default_is_random() {
        let id1 = MessageId::default();
        let id2 = MessageId::default();
        assert_ne!(id1, id2);
    }

    // ========== QueryFilter Tests ==========

    #[test]
    fn query_filter_any_is_empty() {
        let filter = QueryFilter::any();
        assert!(filter.is_empty());
    }

    #[test]
    fn query_filter_builder() {
        let filter = QueryFilter::any()
            .with_gpu_model("A100")
            .with_min_vram(80)
            .with_job_type("training")
            .with_max_price(500);

        assert_eq!(filter.gpu_model, Some("A100".to_string()));
        assert_eq!(filter.min_vram_gb, Some(80));
        assert_eq!(filter.job_type, Some("training".to_string()));
        assert_eq!(filter.max_gpu_hour_cents, Some(500));
        assert!(!filter.is_empty());
    }

    // ========== GossipQuery Tests ==========

    #[test]
    fn gossip_query_creation() {
        let peer_id = make_peer_id();
        let filter = QueryFilter::any().with_min_vram(24);
        let query = GossipQuery::new(peer_id, filter.clone(), 10, 5);

        assert_eq!(query.from_peer, peer_id);
        assert_eq!(query.filter, filter);
        assert_eq!(query.max_results, 10);
        assert_eq!(query.ttl_hops, 5);
    }

    #[test]
    fn gossip_query_decrement_ttl() {
        let peer_id = make_peer_id();
        let query = GossipQuery::new(peer_id, QueryFilter::any(), 10, 3);

        let decremented = query.decrement_ttl();
        assert!(decremented.is_some());
        assert_eq!(decremented.as_ref().map(|q| q.ttl_hops), Some(2));

        // Same query ID
        assert_eq!(decremented.as_ref().map(|q| q.query_id), Some(query.query_id));
    }

    #[test]
    fn gossip_query_decrement_ttl_at_zero() {
        let peer_id = make_peer_id();
        let query = GossipQuery::new(peer_id, QueryFilter::any(), 10, 0);

        assert!(query.decrement_ttl().is_none());
    }

    // ========== GossipMessage Tests ==========

    #[test]
    fn gossip_message_announce_creation() {
        let announcement = make_announcement();
        let msg = GossipMessage::announce(announcement.clone(), 3);

        match msg {
            GossipMessage::Announce {
                message_id,
                announcement: a,
                ttl_hops,
            } => {
                assert!(message_id.as_bytes().iter().any(|&b| b != 0));
                assert_eq!(a.peer_id(), announcement.peer_id());
                assert_eq!(ttl_hops, 3);
            }
            _ => panic!("Expected Announce message"),
        }
    }

    #[test]
    fn gossip_message_query_creation() {
        let peer_id = make_peer_id();
        let filter = QueryFilter::any().with_gpu_model("RTX 4090");
        let msg = GossipMessage::query(peer_id, filter.clone(), 5, 4);

        match msg {
            GossipMessage::Query(q) => {
                assert_eq!(q.from_peer, peer_id);
                assert_eq!(q.filter, filter);
                assert_eq!(q.max_results, 5);
                assert_eq!(q.ttl_hops, 4);
            }
            _ => panic!("Expected Query message"),
        }
    }

    #[test]
    fn gossip_message_response_creation() {
        let query_id = MessageId::new();
        let peer_id = make_peer_id();
        let announcements = vec![make_announcement(), make_announcement()];
        let msg = GossipMessage::response(query_id, peer_id, announcements.clone());

        match msg {
            GossipMessage::Response {
                query_id: qid,
                from_peer,
                announcements: a,
            } => {
                assert_eq!(qid, query_id);
                assert_eq!(from_peer, peer_id);
                assert_eq!(a.len(), 2);
            }
            _ => panic!("Expected Response message"),
        }
    }

    #[test]
    fn gossip_message_heartbeat_creation() {
        let peer_id = make_peer_id();
        let msg = GossipMessage::heartbeat(peer_id);

        match msg {
            GossipMessage::Heartbeat {
                from_peer,
                timestamp_ms,
            } => {
                assert_eq!(from_peer, peer_id);
                assert!(timestamp_ms > 0);
            }
            _ => panic!("Expected Heartbeat message"),
        }
    }

    #[test]
    fn gossip_message_sync_request_creation() {
        let peer_id = make_peer_id();
        let msg = GossipMessage::sync_request(peer_id, 1234567890);

        match msg {
            GossipMessage::SyncRequest {
                from_peer,
                since_timestamp,
            } => {
                assert_eq!(from_peer, peer_id);
                assert_eq!(since_timestamp, 1234567890);
            }
            _ => panic!("Expected SyncRequest message"),
        }
    }

    #[test]
    fn gossip_message_sync_response_creation() {
        let announcements = vec![make_announcement()];
        let msg = GossipMessage::sync_response(announcements.clone());

        match msg {
            GossipMessage::SyncResponse { announcements: a } => {
                assert_eq!(a.len(), 1);
            }
            _ => panic!("Expected SyncResponse message"),
        }
    }

    #[test]
    fn gossip_message_type_strings() {
        assert_eq!(GossipMessage::announce(make_announcement(), 1).message_type(), "Announce");
        assert_eq!(
            GossipMessage::query(make_peer_id(), QueryFilter::any(), 1, 1).message_type(),
            "Query"
        );
        assert_eq!(
            GossipMessage::response(MessageId::new(), make_peer_id(), vec![]).message_type(),
            "Response"
        );
        assert_eq!(GossipMessage::heartbeat(make_peer_id()).message_type(), "Heartbeat");
        assert_eq!(
            GossipMessage::sync_request(make_peer_id(), 0).message_type(),
            "SyncRequest"
        );
        assert_eq!(
            GossipMessage::sync_response(vec![]).message_type(),
            "SyncResponse"
        );
    }

    #[test]
    fn gossip_message_id_extraction() {
        let announcement = make_announcement();
        let msg = GossipMessage::announce(announcement, 3);
        assert!(msg.message_id().is_some());

        let query_msg = GossipMessage::query(make_peer_id(), QueryFilter::any(), 1, 1);
        assert!(query_msg.message_id().is_some());

        let heartbeat = GossipMessage::heartbeat(make_peer_id());
        assert!(heartbeat.message_id().is_none());
    }

    // ========== Wire Encoding Tests ==========

    #[test]
    fn wire_encode_decode_announce() {
        let announcement = make_announcement();
        let msg = GossipMessage::announce(announcement.clone(), 5);

        let encoded = msg.encode_wire().expect("encoding should succeed");
        let decoded = GossipMessage::decode_wire(&encoded).expect("decoding should succeed");

        match decoded {
            GossipMessage::Announce {
                announcement: a,
                ttl_hops,
                ..
            } => {
                assert_eq!(a.peer_id(), announcement.peer_id());
                assert_eq!(ttl_hops, 5);
            }
            _ => panic!("Expected Announce message"),
        }
    }

    #[test]
    fn wire_encode_decode_query() {
        let peer_id = make_peer_id();
        let filter = QueryFilter::any().with_min_vram(32);
        let msg = GossipMessage::query(peer_id, filter.clone(), 10, 3);

        let encoded = msg.encode_wire().expect("encoding should succeed");
        let decoded = GossipMessage::decode_wire(&encoded).expect("decoding should succeed");

        match decoded {
            GossipMessage::Query(q) => {
                assert_eq!(q.from_peer, peer_id);
                assert_eq!(q.filter, filter);
                assert_eq!(q.max_results, 10);
            }
            _ => panic!("Expected Query message"),
        }
    }

    #[test]
    fn wire_encode_decode_response() {
        let query_id = MessageId::new();
        let peer_id = make_peer_id();
        let announcements = vec![make_announcement()];
        let msg = GossipMessage::response(query_id, peer_id, announcements);

        let encoded = msg.encode_wire().expect("encoding should succeed");
        let decoded = GossipMessage::decode_wire(&encoded).expect("decoding should succeed");

        match decoded {
            GossipMessage::Response {
                query_id: qid,
                from_peer,
                announcements: a,
            } => {
                assert_eq!(qid, query_id);
                assert_eq!(from_peer, peer_id);
                assert_eq!(a.len(), 1);
            }
            _ => panic!("Expected Response message"),
        }
    }

    #[test]
    fn wire_encode_decode_heartbeat() {
        let peer_id = make_peer_id();
        let msg = GossipMessage::heartbeat(peer_id);

        let encoded = msg.encode_wire().expect("encoding should succeed");
        let decoded = GossipMessage::decode_wire(&encoded).expect("decoding should succeed");

        match decoded {
            GossipMessage::Heartbeat { from_peer, .. } => {
                assert_eq!(from_peer, peer_id);
            }
            _ => panic!("Expected Heartbeat message"),
        }
    }

    #[test]
    fn wire_decode_invalid_bytes() {
        let invalid = b"not valid prost";
        let result = GossipMessage::decode_wire(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn wire_decode_empty_bytes() {
        let result = GossipMessage::decode_wire(&[]);
        // Empty should decode to default WireGossipMessage, which has empty payload
        // That should fail JSON deserialization
        assert!(result.is_err());
    }

    // ========== Protocol Version Tests ==========

    #[test]
    fn protocol_version_current_is_v1() {
        assert_eq!(ProtocolVersion::CURRENT, ProtocolVersion::V1);
        assert_eq!(ProtocolVersion::CURRENT.as_u32(), 1);
    }

    #[test]
    fn protocol_version_from_u32_valid() {
        let v1 = ProtocolVersion::from_u32(1);
        assert!(v1.is_ok());
        assert_eq!(v1.unwrap(), ProtocolVersion::V1);
    }

    #[test]
    fn protocol_version_from_u32_zero_rejected() {
        let result = ProtocolVersion::from_u32(0);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid"));
    }

    #[test]
    fn protocol_version_from_u32_future_rejected() {
        // Future version (beyond current)
        let result = ProtocolVersion::from_u32(99);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unsupported"));
    }

    #[test]
    fn protocol_version_compatibility_same_version() {
        assert!(ProtocolVersion::V1.is_compatible_with(ProtocolVersion::V1));
    }

    #[test]
    fn protocol_version_try_from_trait() {
        let result: Result<ProtocolVersion, _> = 1u32.try_into();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ProtocolVersion::V1);

        let result: Result<ProtocolVersion, _> = 99u32.try_into();
        assert!(result.is_err());
    }

    #[test]
    fn protocol_version_into_u32() {
        let v: u32 = ProtocolVersion::V1.into();
        assert_eq!(v, 1);
    }

    #[test]
    fn protocol_version_display() {
        assert_eq!(format!("{}", ProtocolVersion::V1), "v1");
    }

    #[test]
    fn protocol_version_default() {
        assert_eq!(ProtocolVersion::default(), ProtocolVersion::CURRENT);
    }

    #[test]
    fn protocol_version_valid_message_types_v1() {
        // All V1 message types should be valid
        assert!(ProtocolVersion::V1.is_valid_message_type(wire_types::ANNOUNCE));
        assert!(ProtocolVersion::V1.is_valid_message_type(wire_types::QUERY));
        assert!(ProtocolVersion::V1.is_valid_message_type(wire_types::RESPONSE));
        assert!(ProtocolVersion::V1.is_valid_message_type(wire_types::HEARTBEAT));
        assert!(ProtocolVersion::V1.is_valid_message_type(wire_types::SYNC_REQUEST));
        assert!(ProtocolVersion::V1.is_valid_message_type(wire_types::SYNC_RESPONSE));
    }

    #[test]
    fn protocol_version_invalid_message_types_v1() {
        // Unknown message types should be invalid
        assert!(!ProtocolVersion::V1.is_valid_message_type(0)); // Zero
        assert!(!ProtocolVersion::V1.is_valid_message_type(7)); // Beyond known types
        assert!(!ProtocolVersion::V1.is_valid_message_type(100)); // Far beyond
        assert!(!ProtocolVersion::V1.is_valid_message_type(u32::MAX)); // Maximum
    }

    #[test]
    fn protocol_version_supported_message_types_description() {
        let desc = ProtocolVersion::V1.supported_message_types();
        assert!(desc.contains("Announce"));
        assert!(desc.contains("Query"));
        assert!(desc.contains("Response"));
        assert!(desc.contains("Heartbeat"));
        assert!(desc.contains("SyncRequest"));
        assert!(desc.contains("SyncResponse"));
    }

    #[test]
    fn protocol_version_max_message_type() {
        assert_eq!(ProtocolVersion::V1.max_message_type(), wire_types::SYNC_RESPONSE);
    }

    // ========== Wire Decode Version Validation Tests ==========

    #[test]
    fn wire_decode_rejects_version_zero() {
        // Manually create a wire message with version 0
        let msg = GossipMessage::heartbeat(make_peer_id());
        let payload = serde_json::to_vec(&msg).expect("serialize");
        let wire_msg = WireGossipMessage {
            msg_type: wire_types::HEARTBEAT,
            payload,
            version: 0, // Invalid version
        };
        let encoded = wire_msg.encode_to_vec();

        let result = GossipMessage::decode_wire(&encoded);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid"));
    }

    #[test]
    fn wire_decode_rejects_future_version() {
        // Manually create a wire message with a future version
        let msg = GossipMessage::heartbeat(make_peer_id());
        let payload = serde_json::to_vec(&msg).expect("serialize");
        let wire_msg = WireGossipMessage {
            msg_type: wire_types::HEARTBEAT,
            payload,
            version: 99, // Future version
        };
        let encoded = wire_msg.encode_to_vec();

        let result = GossipMessage::decode_wire(&encoded);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unsupported"));
    }

    #[test]
    fn wire_decode_rejects_unknown_message_type() {
        // Manually create a wire message with an unknown message type
        let msg = GossipMessage::heartbeat(make_peer_id());
        let payload = serde_json::to_vec(&msg).expect("serialize");
        let wire_msg = WireGossipMessage {
            msg_type: 100, // Unknown message type
            payload,
            version: 1,
        };
        let encoded = wire_msg.encode_to_vec();

        let result = GossipMessage::decode_wire(&encoded);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown message type"));
    }

    #[test]
    fn wire_decode_rejects_message_type_zero() {
        // Message type 0 is not valid
        let msg = GossipMessage::heartbeat(make_peer_id());
        let payload = serde_json::to_vec(&msg).expect("serialize");
        let wire_msg = WireGossipMessage {
            msg_type: 0, // Invalid message type
            payload,
            version: 1,
        };
        let encoded = wire_msg.encode_to_vec();

        let result = GossipMessage::decode_wire(&encoded);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown message type"));
    }

    #[test]
    fn wire_decode_with_version_returns_both() {
        let peer_id = make_peer_id();
        let msg = GossipMessage::heartbeat(peer_id);

        let encoded = msg.encode_wire().expect("encoding should succeed");
        let (decoded, version) = GossipMessage::decode_wire_with_version(&encoded)
            .expect("decoding should succeed");

        assert_eq!(version, ProtocolVersion::V1);
        match decoded {
            GossipMessage::Heartbeat { from_peer, .. } => {
                assert_eq!(from_peer, peer_id);
            }
            _ => panic!("Expected Heartbeat message"),
        }
    }

    #[test]
    fn wire_encode_with_version() {
        let msg = GossipMessage::heartbeat(make_peer_id());

        let encoded = msg.encode_wire_with_version(ProtocolVersion::V1)
            .expect("encoding should succeed");
        let (decoded, version) = GossipMessage::decode_wire_with_version(&encoded)
            .expect("decoding should succeed");

        assert_eq!(version, ProtocolVersion::V1);
        assert!(matches!(decoded, GossipMessage::Heartbeat { .. }));
    }

    #[test]
    fn wire_decode_all_message_types_v1() {
        // Verify all V1 message types can be encoded and decoded
        let messages = vec![
            GossipMessage::announce(make_announcement(), 3),
            GossipMessage::query(make_peer_id(), QueryFilter::any(), 10, 5),
            GossipMessage::response(MessageId::new(), make_peer_id(), vec![]),
            GossipMessage::heartbeat(make_peer_id()),
            GossipMessage::sync_request(make_peer_id(), 12345),
            GossipMessage::sync_response(vec![]),
        ];

        for msg in messages {
            let encoded = msg.encode_wire().expect("encoding should succeed");
            let (decoded, version) = GossipMessage::decode_wire_with_version(&encoded)
                .expect("decoding should succeed");

            assert_eq!(version, ProtocolVersion::V1);
            assert_eq!(decoded.message_type(), msg.message_type());
        }
    }

    #[test]
    fn wire_version_constant_matches_current() {
        assert_eq!(WIRE_VERSION, ProtocolVersion::CURRENT.as_u32());
    }

    // ========== Version Negotiation Tests ==========

    #[test]
    fn version_negotiation_both_v1() {
        // Simulate two peers both using V1
        let sender_version = ProtocolVersion::V1;
        let receiver_version = ProtocolVersion::V1;

        assert!(sender_version.is_compatible_with(receiver_version));
        assert!(receiver_version.is_compatible_with(sender_version));
    }

    #[test]
    fn version_min_compatible_is_v1() {
        assert_eq!(ProtocolVersion::MIN_COMPATIBLE, ProtocolVersion::V1);
    }

    // ========== Proptest ==========

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn message_id_roundtrip(bytes in prop::array::uniform16(any::<u8>())) {
                let id = MessageId::from_bytes(bytes);
                prop_assert_eq!(*id.as_bytes(), bytes);
            }

            #[test]
            fn query_filter_serialization(
                min_vram in proptest::option::of(1u32..256),
                max_price in proptest::option::of(1u64..1_000_000)
            ) {
                let mut filter = QueryFilter::any();
                if let Some(v) = min_vram {
                    filter = filter.with_min_vram(v);
                }
                if let Some(p) = max_price {
                    filter = filter.with_max_price(p);
                }

                let json = serde_json::to_string(&filter).unwrap();
                let deserialized: QueryFilter = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(filter, deserialized);
            }

            #[test]
            fn gossip_query_ttl_decrement(ttl in 0u8..10) {
                let query = GossipQuery::new(
                    make_peer_id(),
                    QueryFilter::any(),
                    10,
                    ttl,
                );

                if ttl == 0 {
                    prop_assert!(query.decrement_ttl().is_none());
                } else {
                    let decremented = query.decrement_ttl();
                    prop_assert!(decremented.is_some());
                    prop_assert_eq!(decremented.unwrap().ttl_hops, ttl - 1);
                }
            }

            #[test]
            fn protocol_version_from_u32_rejects_invalid(version in 2u32..1000) {
                // All versions > 1 (current) should be rejected
                let result = ProtocolVersion::from_u32(version);
                prop_assert!(result.is_err());
            }

            #[test]
            fn wire_decode_rejects_unknown_message_types(msg_type in 7u32..1000) {
                // All message types > 6 should be rejected for V1
                let msg = GossipMessage::heartbeat(make_peer_id());
                let payload = serde_json::to_vec(&msg).ok();
                if let Some(payload) = payload {
                    let wire_msg = WireGossipMessage {
                        msg_type,
                        payload,
                        version: 1,
                    };
                    let encoded = wire_msg.encode_to_vec();
                    let result = GossipMessage::decode_wire(&encoded);
                    prop_assert!(result.is_err());
                }
            }

            #[test]
            fn wire_encode_decode_roundtrip_all_versions(msg_type in 1u32..=6) {
                // All valid message types for V1 should roundtrip successfully
                let msg = match msg_type {
                    1 => GossipMessage::announce(make_announcement(), 3),
                    2 => GossipMessage::query(make_peer_id(), QueryFilter::any(), 10, 5),
                    3 => GossipMessage::response(MessageId::new(), make_peer_id(), vec![]),
                    4 => GossipMessage::heartbeat(make_peer_id()),
                    5 => GossipMessage::sync_request(make_peer_id(), 12345),
                    6 => GossipMessage::sync_response(vec![]),
                    _ => unreachable!(),
                };

                let encoded = msg.encode_wire();
                prop_assert!(encoded.is_ok());

                let decoded = GossipMessage::decode_wire(&encoded.unwrap());
                prop_assert!(decoded.is_ok());
                prop_assert_eq!(decoded.unwrap().message_type(), msg.message_type());
            }
        }
    }
}
