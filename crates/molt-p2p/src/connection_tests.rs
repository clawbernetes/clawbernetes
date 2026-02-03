//! Tests for P2P connection management.

use super::*;
use crate::message::P2pMessage;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

fn make_peer_id() -> PeerId {
    let signing_key = SigningKey::generate(&mut OsRng);
    PeerId::from_public_key(&signing_key.verifying_key())
}

fn make_socket_addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, port as u8)), port)
}

fn make_connection(port: u16) -> PeerConnection {
    PeerConnection::new(make_peer_id(), make_socket_addr(port))
}

// ========== ConnectionState Tests ==========

#[test]
fn connection_state_is_usable() {
    assert!(!ConnectionState::Connecting.is_usable());
    assert!(ConnectionState::Connected.is_usable());
    assert!(!ConnectionState::Disconnecting.is_usable());
    assert!(!ConnectionState::Disconnected.is_usable());
    assert!(!ConnectionState::Failed.is_usable());
}

#[test]
fn connection_state_is_terminal() {
    assert!(!ConnectionState::Connecting.is_terminal());
    assert!(!ConnectionState::Connected.is_terminal());
    assert!(!ConnectionState::Disconnecting.is_terminal());
    assert!(ConnectionState::Disconnected.is_terminal());
    assert!(ConnectionState::Failed.is_terminal());
}

// ========== ConnectionHealth Tests ==========

#[test]
fn connection_health_new() {
    let health = ConnectionHealth::new();
    assert!(health.last_rtt.is_none());
    assert_eq!(health.successful_pings, 0);
    assert_eq!(health.failed_pings, 0);
    assert_eq!(health.messages_sent, 0);
    assert_eq!(health.messages_received, 0);
}

#[test]
fn connection_health_record_ping_success() {
    let mut health = ConnectionHealth::new();
    let rtt = Duration::from_millis(50);

    health.record_ping_success(rtt);

    assert_eq!(health.last_rtt, Some(rtt));
    assert_eq!(health.successful_pings, 1);
    assert_eq!(health.failed_pings, 0);
}

#[test]
fn connection_health_record_ping_failure() {
    let mut health = ConnectionHealth::new();

    health.record_ping_failure();

    assert!(health.last_rtt.is_none());
    assert_eq!(health.successful_pings, 0);
    assert_eq!(health.failed_pings, 1);
}

#[test]
fn connection_health_ping_success_rate() {
    let mut health = ConnectionHealth::new();

    // No pings yet - assume 100% healthy
    assert!((health.ping_success_rate() - 1.0).abs() < f64::EPSILON);

    // 3 successes, 1 failure = 75%
    health.record_ping_success(Duration::from_millis(10));
    health.record_ping_success(Duration::from_millis(10));
    health.record_ping_success(Duration::from_millis(10));
    health.record_ping_failure();

    assert!((health.ping_success_rate() - 0.75).abs() < f64::EPSILON);
}

#[test]
fn connection_health_message_tracking() {
    let mut health = ConnectionHealth::new();

    health.record_message_sent();
    health.record_message_sent();
    health.record_message_received();

    assert_eq!(health.messages_sent, 2);
    assert_eq!(health.messages_received, 1);
}

#[test]
fn connection_health_is_healthy() {
    let mut health = ConnectionHealth::new();
    let max_silence = Duration::from_secs(60);

    // Fresh connection should be healthy
    assert!(health.is_healthy(max_silence));

    // After many failures, should be unhealthy
    for _ in 0..10 {
        health.record_ping_failure();
    }
    health.record_ping_success(Duration::from_millis(10)); // Touch last_seen
    assert!(!health.is_healthy(max_silence)); // < 50% success rate
}

// ========== PeerConnection Tests ==========

