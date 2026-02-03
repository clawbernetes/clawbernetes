//! P2P message types for network communication.
//!
//! This module defines the messages exchanged between peers in the MOLT network.

use crate::gossip::CapacityAnnouncement;
use crate::protocol::PeerId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Requirements for finding compute providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapacityRequirements {
    /// Minimum GPU VRAM in gigabytes.
    pub min_vram_gb: Option<u32>,
    /// Required GPU model (e.g., "A100", "RTX 4090").
    pub gpu_model: Option<String>,
    /// Minimum number of GPUs required.
    pub min_gpu_count: Option<u32>,
    /// Required job type (e.g., "inference", "training").
    pub job_type: Option<String>,
    /// Maximum price per GPU-hour in cents.
    pub max_gpu_hour_cents: Option<u64>,
}

impl CapacityRequirements {
    /// Creates empty requirements (matches any provider).
    #[must_use]
    pub const fn any() -> Self {
        Self {
            min_vram_gb: None,
            gpu_model: None,
            min_gpu_count: None,
            job_type: None,
            max_gpu_hour_cents: None,
        }
    }

    /// Sets the minimum VRAM requirement.
    #[must_use]
    pub const fn with_min_vram(mut self, vram_gb: u32) -> Self {
        self.min_vram_gb = Some(vram_gb);
        self
    }

    /// Sets the GPU model requirement.
    #[must_use]
    pub fn with_gpu_model(mut self, model: impl Into<String>) -> Self {
        self.gpu_model = Some(model.into());
        self
    }

    /// Sets the minimum GPU count requirement.
    #[must_use]
    pub const fn with_min_gpu_count(mut self, count: u32) -> Self {
        self.min_gpu_count = Some(count);
        self
    }

    /// Sets the job type requirement.
    #[must_use]
    pub fn with_job_type(mut self, job_type: impl Into<String>) -> Self {
        self.job_type = Some(job_type.into());
        self
    }

    /// Sets the maximum price requirement.
    #[must_use]
    pub const fn with_max_price(mut self, cents_per_gpu_hour: u64) -> Self {
        self.max_gpu_hour_cents = Some(cents_per_gpu_hour);
        self
    }
}

impl Default for CapacityRequirements {
    fn default() -> Self {
        Self::any()
    }
}

/// Messages exchanged between peers in the P2P network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2pMessage {
    /// Ping to check if a peer is alive.
    Ping {
        /// Unique identifier for this ping.
        nonce: u64,
    },

    /// Response to a ping.
    Pong {
        /// Echo of the ping nonce.
        nonce: u64,
    },

    /// Announcement of available compute capacity.
    CapacityAnnounce {
        /// The capacity announcement with signature.
        announcement: CapacityAnnouncement,
    },

    /// Request for providers matching certain requirements.
    CapacityRequest {
        /// Unique request ID for correlation.
        request_id: Uuid,
        /// Requirements the provider must meet.
        requirements: CapacityRequirements,
        /// Maximum number of providers to return.
        max_results: u32,
    },

    /// Response to a capacity request.
    CapacityResponse {
        /// Correlates with the request ID.
        request_id: Uuid,
        /// List of providers matching the requirements.
        providers: Vec<CapacityAnnouncement>,
    },

    /// Request to join the network.
    Join {
        /// The joining peer's ID.
        peer_id: PeerId,
        /// Addresses where this peer can be reached.
        addresses: Vec<String>,
        /// Capabilities of this peer.
        capabilities: Vec<String>,
    },

    /// Acknowledgment of a join request.
    JoinAck {
        /// The peer that accepted the join.
        from_peer: PeerId,
        /// Known peers to help the joiner bootstrap.
        known_peers: Vec<(PeerId, Vec<String>)>,
    },

    /// Notification that a peer is leaving the network.
    Leave {
        /// The leaving peer's ID.
        peer_id: PeerId,
    },
}

impl P2pMessage {
    /// Creates a new Ping message with a random nonce.
    #[must_use]
    pub fn ping() -> Self {
        Self::Ping {
            nonce: rand::random(),
        }
    }

    /// Creates a Pong response to a Ping.
    #[must_use]
    pub const fn pong(nonce: u64) -> Self {
        Self::Pong { nonce }
    }

    /// Creates a capacity announce message.
    #[must_use]
    pub const fn capacity_announce(announcement: CapacityAnnouncement) -> Self {
        Self::CapacityAnnounce { announcement }
    }

    /// Creates a capacity request message.
    #[must_use]
    pub fn capacity_request(requirements: CapacityRequirements, max_results: u32) -> Self {
        Self::CapacityRequest {
            request_id: Uuid::new_v4(),
            requirements,
            max_results,
        }
    }

