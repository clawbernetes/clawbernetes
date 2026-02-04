//! Gateway event types.

use std::time::Duration;
use claw_proto::GatewayMessage;

/// Events emitted by the gateway client.
#[derive(Debug, Clone)]
pub enum GatewayEvent {
    /// Connection established.
    Connected,
    /// Connection lost.
    Disconnected {
        /// Reason for disconnection.
        reason: String,
    },
    /// Attempting reconnection.
    Reconnecting {
        /// Attempt number.
        attempt: u32,
        /// Delay before next attempt.
        delay: Duration,
    },
    /// Message received from gateway.
    Message(GatewayMessage),
    /// Error occurred.
    Error(String),
}

/// Events from the auto-reconnect client.
#[derive(Debug, Clone)]
pub enum AutoReconnectEvent {
    /// Successfully connected.
    Connected,
    /// Disconnected (will attempt reconnection).
    Disconnected { reason: String },
    /// Attempting reconnection.
    Reconnecting { attempt: u32, delay: Duration },
    /// Reconnection failed permanently.
    ReconnectFailed { attempts: u32, last_error: String },
    /// Received a message from the gateway.
    Message(GatewayMessage),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_event_variants() {
        let connected = GatewayEvent::Connected;
        assert!(matches!(connected, GatewayEvent::Connected));

        let disconnected = GatewayEvent::Disconnected {
            reason: "timeout".to_string(),
        };
        if let GatewayEvent::Disconnected { reason } = disconnected {
            assert_eq!(reason, "timeout");
        } else {
            panic!("expected Disconnected");
        }

        let reconnecting = GatewayEvent::Reconnecting {
            attempt: 3,
            delay: Duration::from_secs(4),
        };
        if let GatewayEvent::Reconnecting { attempt, delay } = reconnecting {
            assert_eq!(attempt, 3);
            assert_eq!(delay, Duration::from_secs(4));
        } else {
            panic!("expected Reconnecting");
        }

        let error = GatewayEvent::Error("test error".to_string());
        if let GatewayEvent::Error(msg) = error {
            assert_eq!(msg, "test error");
        } else {
            panic!("expected Error");
        }
    }

    #[test]
    fn test_auto_reconnect_event_connected() {
        let event = AutoReconnectEvent::Connected;
        assert!(matches!(event, AutoReconnectEvent::Connected));
    }

    #[test]
    fn test_auto_reconnect_event_disconnected() {
        let event = AutoReconnectEvent::Disconnected {
            reason: "connection reset".to_string(),
        };
        if let AutoReconnectEvent::Disconnected { reason } = event {
            assert_eq!(reason, "connection reset");
        } else {
            panic!("expected Disconnected");
        }
    }

    #[test]
    fn test_auto_reconnect_event_reconnecting() {
        let event = AutoReconnectEvent::Reconnecting {
            attempt: 3,
            delay: Duration::from_secs(4),
        };
        if let AutoReconnectEvent::Reconnecting { attempt, delay } = event {
            assert_eq!(attempt, 3);
            assert_eq!(delay, Duration::from_secs(4));
        } else {
            panic!("expected Reconnecting");
        }
    }

    #[test]
    fn test_auto_reconnect_event_reconnect_failed() {
        let event = AutoReconnectEvent::ReconnectFailed {
            attempts: 5,
            last_error: "connection refused".to_string(),
        };
        if let AutoReconnectEvent::ReconnectFailed {
            attempts,
            last_error,
        } = event
        {
            assert_eq!(attempts, 5);
            assert_eq!(last_error, "connection refused");
        } else {
            panic!("expected ReconnectFailed");
        }
    }

    #[test]
    fn test_auto_reconnect_event_message() {
        let gateway_msg = GatewayMessage::heartbeat_ack();
        let event = AutoReconnectEvent::Message(gateway_msg);
        assert!(matches!(
            event,
            AutoReconnectEvent::Message(GatewayMessage::HeartbeatAck { .. })
        ));
    }
}