#[test]
fn peer_connection_new() {
    let peer_id = make_peer_id();
    let addr = make_socket_addr(8080);

    let conn = PeerConnection::new(peer_id, addr);

    assert_eq!(conn.peer_id(), peer_id);
    assert_eq!(conn.remote_addr(), addr);
    assert_eq!(conn.state(), ConnectionState::Connecting);
    assert!(conn.established_at().is_none());
    assert!(conn.error_message().is_none());
}

#[test]
fn peer_connection_mark_connected() {
    let mut conn = make_connection(8080);

    assert_eq!(conn.state(), ConnectionState::Connecting);
    assert!(conn.established_at().is_none());

    conn.mark_connected();

    assert_eq!(conn.state(), ConnectionState::Connected);
    assert!(conn.established_at().is_some());
    assert!(conn.is_usable());
}

#[test]
fn peer_connection_mark_disconnecting() {
    let mut conn = make_connection(8080);

    // Disconnecting from Connecting state does nothing
    conn.mark_disconnecting();
    assert_eq!(conn.state(), ConnectionState::Connecting);

    // Disconnecting from Connected state works
    conn.mark_connected();
    conn.mark_disconnecting();
    assert_eq!(conn.state(), ConnectionState::Disconnecting);
}

#[test]
fn peer_connection_mark_disconnected() {
    let mut conn = make_connection(8080);
    conn.mark_connected();
    conn.mark_disconnecting();
    conn.mark_disconnected();

    assert_eq!(conn.state(), ConnectionState::Disconnected);
    assert!(!conn.is_usable());
}

#[test]
fn peer_connection_mark_failed() {
    let mut conn = make_connection(8080);

    conn.mark_failed("Connection refused");

    assert_eq!(conn.state(), ConnectionState::Failed);
    assert_eq!(conn.error_message(), Some("Connection refused"));
    assert!(!conn.is_usable());
}

#[test]
fn peer_connection_health_tracking() {
    let mut conn = make_connection(8080);
    conn.mark_connected();

    conn.record_ping_success(Duration::from_millis(50));
    conn.record_message_sent();
    conn.record_message_received();

    assert_eq!(conn.health().successful_pings, 1);
    assert_eq!(conn.health().messages_sent, 1);
    assert_eq!(conn.health().messages_received, 1);
}

#[test]
fn peer_connection_duration() {
    let mut conn = make_connection(8080);

    // Not established yet
    assert!(conn.connection_duration().is_none());

    conn.mark_connected();
    std::thread::sleep(Duration::from_millis(10));

    let duration = conn.connection_duration();
    assert!(duration.is_some());
    assert!(duration.unwrap() >= Duration::from_millis(10));
}

// ========== ConnectionPoolConfig Tests ==========

#[test]
fn connection_pool_config_default() {
    let config = ConnectionPoolConfig::default();

    assert_eq!(config.max_connections, 100);
    assert_eq!(config.connect_timeout, Duration::from_secs(10));
    assert_eq!(config.ping_interval, Duration::from_secs(30));
    assert_eq!(config.max_silence, Duration::from_secs(90));
}

#[test]
fn connection_pool_config_builder() {
    let config = ConnectionPoolConfig::new(50)
        .with_connect_timeout(Duration::from_secs(5))
        .with_ping_interval(Duration::from_secs(15))
        .with_max_silence(Duration::from_secs(45));

    assert_eq!(config.max_connections, 50);
    assert_eq!(config.connect_timeout, Duration::from_secs(5));
    assert_eq!(config.ping_interval, Duration::from_secs(15));
    assert_eq!(config.max_silence, Duration::from_secs(45));
}

// ========== ConnectionPool Tests ==========

#[test]
fn connection_pool_new() {
    let pool = ConnectionPool::with_defaults();

    assert!(pool.is_empty());
    assert_eq!(pool.len(), 0);
    assert_eq!(pool.active_count(), 0);
}

