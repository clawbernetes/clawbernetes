//! Heartbeat/keepalive mechanism.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use claw_proto::{NodeId, NodeMessage};
use tokio::sync::mpsc;

/// Configuration for heartbeat behavior.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Interval between heartbeats.
    pub interval: Duration,
    /// Timeout for waiting on heartbeat ack.
    pub ack_timeout: Duration,
    /// Number of missed acks before considering connection dead.
    pub max_missed_acks: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(30),
            ack_timeout: Duration::from_secs(10),
            max_missed_acks: 3,
        }
    }
}

/// Handle for controlling the heartbeat task.
#[derive(Debug)]
pub struct HeartbeatHandle {
    running: Arc<AtomicBool>,
    missed_acks: Arc<AtomicU32>,
}

impl HeartbeatHandle {
    /// Create a new heartbeat handle.
    pub(crate) fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            missed_acks: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Check if the heartbeat task is running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the number of consecutive missed acks.
    #[must_use]
    pub fn missed_acks(&self) -> u32 {
        self.missed_acks.load(Ordering::SeqCst)
    }

    /// Reset the missed ack counter (call when ack received).
    pub fn ack_received(&self) {
        self.missed_acks.store(0, Ordering::SeqCst);
    }

    /// Stop the heartbeat task.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Start a periodic heartbeat task.
///
/// Returns a handle to control the task and monitor its state.
pub fn start_heartbeat_task(
    node_id: NodeId,
    tx: mpsc::Sender<NodeMessage>,
    config: HeartbeatConfig,
) -> HeartbeatHandle {
    let handle = HeartbeatHandle::new();
    handle.running.store(true, Ordering::SeqCst);

    let running = Arc::clone(&handle.running);
    let missed_acks = Arc::clone(&handle.missed_acks);
    let max_missed = config.max_missed_acks;

    tokio::spawn(async move {
        let mut interval_timer = tokio::time::interval(config.interval);

        while running.load(Ordering::SeqCst) {
            interval_timer.tick().await;

            if !running.load(Ordering::SeqCst) {
                break;
            }

            let msg = NodeMessage::heartbeat(node_id);
            if tx.send(msg).await.is_err() {
                // Channel closed, stop the task
                running.store(false, Ordering::SeqCst);
                break;
            }

            // Increment missed acks counter (will be reset when ack received)
            let current_missed = missed_acks.fetch_add(1, Ordering::SeqCst) + 1;
            if current_missed >= max_missed {
                // Too many missed acks, signal connection may be dead
                running.store(false, Ordering::SeqCst);
                break;
            }
        }
    });

    handle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat_config_default() {
        let config = HeartbeatConfig::default();
        assert_eq!(config.interval, Duration::from_secs(30));
        assert_eq!(config.ack_timeout, Duration::from_secs(10));
        assert_eq!(config.max_missed_acks, 3);
    }

    #[test]
    fn test_heartbeat_config_custom() {
        let config = HeartbeatConfig {
            interval: Duration::from_secs(60),
            ack_timeout: Duration::from_secs(20),
            max_missed_acks: 5,
        };
        assert_eq!(config.interval, Duration::from_secs(60));
        assert_eq!(config.ack_timeout, Duration::from_secs(20));
        assert_eq!(config.max_missed_acks, 5);
    }

    #[test]
    fn test_heartbeat_handle_initial_state() {
        let handle = HeartbeatHandle::new();
        assert!(!handle.is_running());
        assert_eq!(handle.missed_acks(), 0);
    }

    #[test]
    fn test_heartbeat_handle_ack_received() {
        let handle = HeartbeatHandle::new();
        handle.missed_acks.store(5, Ordering::SeqCst);

        handle.ack_received();

        assert_eq!(handle.missed_acks(), 0);
    }

    #[test]
    fn test_heartbeat_handle_stop() {
        let handle = HeartbeatHandle::new();
        handle.running.store(true, Ordering::SeqCst);

        assert!(handle.is_running());

        handle.stop();

        assert!(!handle.is_running());
    }

    #[tokio::test]
    async fn test_heartbeat_task_sends_heartbeats() {
        let node_id = NodeId::new();
        let (tx, mut rx) = mpsc::channel::<NodeMessage>(32);

        let config = HeartbeatConfig {
            interval: Duration::from_millis(10),
            ack_timeout: Duration::from_secs(1),
            max_missed_acks: 100, // High so we don't stop early
        };

        let handle = start_heartbeat_task(node_id, tx, config);

        // Should receive at least one heartbeat
        let msg = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("timeout waiting for heartbeat")
            .expect("channel closed");

        match msg {
            NodeMessage::Heartbeat {
                node_id: recv_id, ..
            } => {
                assert_eq!(recv_id, node_id);
            }
            _ => panic!("expected Heartbeat, got {:?}", msg),
        }

        handle.stop();
    }

    #[tokio::test]
    async fn test_heartbeat_task_stops_on_channel_close() {
        let node_id = NodeId::new();
        let (tx, rx) = mpsc::channel::<NodeMessage>(1);

        let config = HeartbeatConfig {
            interval: Duration::from_millis(5),
            ack_timeout: Duration::from_secs(1),
            max_missed_acks: 100,
        };

        let handle = start_heartbeat_task(node_id, tx, config);

        // Drop receiver to close channel
        drop(rx);

        // Wait a bit for task to notice
        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(!handle.is_running());
    }

    #[tokio::test]
    async fn test_heartbeat_task_stops_on_max_missed_acks() {
        let node_id = NodeId::new();
        let (tx, mut rx) = mpsc::channel::<NodeMessage>(32);

        let config = HeartbeatConfig {
            interval: Duration::from_millis(5),
            ack_timeout: Duration::from_secs(1),
            max_missed_acks: 2,
        };

        let handle = start_heartbeat_task(node_id, tx, config);

        // Drain heartbeats without acknowledging
        for _ in 0..5 {
            let _ = tokio::time::timeout(Duration::from_millis(50), rx.recv()).await;
        }

        // Task should have stopped
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!handle.is_running());
    }
}
