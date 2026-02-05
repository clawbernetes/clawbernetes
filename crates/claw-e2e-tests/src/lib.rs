//! End-to-end integration tests for Clawbernetes.
//!
//! These tests exercise the full stack:
//! - Gateway server startup and shutdown
//! - CLI client connections
//! - Node registration and heartbeats
//! - Workload lifecycle
//! - MOLT integration

#![cfg(test)]