#[test]
fn connection_pool_add_connection() {
    let mut pool = ConnectionPool::with_defaults();
    let conn = make_connection(8080);
    let peer_id = conn.peer_id();

    pool.add_connection(conn).expect("should add connection");

    assert_eq!(pool.len(), 1);
    assert!(pool.contains(&peer_id));
}

#[test]
fn connection_pool_add_duplicate_fails() {
    let mut pool = ConnectionPool::with_defaults();
    let peer_id = make_peer_id();
    let addr = make_socket_addr(8080);

    let conn1 = PeerConnection::new(peer_id, addr);
    let conn2 = PeerConnection::new(peer_id, addr);

    pool.add_connection(conn1).expect("first add should succeed");
    let result = pool.add_connection(conn2);

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), P2pError::Connection(_)));
}

#[test]
fn connection_pool_max_connections() {
    let config = ConnectionPoolConfig::new(2);
    let mut pool = ConnectionPool::new(config);

    pool.add_connection(make_connection(8081))
        .expect("first add should succeed");
    pool.add_connection(make_connection(8082))
        .expect("second add should succeed");

    assert!(pool.is_full());

    let result = pool.add_connection(make_connection(8083));
    assert!(result.is_err());
}

#[test]
fn connection_pool_get_and_get_mut() {
    let mut pool = ConnectionPool::with_defaults();
    let conn = make_connection(8080);
    let peer_id = conn.peer_id();

    pool.add_connection(conn).expect("should add connection");

    // Get immutable reference
    let conn_ref = pool.get(&peer_id);
    assert!(conn_ref.is_some());
    assert_eq!(conn_ref.unwrap().state(), ConnectionState::Connecting);

    // Get mutable reference and modify
    let conn_mut = pool.get_mut(&peer_id);
    assert!(conn_mut.is_some());
    conn_mut.unwrap().mark_connected();

    // Verify modification
    assert_eq!(
        pool.get(&peer_id).unwrap().state(),
        ConnectionState::Connected
    );
}

#[test]
fn connection_pool_remove() {
    let mut pool = ConnectionPool::with_defaults();
    let conn = make_connection(8080);
    let peer_id = conn.peer_id();

    pool.add_connection(conn).expect("should add connection");
    assert!(pool.contains(&peer_id));

    let removed = pool.remove(&peer_id);
    assert!(removed.is_some());
    assert!(!pool.contains(&peer_id));
    assert!(pool.is_empty());
}

#[test]
fn connection_pool_active_count() {
    let mut pool = ConnectionPool::with_defaults();

    let mut conn1 = make_connection(8081);
    let mut conn2 = make_connection(8082);
    let conn3 = make_connection(8083);

    conn1.mark_connected();
    conn2.mark_connected();
    // conn3 is still Connecting

    pool.add_connection(conn1).expect("add conn1");
    pool.add_connection(conn2).expect("add conn2");
    pool.add_connection(conn3).expect("add conn3");

    assert_eq!(pool.len(), 3);
    assert_eq!(pool.active_count(), 2);
}

#[test]
fn connection_pool_peer_ids() {
    let mut pool = ConnectionPool::with_defaults();

    let conn1 = make_connection(8081);
    let conn2 = make_connection(8082);
    let id1 = conn1.peer_id();
    let id2 = conn2.peer_id();

    pool.add_connection(conn1).expect("add conn1");
    pool.add_connection(conn2).expect("add conn2");

    let ids = pool.peer_ids();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id2));
}

#[test]
fn connection_pool_active_connections() {
    let mut pool = ConnectionPool::with_defaults();

    let mut conn1 = make_connection(8081);
    let mut conn2 = make_connection(8082);
    let mut conn3 = make_connection(8083);

    conn1.mark_connected();
    conn2.mark_failed("error");
    conn3.mark_connected();

    pool.add_connection(conn1).expect("add");
    pool.add_connection(conn2).expect("add");
    pool.add_connection(conn3).expect("add");

    let active = pool.active_connections();
    assert_eq!(active.len(), 2);
}

