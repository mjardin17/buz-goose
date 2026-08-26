#![deny(unsafe_code)]
#![warn(missing_docs)]
//! Bounded integration between the Buzz control plane and the Goose runtime.
//!
//! Goose remains an external runtime. This crate invokes its documented CLI;
//! Buzz supplies policy preflight, verification, and evidence around the call.

mod execution;

pub use execution::{
    AgentGenome, ArtifactStore, Capability, ExecutionEnvelope, ExecutionEvent, ExecutionRecord,
    ExecutionState, FileArtifactStore, GooseHealth, GooseRuntime, GooseRuntimeConfig, Permission,
    RepositoryHealthRequest, VerificationResult,
};
