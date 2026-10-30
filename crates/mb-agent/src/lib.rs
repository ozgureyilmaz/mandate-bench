//! Offline episode driver. Provider-driven turns are deferred to Milestone 2.
//! This crate receives visible task data and calls, never a hidden Reference.

use anyhow::Result;
use mb_core::{Task, ToolCall, initial_messages};
use mb_env::Environment;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct Transcript {
    pub messages: Value,
    pub tool_calls: Vec<mb_env::ToolRecord>,
    pub final_output: String,
}

pub struct Episode {
    pub environment: Environment,
    pub transcript: Transcript,
}

pub fn replay(task: Task, calls: &[ToolCall], final_output: String) -> Result<Episode> {
    let messages = initial_messages(&task)?;
    let mut environment = Environment::new(task);
    for call in calls {
        environment.execute(call.clone());
    }
    let transcript = Transcript {
        messages,
        tool_calls: environment.ledger.clone(),
        final_output,
    };
    Ok(Episode {
        environment,
        transcript,
    })
}
