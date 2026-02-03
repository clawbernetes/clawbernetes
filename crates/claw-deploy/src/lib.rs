//! # claw-deploy
//!
//! Intent-based deployment system for Clawbernetes.
//!
//! This crate replaces traditional YAML-based deployment tools (`ArgoCD`, Helm)
//! with a natural language intent system that automatically selects and executes
//! appropriate deployment strategies.
//!
//! ## Features
//!
//! - **Natural Language Parsing**: Parse deployment intents from human-readable commands
//! - **Automatic Strategy Selection**: Choose between Immediate, Canary, Blue-Green, or Rolling
//! - **Metrics-Based Decisions**: Promote or rollback based on real-time health metrics
//! - **No YAML Required**: Define deployments through intent, not configuration
//!
//! ## Example
//!
//! ```rust
//! use claw_deploy::{DeploymentIntent, DeploymentStrategy, StrategyHint};
//!
//! // Create an intent programmatically
//! let intent = DeploymentIntent::new("myapp:v2.0")
//!     .with_replicas(5)
//!     .with_gpus(2)
//!     .with_strategy_hint(StrategyHint::Canary { percentage: 10 });
//!
//! // Validate the intent
//! intent.validate().expect("valid intent");
//! ```
//!
//! ## Modules
//!
//! - [`types`]: Core deployment types (Intent, Strategy, State, Status)
//! - [`error`]: Error types and results
//! - [`parser`]: Natural language intent parsing
//! - [`strategy`]: Strategy selection logic
//! - [`executor`]: Deployment execution
//! - [`monitor`]: Health monitoring and assessment

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod executor;
pub mod monitor;
pub mod parser;
pub mod strategy;
pub mod types;

// Re-export main types for convenience
pub use error::{DeployError, DeployResult};
pub use executor::DeploymentExecutor;
pub use monitor::{DeploymentMonitor, HealthAssessment, MetricPoint};
pub use parser::{parse_intent, IntentKeyword};
pub use strategy::{select_strategy, ClusterContext};
pub use types::{
    DeploymentConstraints, DeploymentId, DeploymentIntent, DeploymentState, DeploymentStatus,
    DeploymentStrategy, Environment, StrategyHint,
};