#[test]
fn connection_pool_connections_in_state() {
    let mut pool = ConnectionPool::with_defaults();

    let mut conn1 = make_connection(8081);
    let mut conn2 = make_connection(8082);
    let conn3 = make_connection(8083);

    conn1.mark_connected();
    conn2.mark_connected();
    // conn3 is Connecting

    pool.add_connection(conn1).expect("add");
    pool.add_connection(conn2).expect("add");
    pool.add_connection(conn3).expect("add");

    let connected = pool.connections_in_state(ConnectionState::Connected);
    let connecting = pool.connections_in_state(ConnectionState::Connecting);

    assert_eq!(connected.len(), 2);
    assert_eq!(connecting.len(), 1);
}

#[test]
fn connection_pool_cleanup_terminal() {
    let mut pool = ConnectionPool::with_defaults();

    let mut conn1 = make_connection(8081);
    let mut conn2 = make_connection(8082);
    let mut conn3 = make_connection(8083);

    conn1.mark_connected();
    conn2.mark_failed("error");
    conn3.mark_connected();
    conn3.mark_disconnecting();
    conn3.mark_disconnected();

    pool.add_connection(conn1).expect("add");
    pool.add_connection(conn2).expect("add");
    pool.add_connection(conn3).expect("add");

    assert_eq!(pool.len(), 3);

    let removed = pool.cleanup_terminal();
    assert_eq!(removed, 2); // conn2 (Failed) and conn3 (Disconnected)
    assert_eq!(pool.len(), 1);
}

#[test]
fn connection_pool_iter() {
    let mut pool = ConnectionPool::with_defaults();

    pool.add_connection(make_connection(8081)).expect("add");
    pool.add_connection(make_connection(8082)).expect("add");

    let count = pool.iter().count();
    assert_eq!(count, 2);
}

#[test]
fn connection_pool_iter_mut() {
    let mut pool = ConnectionPool::with_defaults();

    pool.add_connection(make_connection(8081)).expect("add");
    pool.add_connection(make_connection(8082)).expect("add");

    // Mark all as connected using iter_mut
    for (_, conn) in pool.iter_mut() {
        conn.mark_connected();
    }

    assert_eq!(pool.active_count(), 2);
}

// ========== SharedConnectionPool Tests ==========

#[tokio::test]
async fn shared_connection_pool_basic_operations() {
    let pool = SharedConnectionPool::with_defaults();

    assert!(pool.is_empty().await);

    let conn = make_connection(8080);
    let peer_id = conn.peer_id();

    pool.add_connection(conn).await.expect("should add");

    assert_eq!(pool.len().await, 1);
    assert!(pool.contains(&peer_id).await);

    pool.remove(&peer_id).await;
    assert!(pool.is_empty().await);
}

#[tokio::test]
async fn shared_connection_pool_concurrent_access() {
    let pool = SharedConnectionPool::with_defaults();

    let pool1 = pool.clone();
    let pool2 = pool.clone();

    let handle1 = tokio::spawn(async move {
        for i in 0..10 {
            let conn = make_connection(8000 + i);
            let _ = pool1.add_connection(conn).await;
        }
    });

    let handle2 = tokio::spawn(async move {
        for _ in 0..50 {
            let _ = pool2.len().await;
            tokio::task::yield_now().await;
        }
    });

    handle1.await.expect("task 1 should complete");
    handle2.await.expect("task 2 should complete");

    // Should have added some connections (exact count depends on timing)
    assert!(pool.len().await > 0);
}

#[tokio::test]
async fn shared_connection_pool_cleanup_terminal() {
    let pool = SharedConnectionPool::with_defaults();

    let mut conn1 = make_connection(8081);
    let mut conn2 = make_connection(8082);

    conn1.mark_connected();
    conn2.mark_failed("test error");

    pool.add_connection(conn1).await.expect("add");
    pool.add_connection(conn2).await.expect("add");

    assert_eq!(pool.len().await, 2);

    let removed = pool.cleanup_terminal().await;
    assert_eq!(removed, 1);
    assert_eq!(pool.len().await, 1);
}

