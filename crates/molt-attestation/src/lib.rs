//! # molt-attestation
//!
//! Hardware and execution attestation for the MOLT network.
//!
//! This crate provides:
//!
//! - Hardware attestation — GPU verification, capability proofs
//! - Execution attestation — job completion proofs, checkpoint verification
//! - Verification — unified verification functions for all attestation types
//!
//! ## Quick Start
//!
//! ### Hardware Attestation
//!
//! ```rust
//! use molt_attestation::hardware::{HardwareAttestation, GpuInfo};
//! use molt_attestation::verification::verify_hardware_attestation;
//! use chrono::Duration;
//! use ed25519_dalek::SigningKey;
//! use uuid::Uuid;
//!
//! // Create a keypair for signing
//! let mut secret = [0u8; 32];
//! rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut secret);
//! let signing_key = SigningKey::from_bytes(&secret);
//! let verifying_key = signing_key.verifying_key();
//!
//! // Create GPU info
//! let gpu = GpuInfo {
//!     vendor: molt_attestation::GpuVendor::Nvidia,
//!     model: "RTX 4090".to_string(),
//!     vram_mb: 24576,
//!     compute_capability: "8.9".to_string(),
//! };
//!
//! // Create and sign attestation
//! let attestation = HardwareAttestation::create_and_sign(
//!     Uuid::new_v4(),
//!     vec![gpu],
//!     Duration::hours(24),
//!     &signing_key,
//! ).unwrap();
//!
//! // Verify the attestation
//! let result = verify_hardware_attestation(&attestation, &verifying_key).unwrap();
//! assert!(result.valid);
//! ```
//!
//! ### Execution Attestation
//!
//! ```rust
//! use molt_attestation::execution::{CheckpointChain, ExecutionAttestation, ExecutionMetrics};
//! use molt_attestation::verification::verify_execution_attestation;
//! use chrono::Duration;
//! use ed25519_dalek::SigningKey;
//! use uuid::Uuid;
//!
//! // Create a keypair
//! let mut secret = [0u8; 32];
//! rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut secret);
//! let signing_key = SigningKey::from_bytes(&secret);
//! let verifying_key = signing_key.verifying_key();
//!
//! // Build checkpoint chain during execution
//! let mut chain = CheckpointChain::new(b"initial state");
//! chain.add_checkpoint(b"after step 1");
//! chain.add_checkpoint(b"final state");
//!
//! // Create attestation
//! let attestation = ExecutionAttestation::create_and_sign(
//!     Uuid::new_v4(),
//!     chain.into_checkpoints(),
//!     Duration::seconds(3600),
//!     ExecutionMetrics::default(),
//!     &signing_key,
//! ).unwrap();
//!
//! // Verify
//! let result = verify_execution_attestation(&attestation, &verifying_key).unwrap();
//! assert!(result.valid);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod execution;
pub mod hardware;
pub mod verification;

pub use error::AttestationError;

// Re-export commonly used types
pub use execution::{Checkpoint, CheckpointChain, ExecutionAttestation, ExecutionMetrics};
pub use hardware::{AttestationChain, AttestationEntry, GpuInfo, GpuVendor, HardwareAttestation};
pub use verification::{
    batch_verify_execution, batch_verify_hardware, verify_execution_attestation,
    verify_execution_with_data, verify_hardware_attestation, VerificationDetails,
    VerificationResult,
};
