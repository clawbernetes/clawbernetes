//! Gossip node trait and state management.

use super::{
    BroadcastConfig, BroadcastResult, CapacityAnnouncement, GossipBroadcaster, GossipMessage,
    MessageId, QueryFilter,
};
use crate::error::P2pError;
use crate::protocol::PeerId;
use std::future::Future;
use std::pin::Pin;

/// Events emitted by gossip nodes.
#[derive(Debug, Clone)]
pub enum GossipEvent {
    /// A new capacity announcement was received and validated.
    AnnouncementReceived {
        /// The announcement.
        announcement: CapacityAnnouncement,
        /// The peer that sent it to us.
        from_peer: PeerId,
    },

    /// A query was received that we should respond to.
    QueryReceived {
        /// The query ID for correlation.
        query_id: MessageId,
        /// The peer that originated the query.
        from_peer: PeerId,
        /// The filter criteria.
        filter: QueryFilter,
    },

    /// A response to our query was received.
    ResponseReceived {
        /// The query ID this responds to.
        query_id: MessageId,
        /// The peer that responded.
        from_peer: PeerId,
        /// Matching announcements.
        announcements: Vec<CapacityAnnouncement>,
    },

    /// A peer disconnected or timed out.
    PeerDisconnected {
        /// The disconnected peer.
        peer_id: PeerId,
    },

    /// A peer connected.
    PeerConnected {
        /// The connected peer.
        peer_id: PeerId,
    },
}

/// State of a gossip node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    /// Node is initializing.
    Initializing,
    /// Node is running and participating in gossip.
    Running,
    /// Node is syncing with peers.
    Syncing,
    /// Node is shutting down.
    ShuttingDown,
    /// Node is stopped.
    Stopped,
}

impl NodeState {
    /// Returns true if the node can process gossip messages.
    #[must_use]
    pub const fn can_process(&self) -> bool {
        matches!(self, Self::Running | Self::Syncing)
    }

    /// Returns true if the node is active (not stopped/shutting down).
    #[must_use]
    pub const fn is_active(&self) -> bool {
        !matches!(self, Self::ShuttingDown | Self::Stopped)
    }
}

/// Boxed future type for async trait methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Trait for nodes participating in the gossip network.
///
/// This trait defines the interface for sending and receiving gossip messages.
/// Implementations handle the actual network transport (QUIC, TCP, etc.).
pub trait GossipNode: Send + Sync {
    /// Returns the node's peer ID.
    fn peer_id(&self) -> PeerId;

    /// Returns the current node state.
    fn state(&self) -> NodeState;

    /// Sends a gossip message to a specific peer.
    ///
    /// # Errors
    ///
    /// Returns an error if the send fails.
    fn send_to<'a>(
        &'a self,
        peer_id: PeerId,
        message: GossipMessage,
    ) -> BoxFuture<'a, Result<(), P2pError>>;

    /// Broadcasts a message to multiple peers.
    ///
    /// # Errors
    ///
    /// Returns an error if the broadcast fails.
    fn broadcast<'a>(
        &'a self,
        peers: &'a [PeerId],
        message: GossipMessage,
    ) -> BoxFuture<'a, Result<Vec<PeerId>, P2pError>>;

    /// Receives the next gossip event.
    ///
    /// Returns `None` if the node is shutting down.
    fn recv_event<'a>(&'a self) -> BoxFuture<'a, Option<GossipEvent>>;

    /// Announces capacity to the network.
    ///
    /// # Errors
    ///
    /// Returns an error if the announcement fails.
    fn announce<'a>(
        &'a self,
        announcement: CapacityAnnouncement,
    ) -> BoxFuture<'a, Result<MessageId, P2pError>>;

    /// Queries the network for matching capacity.
    ///
    /// # Errors
    ///
    /// Returns an error if the query fails.
    fn query<'a>(
        &'a self,
        filter: QueryFilter,
        max_results: u32,
    ) -> BoxFuture<'a, Result<MessageId, P2pError>>;
}