// ========== MessageChannel Tests ==========

#[tokio::test]
async fn message_channel_send_recv() {
    let (channel_a, mut channel_b) = MessageChannel::connected_pair();

    // Send from A, receive on B
    let msg = P2pMessage::ping();
    channel_a.send(msg).await.expect("send should succeed");

    let received = channel_b.recv().await.expect("recv should succeed");
    assert_eq!(received.message_type(), "Ping");
}

#[tokio::test]
async fn message_channel_bidirectional() {
    let (mut channel_a, mut channel_b) = MessageChannel::connected_pair();

    // A -> B
    channel_a
        .send(P2pMessage::ping())
        .await
        .expect("a->b send");
    let from_a = channel_b.recv().await.expect("b recv");
    assert_eq!(from_a.message_type(), "Ping");

    // B -> A (respond with Pong)
    if let P2pMessage::Ping { nonce } = from_a {
        channel_b
            .send(P2pMessage::pong(nonce))
            .await
            .expect("b->a send");
    }

    let from_b = channel_a.recv().await.expect("a recv");
    assert_eq!(from_b.message_type(), "Pong");
}

#[tokio::test]
async fn message_channel_try_recv_empty() {
    let (_channel_a, mut channel_b) = MessageChannel::connected_pair();

    // No messages sent, try_recv should return None
    let result = channel_b.try_recv().expect("try_recv should not error");
    assert!(result.is_none());
}

#[tokio::test]
async fn message_channel_try_recv_has_message() {
    let (channel_a, mut channel_b) = MessageChannel::connected_pair();

    channel_a.send(P2pMessage::ping()).await.expect("send");

    // Give a moment for the message to be delivered
    tokio::task::yield_now().await;

    let result = channel_b.try_recv().expect("try_recv should not error");
    assert!(result.is_some());
    assert_eq!(result.unwrap().message_type(), "Ping");
}

