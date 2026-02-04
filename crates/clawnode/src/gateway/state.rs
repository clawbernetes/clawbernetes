//! Connection state types.

use std::sync::atomic::{AtomicU32, Ordering};

/// State of the gateway connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected.
    Disconnected,
    /// Attempting to connect.
    Connecting,
    /// Connected and registered.
    Connected,
    /// Connection failed, will retry.
    Reconnecting,
    /// Permanently failed (max retries exceeded).
    Failed,
}

/// Atomic wrapper for connection state.
#[derive(Debug)]
pub struct AtomicConnectionState(AtomicU32);

impl AtomicConnectionState {
    /// Create a new atomic state.
    #[must_use]
    pub const fn new(state: ConnectionState) -> Self {
        Self(AtomicU32::new(state as u32))
    }

    /// Load the current state.
    #[must_use]
    pub fn load(&self) -> ConnectionState {
        match self.0.load(Ordering::SeqCst) {
            0 => ConnectionState::Disconnected,
            1 => ConnectionState::Connecting,
            2 => ConnectionState::Connected,
            3 => ConnectionState::Reconnecting,
            _ => ConnectionState::Failed,
        }
    }

    /// Store a new state.
    pub fn store(&self, state: ConnectionState) {
        self.0.store(state as u32, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_state_enum() {
        assert_eq!(ConnectionState::Disconnected as u32, 0);
        assert_eq!(ConnectionState::Connecting as u32, 1);
        assert_eq!(ConnectionState::Connected as u32, 2);
        assert_eq!(ConnectionState::Reconnecting as u32, 3);
        assert_eq!(ConnectionState::Failed as u32, 4);
    }

    #[test]
    fn test_atomic_connection_state() {
        let state = AtomicConnectionState::new(ConnectionState::Disconnected);
        assert_eq!(state.load(), ConnectionState::Disconnected);

        state.store(ConnectionState::Connecting);
        assert_eq!(state.load(), ConnectionState::Connecting);

        state.store(ConnectionState::Connected);
        assert_eq!(state.load(), ConnectionState::Connected);
    }
}
