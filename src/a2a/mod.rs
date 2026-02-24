//! A2A (Agent-to-Agent) Protocol implementation for OpenCrabs.
//!
//! Implements the A2A Protocol RC v1.0 specification:
//! - Agent Card discovery (`.well-known/agent.json`)
//! - JSON-RPC 2.0 task API (`message/send`, `tasks/get`, `tasks/cancel`)
//! - HTTP gateway server (axum)
//!
//! This is the world's first Rust A2A implementation.

pub mod types;
pub mod agent_card;
pub mod handler;
pub mod server;
pub mod debate;