#[tokio::test]
async fn message_channel_closed_on_drop() {
    let (channel_a, mut channel_b) = MessageChannel::connected_pair();

    // Drop channel_a
    drop(channel_a);

    // Receiving should fail with channel closed
    let result = channel_b.recv().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn message_channel_multiple_messages() {
    let (channel_a, mut channel_b) = MessageChannel::connected_pair();

    // Send multiple messages
    for i in 0..5 {
        channel_a
            .send(P2pMessage::Ping { nonce: i })
            .await
            .expect("send");
    }

    // Receive all messages in order
    for i in 0..5 {
        let msg = channel_b.recv().await.expect("recv");
        match msg {
            P2pMessage::Ping { nonce } => assert_eq!(nonce, i),
            _ => panic!("Expected Ping"),
        }
    }
}

#[tokio::test]
async fn message_channel_is_closed() {
    let (channel_a, channel_b) = MessageChannel::connected_pair();

    assert!(!channel_a.is_closed());
    assert!(!channel_b.is_closed());

    drop(channel_b);

    // channel_a's sender is closed because its receiver (which was in channel_b) was dropped
    assert!(channel_a.is_closed());
}

// ========== ActiveConnection Tests ==========

#[tokio::test]
async fn active_connection_send_recv() {
    let peer_a = make_peer_id();
    let peer_b = make_peer_id();
    let addr_a = make_socket_addr(8080);
    let addr_b = make_socket_addr(8081);

    let (mut conn_a, mut conn_b) =
        ActiveConnection::connected_pair(peer_a, addr_a, peer_b, addr_b);

    // Both should be in Connected state
    assert_eq!(conn_a.state(), ConnectionState::Connected);
    assert_eq!(conn_b.state(), ConnectionState::Connected);

    // Send message from A to B
    conn_a.send(P2pMessage::ping()).await.expect("send");
    let msg = conn_b.recv().await.expect("recv");
    assert_eq!(msg.message_type(), "Ping");

    // Check health tracking
    assert_eq!(conn_a.health().messages_sent, 1);
    assert_eq!(conn_b.health().messages_received, 1);
}

#[tokio::test]
async fn active_connection_send_fails_when_not_connected() {
    let (channel_a, _channel_b) = MessageChannel::connected_pair();
    let conn = PeerConnection::new(make_peer_id(), make_socket_addr(8080));
    // Note: conn is in Connecting state, not Connected

    let mut active = ActiveConnection::new(conn, channel_a);

    let result = active.send(P2pMessage::ping()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn active_connection_recv_fails_when_not_connected() {
    let (_channel_a, channel_b) = MessageChannel::connected_pair();
    let conn = PeerConnection::new(make_peer_id(), make_socket_addr(8080));

    let mut active = ActiveConnection::new(conn, channel_b);

    let result = active.recv().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn active_connection_ping_pong_flow() {
    let peer_a = make_peer_id();
    let peer_b = make_peer_id();
    let addr_a = make_socket_addr(8080);
    let addr_b = make_socket_addr(8081);

    let (mut conn_a, mut conn_b) =
        ActiveConnection::connected_pair(peer_a, addr_a, peer_b, addr_b);

    // A sends ping
    let ping = P2pMessage::ping();
    let sent_nonce = match &ping {
        P2pMessage::Ping { nonce } => *nonce,
        _ => panic!("Expected Ping"),
    };
    conn_a.send(ping).await.expect("send ping");

    // B receives and responds with pong
    let received = conn_b.recv().await.expect("recv ping");
    if let P2pMessage::Ping { nonce } = received {
        conn_b
            .send(P2pMessage::pong(nonce))
            .await
            .expect("send pong");
    } else {
        panic!("Expected Ping");
    }

    // A receives pong
    let pong = conn_a.recv().await.expect("recv pong");
    match pong {
        P2pMessage::Pong { nonce } => assert_eq!(nonce, sent_nonce),
        _ => panic!("Expected Pong"),
    }

    // Verify message counts
    assert_eq!(conn_a.health().messages_sent, 1);
    assert_eq!(conn_a.health().messages_received, 1);
    assert_eq!(conn_b.health().messages_sent, 1);
    assert_eq!(conn_b.health().messages_received, 1);
}

#[tokio::test]
async fn active_connection_try_recv() {
    let peer_a = make_peer_id();
    let peer_b = make_peer_id();
    let addr_a = make_socket_addr(8080);
    let addr_b = make_socket_addr(8081);

    let (mut conn_a, mut conn_b) =
        ActiveConnection::connected_pair(peer_a, addr_a, peer_b, addr_b);

    // Nothing to receive yet
    let result = conn_b.try_recv().expect("try_recv should not error");
    assert!(result.is_none());

    // Send a message
    conn_a.send(P2pMessage::ping()).await.expect("send");
    tokio::task::yield_now().await;

    // Now there should be a message
    let result = conn_b.try_recv().expect("try_recv");
    assert!(result.is_some());
    assert_eq!(result.unwrap().message_type(), "Ping");
}

#[tokio::test]
async fn active_connection_state_transitions() {
    let (channel_a, _channel_b) = MessageChannel::connected_pair();
    let conn = PeerConnection::new(make_peer_id(), make_socket_addr(8080));
    let mut active = ActiveConnection::new(conn, channel_a);

    assert_eq!(active.state(), ConnectionState::Connecting);

    active.mark_connected();
    assert_eq!(active.state(), ConnectionState::Connected);
    assert!(active.is_usable());

    active.mark_disconnecting();
    assert_eq!(active.state(), ConnectionState::Disconnecting);
    assert!(!active.is_usable());

    active.mark_disconnected();
    assert_eq!(active.state(), ConnectionState::Disconnected);
}

#[tokio::test]
async fn active_connection_mark_failed() {
    let (channel_a, _channel_b) = MessageChannel::connected_pair();
    let conn = PeerConnection::new(make_peer_id(), make_socket_addr(8080));
    let mut active = ActiveConnection::new(conn, channel_a);

    active.mark_connected();
    active.mark_failed("Connection lost");

    assert_eq!(active.state(), ConnectionState::Failed);
    assert!(!active.is_usable());
}

#[tokio::test]
async fn active_connection_accessors() {
    let peer_id = make_peer_id();
    let addr = make_socket_addr(8080);
    let (channel, _) = MessageChannel::connected_pair();
    let conn = PeerConnection::new(peer_id, addr);

    let active = ActiveConnection::new(conn, channel);

    assert_eq!(active.peer_id(), peer_id);
    assert_eq!(active.remote_addr(), addr);
}

#[tokio::test]
async fn active_connection_concurrent_send() {
    let peer_a = make_peer_id();
    let peer_b = make_peer_id();
    let addr_a = make_socket_addr(8080);
    let addr_b = make_socket_addr(8081);

    let (conn_a, mut conn_b) = ActiveConnection::connected_pair(peer_a, addr_a, peer_b, addr_b);

    // Wrap conn_a in Arc for sharing (need interior mutability for send)
    let conn_a = Arc::new(tokio::sync::Mutex::new(conn_a));

    let send_count = 10;
    let mut handles = Vec::new();

    for i in 0..send_count {
        let conn = Arc::clone(&conn_a);
        handles.push(tokio::spawn(async move {
            let mut guard = conn.lock().await;
            guard.send(P2pMessage::Ping { nonce: i }).await
        }));
    }

    // Wait for all sends
    for handle in handles {
        handle.await.expect("task").expect("send");
    }

    // Receive all messages
    let mut received = 0;
    while let Ok(Some(_)) = conn_b.try_recv() {
        received += 1;
    }

    // Might need to drain with recv
    while received < send_count {
        if conn_b.recv().await.is_ok() {
            received += 1;
        } else {
            break;
        }
    }

    assert_eq!(received, send_count);
}

// ========== Proptest ==========

mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn connection_health_ping_rate_bounds(
            successes in 0u64..1000,
            failures in 0u64..1000
        ) {
            let mut health = ConnectionHealth::new();

            for _ in 0..successes {
                health.record_ping_success(Duration::from_millis(10));
            }
            for _ in 0..failures {
                health.record_ping_failure();
            }

            let rate = health.ping_success_rate();
            prop_assert!(rate >= 0.0 && rate <= 1.0);
        }

        #[test]
        fn connection_pool_add_up_to_max(max_conns in 1usize..20) {
            let config = ConnectionPoolConfig::new(max_conns);
            let mut pool = ConnectionPool::new(config);

            for i in 0..max_conns {
                let result = pool.add_connection(make_connection(8000 + i as u16));
                prop_assert!(result.is_ok());
            }

            prop_assert!(pool.is_full());
            prop_assert_eq!(pool.len(), max_conns);
        }

        #[test]
        fn connection_state_transitions_valid(
            should_connect in any::<bool>(),
            should_fail in any::<bool>()
        ) {
            let mut conn = make_connection(8080);
            prop_assert_eq!(conn.state(), ConnectionState::Connecting);

            if should_fail {
                conn.mark_failed("test");
                prop_assert_eq!(conn.state(), ConnectionState::Failed);
            } else if should_connect {
                conn.mark_connected();
                prop_assert_eq!(conn.state(), ConnectionState::Connected);

                conn.mark_disconnecting();
                prop_assert_eq!(conn.state(), ConnectionState::Disconnecting);

                conn.mark_disconnected();
                prop_assert_eq!(conn.state(), ConnectionState::Disconnected);
            }
        }
    }
}
