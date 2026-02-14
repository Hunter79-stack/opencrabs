//! Telegram Bot Integration
//!
//! Runs a Telegram bot alongside the TUI, forwarding messages from
//! allowlisted users to the AgentService and replying with responses.

mod agent;
mod handler;

pub use agent::TelegramAgent;