/// A local gossip node implementation using the broadcaster.
#[derive(Debug)]
pub struct LocalGossipNode {
    /// The broadcaster handling message propagation.
    broadcaster: GossipBroadcaster,
    /// Current node state.
    state: NodeState,
    /// Pending outbound messages (peer_id, message).
    pending_sends: Vec<(PeerId, GossipMessage)>,
    /// Pending events to emit.
    pending_events: Vec<GossipEvent>,
}

impl LocalGossipNode {
    /// Creates a new local gossip node.
    #[must_use]
    pub fn new(peer_id: PeerId, config: BroadcastConfig) -> Self {
        Self {
            broadcaster: GossipBroadcaster::new(peer_id, config),
            state: NodeState::Initializing,
            pending_sends: Vec::new(),
            pending_events: Vec::new(),
        }
    }

    /// Creates a node with default configuration.
    #[must_use]
    pub fn with_defaults(peer_id: PeerId) -> Self {
        Self::new(peer_id, BroadcastConfig::default())
    }

    /// Returns the node's peer ID.
    #[must_use]
    pub fn peer_id(&self) -> PeerId {
        self.broadcaster.local_peer_id()
    }

    /// Returns the current state.
    #[must_use]
    pub const fn state(&self) -> NodeState {
        self.state
    }

    /// Sets the node state.
    pub fn set_state(&mut self, state: NodeState) {
        self.state = state;
    }

    /// Starts the node.
    pub fn start(&mut self) {
        self.state = NodeState::Running;
    }

    /// Stops the node.
    pub fn stop(&mut self) {
        self.state = NodeState::Stopped;
    }

    /// Returns a reference to the broadcaster.
    #[must_use]
    pub const fn broadcaster(&self) -> &GossipBroadcaster {
        &self.broadcaster
    }

    /// Returns a mutable reference to the broadcaster.
    pub fn broadcaster_mut(&mut self) -> &mut GossipBroadcaster {
        &mut self.broadcaster
    }

    /// Adds a peer to the node.
    pub fn add_peer(&mut self, peer_id: PeerId) {
        self.broadcaster.add_peer(peer_id);
        self.pending_events.push(GossipEvent::PeerConnected { peer_id });
    }