    /// Creates a capacity response message.
    #[must_use]
    pub const fn capacity_response(request_id: Uuid, providers: Vec<CapacityAnnouncement>) -> Self {
        Self::CapacityResponse {
            request_id,
            providers,
        }
    }

    /// Creates a join message.
    #[must_use]
    pub const fn join(peer_id: PeerId, addresses: Vec<String>, capabilities: Vec<String>) -> Self {
        Self::Join {
            peer_id,
            addresses,
            capabilities,
        }
    }

    /// Creates a join acknowledgment message.
    #[must_use]
    pub const fn join_ack(from_peer: PeerId, known_peers: Vec<(PeerId, Vec<String>)>) -> Self {
        Self::JoinAck {
            from_peer,
            known_peers,
        }
    }

    /// Creates a leave message.
    #[must_use]
    pub const fn leave(peer_id: PeerId) -> Self {
        Self::Leave { peer_id }
    }

    /// Returns the message type as a string (for logging/debugging).
    #[must_use]
    pub const fn message_type(&self) -> &'static str {
        match self {
            Self::Ping { .. } => "Ping",
            Self::Pong { .. } => "Pong",
            Self::CapacityAnnounce { .. } => "CapacityAnnounce",
            Self::CapacityRequest { .. } => "CapacityRequest",
            Self::CapacityResponse { .. } => "CapacityResponse",
            Self::Join { .. } => "Join",
            Self::JoinAck { .. } => "JoinAck",
            Self::Leave { .. } => "Leave",
        }
    }

    /// Serializes the message to JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserializes a message from JSON bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
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

    fn make_announcement() -> CapacityAnnouncement {
        let signing_key = SigningKey::generate(&mut OsRng);
        let peer_id = PeerId::from_public_key(&signing_key.verifying_key());
        let mut announcement = CapacityAnnouncement::new(
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

    // ========== Ping/Pong Tests ==========

    #[test]
    fn ping_message_has_nonce() {
        let msg = P2pMessage::ping();
        match msg {
            P2pMessage::Ping { nonce } => {
                // Nonce should be non-zero (with extremely high probability)
                // Actually, just verify it's a u64
                let _ = nonce;
            }
            _ => panic!("Expected Ping message"),
        }
    }

    #[test]
    fn pong_message_echoes_nonce() {
        let nonce = 42u64;
        let msg = P2pMessage::pong(nonce);
        match msg {
            P2pMessage::Pong { nonce: n } => assert_eq!(n, 42),
            _ => panic!("Expected Pong message"),
        }
    }

    #[test]
    fn ping_serialization_roundtrip() {
        let msg = P2pMessage::Ping { nonce: 12345 };
        let bytes = msg.to_bytes().expect("serialization should succeed");
        let deserialized = P2pMessage::from_bytes(&bytes).expect("deserialization should succeed");

        match deserialized {
            P2pMessage::Ping { nonce } => assert_eq!(nonce, 12345),
            _ => panic!("Expected Ping message"),
        }
    }

    #[test]
    fn pong_serialization_roundtrip() {
        let msg = P2pMessage::Pong { nonce: 67890 };
        let bytes = msg.to_bytes().expect("serialization should succeed");
        let deserialized = P2pMessage::from_bytes(&bytes).expect("deserialization should succeed");

        match deserialized {
            P2pMessage::Pong { nonce } => assert_eq!(nonce, 67890),
            _ => panic!("Expected Pong message"),
        }
    }

    // ========== CapacityAnnounce Tests ==========

    #[test]
    fn capacity_announce_creation() {
        let announcement = make_announcement();
        let msg = P2pMessage::capacity_announce(announcement.clone());

        match msg {
            P2pMessage::CapacityAnnounce { announcement: a } => {
                assert_eq!(a.peer_id(), announcement.peer_id());
            }
            _ => panic!("Expected CapacityAnnounce message"),
        }
    }

    #[test]
    fn capacity_announce_serialization_roundtrip() {
        let announcement = make_announcement();
        let msg = P2pMessage::capacity_announce(announcement.clone());

        let bytes = msg.to_bytes().expect("serialization should succeed");
        let deserialized = P2pMessage::from_bytes(&bytes).expect("deserialization should succeed");

        match deserialized {
            P2pMessage::CapacityAnnounce { announcement: a } => {
                assert_eq!(a.peer_id(), announcement.peer_id());
                assert_eq!(a.gpus().len(), announcement.gpus().len());
            }
            _ => panic!("Expected CapacityAnnounce message"),
        }
    }

    // ========== CapacityRequest/Response Tests ==========

    #[test]
    fn capacity_requirements_builder() {
        let reqs = CapacityRequirements::any()
            .with_min_vram(24)
            .with_gpu_model("A100")
            .with_min_gpu_count(4)
            .with_job_type("training")
            .with_max_price(500);

        assert_eq!(reqs.min_vram_gb, Some(24));
        assert_eq!(reqs.gpu_model, Some("A100".to_string()));
        assert_eq!(reqs.min_gpu_count, Some(4));
        assert_eq!(reqs.job_type, Some("training".to_string()));
        assert_eq!(reqs.max_gpu_hour_cents, Some(500));
    }

    #[test]
    fn capacity_requirements_default_is_any() {
        let reqs = CapacityRequirements::default();
        assert_eq!(reqs.min_vram_gb, None);
        assert_eq!(reqs.gpu_model, None);
        assert_eq!(reqs.min_gpu_count, None);
        assert_eq!(reqs.job_type, None);
        assert_eq!(reqs.max_gpu_hour_cents, None);
    }

    #[test]
    fn capacity_request_creation() {
        let reqs = CapacityRequirements::any().with_min_vram(16);
        let msg = P2pMessage::capacity_request(reqs.clone(), 10);

        match msg {
            P2pMessage::CapacityRequest {
                request_id,
                requirements,
                max_results,
            } => {
                // UUID should be valid
                assert!(!request_id.is_nil());
                assert_eq!(requirements.min_vram_gb, Some(16));
                assert_eq!(max_results, 10);
            }
            _ => panic!("Expected CapacityRequest message"),
        }
    }

    #[test]
    fn capacity_request_serialization_roundtrip() {
        let reqs = CapacityRequirements::any()
            .with_min_vram(32)
            .with_job_type("inference");
        let request_id = Uuid::new_v4();
        let msg = P2pMessage::CapacityRequest {
            request_id,
            requirements: reqs,
            max_results: 5,
        };

        let bytes = msg.to_bytes().expect("serialization should succeed");
        let deserialized = P2pMessage::from_bytes(&bytes).expect("deserialization should succeed");

        match deserialized {
            P2pMessage::CapacityRequest {
                request_id: rid,
                requirements,
                max_results,
            } => {
                assert_eq!(rid, request_id);
                assert_eq!(requirements.min_vram_gb, Some(32));
                assert_eq!(requirements.job_type, Some("inference".to_string()));
                assert_eq!(max_results, 5);
            }
            _ => panic!("Expected CapacityRequest message"),
        }
    }

    #[test]
    fn capacity_response_creation() {
        let request_id = Uuid::new_v4();
        let providers = vec![make_announcement(), make_announcement()];
        let msg = P2pMessage::capacity_response(request_id, providers.clone());

        match msg {
            P2pMessage::CapacityResponse {
                request_id: rid,
                providers: p,
            } => {
                assert_eq!(rid, request_id);
                assert_eq!(p.len(), 2);
            }
            _ => panic!("Expected CapacityResponse message"),
        }
    }

    #[test]
    fn capacity_response_serialization_roundtrip() {
        let request_id = Uuid::new_v4();
        let providers = vec![make_announcement()];
        let msg = P2pMessage::capacity_response(request_id, providers);

        let bytes = msg.to_bytes().expect("serialization should succeed");
        let deserialized = P2pMessage::from_bytes(&bytes).expect("deserialization should succeed");

        match deserialized {
            P2pMessage::CapacityResponse {
                request_id: rid,
                providers,
            } => {
                assert_eq!(rid, request_id);
                assert_eq!(providers.len(), 1);
            }
            _ => panic!("Expected CapacityResponse message"),
        }
    }

    // ========== Join/Leave Tests ==========

    #[test]
    fn join_message_creation() {
        let peer_id = make_peer_id();
        let addresses = vec!["/ip4/192.168.1.1/tcp/8080".to_string()];
        let capabilities = vec!["gpu".to_string(), "inference".to_string()];

        let msg = P2pMessage::join(peer_id, addresses.clone(), capabilities.clone());

        match msg {
            P2pMessage::Join {
                peer_id: pid,
                addresses: addrs,
                capabilities: caps,
            } => {
                assert_eq!(pid, peer_id);
                assert_eq!(addrs, addresses);
                assert_eq!(caps, capabilities);
            }
            _ => panic!("Expected Join message"),
        }
    }

    #[test]
    fn join_serialization_roundtrip() {
        let peer_id = make_peer_id();
        let msg = P2pMessage::join(
            peer_id,
            vec!["/ip4/10.0.0.1/tcp/9000".to_string()],
            vec!["cpu".to_string()],
        );

        let bytes = msg.to_bytes().expect("serialization should succeed");
        let deserialized = P2pMessage::from_bytes(&bytes).expect("deserialization should succeed");

        match deserialized {
            P2pMessage::Join {
                peer_id: pid,
                addresses,
                capabilities,
            } => {
                assert_eq!(pid, peer_id);
                assert_eq!(addresses.len(), 1);
                assert_eq!(capabilities, vec!["cpu".to_string()]);
            }
            _ => panic!("Expected Join message"),
        }
    }

    #[test]
    fn join_ack_message_creation() {
        let from_peer = make_peer_id();
        let known_peer1 = make_peer_id();
        let known_peer2 = make_peer_id();
        let known_peers = vec![
            (known_peer1, vec!["/ip4/1.1.1.1/tcp/8000".to_string()]),
            (known_peer2, vec!["/ip4/2.2.2.2/tcp/8000".to_string()]),
        ];

        let msg = P2pMessage::join_ack(from_peer, known_peers.clone());

        match msg {
            P2pMessage::JoinAck {
                from_peer: fp,
                known_peers: kp,
            } => {
                assert_eq!(fp, from_peer);
                assert_eq!(kp.len(), 2);
            }
            _ => panic!("Expected JoinAck message"),
        }
    }

    #[test]
    fn join_ack_serialization_roundtrip() {
        let from_peer = make_peer_id();
        let msg = P2pMessage::join_ack(from_peer, vec![]);

        let bytes = msg.to_bytes().expect("serialization should succeed");
        let deserialized = P2pMessage::from_bytes(&bytes).expect("deserialization should succeed");

        match deserialized {
            P2pMessage::JoinAck {
                from_peer: fp,
                known_peers,
            } => {
                assert_eq!(fp, from_peer);
                assert!(known_peers.is_empty());
            }
            _ => panic!("Expected JoinAck message"),
        }
    }

    #[test]
    fn leave_message_creation() {
        let peer_id = make_peer_id();
        let msg = P2pMessage::leave(peer_id);

        match msg {
            P2pMessage::Leave { peer_id: pid } => assert_eq!(pid, peer_id),
            _ => panic!("Expected Leave message"),
        }
    }

    #[test]
    fn leave_serialization_roundtrip() {
        let peer_id = make_peer_id();
        let msg = P2pMessage::leave(peer_id);

        let bytes = msg.to_bytes().expect("serialization should succeed");
        let deserialized = P2pMessage::from_bytes(&bytes).expect("deserialization should succeed");

        match deserialized {
            P2pMessage::Leave { peer_id: pid } => assert_eq!(pid, peer_id),
            _ => panic!("Expected Leave message"),
        }
    }

    // ========== Message Type Tests ==========

    #[test]
    fn message_type_returns_correct_strings() {
        assert_eq!(P2pMessage::ping().message_type(), "Ping");
        assert_eq!(P2pMessage::pong(0).message_type(), "Pong");
        assert_eq!(
            P2pMessage::capacity_announce(make_announcement()).message_type(),
            "CapacityAnnounce"
        );
        assert_eq!(
            P2pMessage::capacity_request(CapacityRequirements::any(), 10).message_type(),
            "CapacityRequest"
        );
        assert_eq!(
            P2pMessage::capacity_response(Uuid::new_v4(), vec![]).message_type(),
            "CapacityResponse"
        );
        assert_eq!(
            P2pMessage::join(make_peer_id(), vec![], vec![]).message_type(),
            "Join"
        );
        assert_eq!(
            P2pMessage::join_ack(make_peer_id(), vec![]).message_type(),
            "JoinAck"
        );
        assert_eq!(P2pMessage::leave(make_peer_id()).message_type(), "Leave");
    }

    // ========== Error Handling Tests ==========

    #[test]
    fn invalid_json_returns_error() {
        let invalid_bytes = b"not valid json";
        let result = P2pMessage::from_bytes(invalid_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn wrong_message_structure_returns_error() {
        let wrong_structure = r#"{"type":"Unknown","data":123}"#;
        let result = P2pMessage::from_bytes(wrong_structure.as_bytes());
        assert!(result.is_err());
    }

    // ========== Proptest ==========

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn ping_pong_roundtrip(nonce in any::<u64>()) {
                let ping = P2pMessage::Ping { nonce };
                let bytes = ping.to_bytes().unwrap();
                let deserialized = P2pMessage::from_bytes(&bytes).unwrap();
                
                match deserialized {
                    P2pMessage::Ping { nonce: n } => prop_assert_eq!(n, nonce),
                    _ => prop_assert!(false, "Expected Ping"),
                }
            }

            #[test]
            fn capacity_requirements_serialization(
                min_vram in proptest::option::of(1u32..256),
                min_gpu_count in proptest::option::of(1u32..64),
                max_price in proptest::option::of(1u64..1_000_000)
            ) {
                let reqs = CapacityRequirements {
                    min_vram_gb: min_vram,
                    gpu_model: None,
                    min_gpu_count,
                    job_type: None,
                    max_gpu_hour_cents: max_price,
                };
                
                let json = serde_json::to_string(&reqs).unwrap();
                let deserialized: CapacityRequirements = serde_json::from_str(&json).unwrap();
                
                prop_assert_eq!(reqs, deserialized);
            }
        }
    }
}
