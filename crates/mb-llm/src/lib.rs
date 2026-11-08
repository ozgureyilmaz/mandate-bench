//! Shared provider client, wire codecs, request limits and retry policy.

pub mod budget;
pub mod client;
pub mod config;
pub mod limits;
pub mod retry;
pub mod types;
pub mod wire;

pub use budget::{BudgetGuard, MAX_BUDGETED_BODY_BYTES, MAX_INPUT_CONTEXT_TOKENS};
pub use client::Client;
pub use config::{estimate_cost, load_config};
pub use types::*;
pub use wire::{build_request, parse_response, tool_result};
