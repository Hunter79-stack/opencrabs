//! Brain Module
//!
//! Dynamic system brain assembly from workspace files, user-defined slash commands,
//! and self-update capabilities.

pub mod commands;
pub mod prompt_builder;
pub mod self_update;

// Re-exports
pub use commands::{CommandLoader, UserCommand};
pub use prompt_builder::BrainLoader;
pub use self_update::SelfUpdater;