    /// Removes a peer from the node.
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.broadcaster.remove_peer(peer_id);
        self.pending_events
            .push(GossipEvent::PeerDisconnected { peer_id: *peer_id });
    }

    /// Handles an incoming gossip message.
    ///
    /// # Errors
    ///
    /// Returns an error if message handling fails.
    pub fn handle_message(
        &mut self,
        message: &GossipMessage,
        from_peer: PeerId,
    ) -> Result<BroadcastResult, P2pError> {
        if !self.state.can_process() {
            return Err(P2pError::Gossip(format!(
                "Node cannot process messages in state {:?}",
                self.state
            )));
        }

        let result = self.broadcaster.handle_message(message, from_peer)?;

        // Emit events based on message type
        if !result.was_duplicate {
            match message {
                GossipMessage::Announce { announcement, .. } => {
                    self.pending_events.push(GossipEvent::AnnouncementReceived {
                        announcement: announcement.clone(),
                        from_peer,
                    });
                }
                GossipMessage::Query(query) => {
                    self.pending_events.push(GossipEvent::QueryReceived {
                        query_id: query.query_id,
                        from_peer: query.from_peer,
                        filter: query.filter.clone(),
                    });
                }
                GossipMessage::Response {
                    query_id,
                    from_peer,
                    announcements,
                } => {
                    self.pending_events.push(GossipEvent::ResponseReceived {
                        query_id: *query_id,
                        from_peer: *from_peer,
                        announcements: announcements.clone(),
                    });
                }
                _ => {}
            }

            // Queue forwarding to target peers
            for peer in &result.target_peers {
                self.queue_send(*peer, message.clone());
            }
        }

        Ok(result)
    }

    /// Prepares an announcement for broadcast.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not running.
    pub fn prepare_announce(
        &mut self,
        announcement: CapacityAnnouncement,
    ) -> Result<BroadcastResult, P2pError> {
        if !self.state.can_process() {
            return Err(P2pError::Gossip(format!(
                "Node cannot announce in state {:?}",
                self.state
            )));
        }

        let result = self.broadcaster.prepare_announce(announcement.clone());
        let ttl = self.broadcaster.config().max_ttl_hops;
        let message = GossipMessage::Announce {
            message_id: result.message_id,
            announcement,
            ttl_hops: ttl,
        };

        for peer in &result.target_peers {
            self.queue_send(*peer, message.clone());
        }

        Ok(result)
    }

    /// Prepares a query for broadcast.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not running.
    pub fn prepare_query(
        &mut self,
        filter: QueryFilter,
        max_results: u32,
    ) -> Result<MessageId, P2pError> {
        if !self.state.can_process() {
            return Err(P2pError::Gossip(format!(
                "Node cannot query in state {:?}",
                self.state
            )));
        }

        let ttl = self.broadcaster.config().max_ttl_hops;
        let message = GossipMessage::query(self.peer_id(), filter, max_results, ttl);
        let query_id = message.message_id().ok_or_else(|| {
            P2pError::Gossip("Failed to get query ID".to_string())
        })?;

        // Mark as seen so we don't process our own query
        // This is handled internally when we handle the message
        let peers = self.broadcaster.known_peers();
        for peer in peers {
            self.queue_send(peer, message.clone());
        }

        Ok(query_id)
    }

    /// Sends a response to a query.
    ///
    /// # Errors
    ///
    /// Returns an error if the node is not running.
    pub fn send_response(
        &mut self,
        query_id: MessageId,
        to_peer: PeerId,
        announcements: Vec<CapacityAnnouncement>,
    ) -> Result<(), P2pError> {
        if !self.state.can_process() {
            return Err(P2pError::Gossip(format!(
                "Node cannot respond in state {:?}",
                self.state
            )));
        }

        let message = GossipMessage::response(query_id, self.peer_id(), announcements);
        self.queue_send(to_peer, message);
        Ok(())
    }

    /// Queries the local cache for matching announcements.
    #[must_use]
    pub fn query_local(&self, filter: &QueryFilter, max_results: u32) -> Vec<CapacityAnnouncement> {
        self.broadcaster.query_cache(filter, max_results)
    }

    /// Takes the next pending send, if any.
    #[must_use]
    pub fn take_pending_send(&mut self) -> Option<(PeerId, GossipMessage)> {
        if self.pending_sends.is_empty() {
            None
        } else {
            Some(self.pending_sends.remove(0))
        }
    }

    /// Takes all pending sends.
    #[must_use]
    pub fn take_all_pending_sends(&mut self) -> Vec<(PeerId, GossipMessage)> {
        std::mem::take(&mut self.pending_sends)
    }

    /// Takes the next pending event, if any.
    #[must_use]
    pub fn take_pending_event(&mut self) -> Option<GossipEvent> {
        if self.pending_events.is_empty() {
            None
        } else {
            Some(self.pending_events.remove(0))
        }
    }

    /// Takes all pending events.
    #[must_use]
    pub fn take_all_pending_events(&mut self) -> Vec<GossipEvent> {
        std::mem::take(&mut self.pending_events)
    }

    /// Returns the number of pending sends.
    #[must_use]
    pub fn pending_send_count(&self) -> usize {
        self.pending_sends.len()
    }

    /// Returns the number of pending events.
    #[must_use]
    pub fn pending_event_count(&self) -> usize {
        self.pending_events.len()
    }

    /// Queues a message to be sent.
    fn queue_send(&mut self, peer_id: PeerId, message: GossipMessage) {
        self.pending_sends.push((peer_id, message));
    }

    /// Runs periodic maintenance (cleanup, etc.).
    pub fn tick(&mut self) {
        self.broadcaster.cleanup();
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

    fn make_signed_announcement(signing_key: &SigningKey) -> CapacityAnnouncement {
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
        announcement.sign(signing_key);
        announcement
    }

    // ========== NodeState Tests ==========

    #[test]
    fn node_state_can_process() {
        assert!(!NodeState::Initializing.can_process());
        assert!(NodeState::Running.can_process());
        assert!(NodeState::Syncing.can_process());
        assert!(!NodeState::ShuttingDown.can_process());
        assert!(!NodeState::Stopped.can_process());
    }

    #[test]
    fn node_state_is_active() {
        assert!(NodeState::Initializing.is_active());
        assert!(NodeState::Running.is_active());
        assert!(NodeState::Syncing.is_active());
        assert!(!NodeState::ShuttingDown.is_active());
        assert!(!NodeState::Stopped.is_active());
    }

    // ========== LocalGossipNode Creation Tests ==========

    #[test]
    fn local_node_creation() {
        let peer_id = make_peer_id();
        let node = LocalGossipNode::with_defaults(peer_id);

        assert_eq!(node.peer_id(), peer_id);
        assert_eq!(node.state(), NodeState::Initializing);
    }

    #[test]
    fn local_node_with_config() {
        let peer_id = make_peer_id();
        let config = BroadcastConfig::small_network();
        let node = LocalGossipNode::new(peer_id, config);

        assert_eq!(node.broadcaster().config().fanout, 2);
    }

    #[test]
    fn local_node_start_stop() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);

        assert_eq!(node.state(), NodeState::Initializing);

        node.start();
        assert_eq!(node.state(), NodeState::Running);

        node.stop();
        assert_eq!(node.state(), NodeState::Stopped);
    }

    #[test]
    fn local_node_set_state() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);

        node.set_state(NodeState::Syncing);
        assert_eq!(node.state(), NodeState::Syncing);
    }

    // ========== Peer Management Tests ==========

    #[test]
    fn local_node_add_peer_emits_event() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);

        let other_peer = make_peer_id();
        node.add_peer(other_peer);

        let event = node.take_pending_event();
        assert!(matches!(event, Some(GossipEvent::PeerConnected { peer_id: p }) if p == other_peer));
    }

    #[test]
    fn local_node_remove_peer_emits_event() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);

        let other_peer = make_peer_id();
        node.add_peer(other_peer);
        let _ = node.take_pending_event(); // Clear connect event

        node.remove_peer(&other_peer);

        let event = node.take_pending_event();
        assert!(
            matches!(event, Some(GossipEvent::PeerDisconnected { peer_id: p }) if p == other_peer)
        );
    }

    // ========== Message Handling Tests ==========

    #[test]
    fn handle_message_requires_running_state() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);
        // Node is in Initializing state

        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);
        let message = GossipMessage::announce(announcement, 3);

        let result = node.handle_message(&message, make_peer_id());
        assert!(result.is_err());
    }

    #[test]
    fn handle_announce_emits_event() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);
        node.start();

        let other_peer = make_peer_id();
        node.add_peer(other_peer);
        let _ = node.take_pending_event(); // Clear connect event

        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);
        let message = GossipMessage::announce(announcement.clone(), 3);

        node.handle_message(&message, other_peer).ok();

        let event = node.take_pending_event();
        assert!(matches!(
            event,
            Some(GossipEvent::AnnouncementReceived { .. })
        ));
    }

    #[test]
    fn handle_query_emits_event() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);
        node.start();

        let other_peer = make_peer_id();
        node.add_peer(other_peer);
        let _ = node.take_pending_event();

        let querier = make_peer_id();
        let message = GossipMessage::query(querier, QueryFilter::any(), 10, 3);

        node.handle_message(&message, other_peer).ok();

        let event = node.take_pending_event();
        assert!(matches!(event, Some(GossipEvent::QueryReceived { .. })));
    }

    #[test]
    fn handle_response_emits_event() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);
        node.start();

        let other_peer = make_peer_id();
        node.add_peer(other_peer);
        let _ = node.take_pending_event();

        let query_id = MessageId::new();
        let message = GossipMessage::response(query_id, other_peer, vec![]);

        node.handle_message(&message, other_peer).ok();

        let event = node.take_pending_event();
        assert!(matches!(event, Some(GossipEvent::ResponseReceived { .. })));
    }

    #[test]
    fn handle_message_queues_forwards() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);
        node.start();

        let peer1 = make_peer_id();
        let peer2 = make_peer_id();
        node.add_peer(peer1);
        node.add_peer(peer2);
        let _ = node.take_all_pending_events();

        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);
        let message = GossipMessage::announce(announcement, 3);

        let result = node.handle_message(&message, peer1).ok();
        assert!(result.is_some());

        // Should have queued forwards to peer2 (excluding sender peer1)
        assert!(node.pending_send_count() > 0);
    }

    // ========== Prepare Announce Tests ==========

    #[test]
    fn prepare_announce_requires_running() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);

        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);

        let result = node.prepare_announce(announcement);
        assert!(result.is_err());
    }

    #[test]
    fn prepare_announce_queues_sends() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);
        node.start();

        let other_peer = make_peer_id();
        node.add_peer(other_peer);
        let _ = node.take_all_pending_events();

        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);

        let result = node.prepare_announce(announcement);
        assert!(result.is_ok());
        assert!(node.pending_send_count() > 0);
    }

    // ========== Prepare Query Tests ==========

    #[test]
    fn prepare_query_requires_running() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);

        let result = node.prepare_query(QueryFilter::any(), 10);
        assert!(result.is_err());
    }

    #[test]
    fn prepare_query_queues_sends() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);
        node.start();

        let other_peer = make_peer_id();
        node.add_peer(other_peer);
        let _ = node.take_all_pending_events();

        let result = node.prepare_query(QueryFilter::any(), 10);
        assert!(result.is_ok());
        assert!(node.pending_send_count() > 0);
    }

    // ========== Send Response Tests ==========

    #[test]
    fn send_response_requires_running() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);

        let result = node.send_response(MessageId::new(), make_peer_id(), vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn send_response_queues_send() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);
        node.start();

        let other_peer = make_peer_id();
        let result = node.send_response(MessageId::new(), other_peer, vec![]);

        assert!(result.is_ok());
        assert_eq!(node.pending_send_count(), 1);

        let (target, _msg) = node.take_pending_send().expect("should have send");
        assert_eq!(target, other_peer);
    }

    // ========== Query Local Tests ==========

    #[test]
    fn query_local_returns_cached() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);
        node.start();

        let other_peer = make_peer_id();
        node.add_peer(other_peer);

        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);
        let message = GossipMessage::announce(announcement, 3);

        node.handle_message(&message, other_peer).ok();

        let results = node.query_local(&QueryFilter::any(), 10);
        assert_eq!(results.len(), 1);
    }

    // ========== Pending Queue Tests ==========

    #[test]
    fn take_all_pending_sends() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);
        node.start();

        let peer1 = make_peer_id();
        let peer2 = make_peer_id();
        node.add_peer(peer1);
        node.add_peer(peer2);

        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);
        node.prepare_announce(announcement).ok();

        let sends = node.take_all_pending_sends();
        assert!(!sends.is_empty());
        assert_eq!(node.pending_send_count(), 0);
    }

    #[test]
    fn take_all_pending_events() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);

        node.add_peer(make_peer_id());
        node.add_peer(make_peer_id());

        let events = node.take_all_pending_events();
        assert_eq!(events.len(), 2);
        assert_eq!(node.pending_event_count(), 0);
    }

    // ========== Tick Tests ==========

    #[test]
    fn tick_runs_cleanup() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);

        // Just verify tick doesn't panic
        node.tick();
    }

    // ========== Integration Tests ==========

    #[test]
    fn full_announce_flow() {
        let peer_id = make_peer_id();
        let mut node = LocalGossipNode::with_defaults(peer_id);
        node.start();

        // Add peers
        let peers: Vec<_> = (0..5).map(|_| make_peer_id()).collect();
        for p in &peers {
            node.add_peer(*p);
        }
        let _ = node.take_all_pending_events();

        // Announce
        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);
        let result = node.prepare_announce(announcement.clone());

        assert!(result.is_ok());
        let broadcast_result = result.unwrap();
        assert!(!broadcast_result.was_duplicate);
        assert!(!broadcast_result.target_peers.is_empty());

        // Verify sends are queued
        let sends = node.take_all_pending_sends();
        assert!(!sends.is_empty());
        assert!(sends.len() <= node.broadcaster().config().fanout);
    }

    #[test]
    fn full_query_response_flow() {
        let local_id = make_peer_id();
        let mut local_node = LocalGossipNode::with_defaults(local_id);
        local_node.start();

        let remote_id = make_peer_id();
        let mut remote_node = LocalGossipNode::with_defaults(remote_id);
        remote_node.start();

        // Connect nodes
        local_node.add_peer(remote_id);
        remote_node.add_peer(local_id);
        let _ = local_node.take_all_pending_events();
        let _ = remote_node.take_all_pending_events();

        // Remote has an announcement
        let signing_key = SigningKey::generate(&mut OsRng);
        let announcement = make_signed_announcement(&signing_key);
        let announce_msg = GossipMessage::announce(announcement.clone(), 3);
        remote_node.handle_message(&announce_msg, local_id).ok();
        let _ = remote_node.take_all_pending_events();

        // Local sends query
        let query_id = local_node.prepare_query(QueryFilter::any(), 10).unwrap();

        // Take the query message
        let (_, query_msg) = local_node.take_pending_send().unwrap();

        // Remote receives query
        remote_node.handle_message(&query_msg, local_id).ok();

        // Remote should have received query event
        let event = remote_node.take_pending_event().unwrap();
        match event {
            GossipEvent::QueryReceived {
                query_id: qid,
                filter,
                ..
            } => {
                assert_eq!(qid, query_id);

                // Remote queries local cache and responds
                let results = remote_node.query_local(&filter, 10);
                assert_eq!(results.len(), 1);

                remote_node.send_response(qid, local_id, results).ok();
            }
            _ => panic!("Expected QueryReceived event"),
        }

        // Take response message
        let (_, response_msg) = remote_node.take_pending_send().unwrap();

        // Local receives response
        local_node.handle_message(&response_msg, remote_id).ok();

        // Local should have response event
        let event = local_node.take_pending_event().unwrap();
        assert!(matches!(event, GossipEvent::ResponseReceived { announcements, .. } if !announcements.is_empty()));
    }

    // ========== Proptest ==========

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn node_handles_multiple_messages(num_msgs in 1usize..20) {
                let peer_id = make_peer_id();
                let mut node = LocalGossipNode::with_defaults(peer_id);
                node.start();

                let sender = make_peer_id();
                node.add_peer(sender);

                for _ in 0..num_msgs {
                    let signing_key = SigningKey::generate(&mut OsRng);
                    let announcement = make_signed_announcement(&signing_key);
                    let message = GossipMessage::announce(announcement, 3);

                    let result = node.handle_message(&message, sender);
                    prop_assert!(result.is_ok());
                }

                prop_assert!(node.pending_event_count() >= num_msgs);
            }

            #[test]
            fn query_local_respects_max_results(
                num_announcements in 1usize..10,
                max_results in 1u32..5
            ) {
                let peer_id = make_peer_id();
                let mut node = LocalGossipNode::with_defaults(peer_id);
                node.start();

                let sender = make_peer_id();
                node.add_peer(sender);

                for _ in 0..num_announcements {
                    let signing_key = SigningKey::generate(&mut OsRng);
                    let announcement = make_signed_announcement(&signing_key);
                    let message = GossipMessage::announce(announcement, 3);
                    node.handle_message(&message, sender).ok();
                }

                let results = node.query_local(&QueryFilter::any(), max_results);
                prop_assert!(results.len() <= max_results as usize);
            }
        }
    }
}
