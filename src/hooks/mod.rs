//! # Hooks System
//!
//! Claude Code-compatible hooks for aichat. External scripts can observe and
//! influence the LLM conversation lifecycle via subprocess hooks.
//!
//! ## Protocol
//! - Hooks receive JSON on stdin (HookPayload with session_id, cwd, hook_event_name, etc.)
//! - Exit code 0: stdout parsed as JSON (HookResult) or plain text (additional_context)
//! - Exit code 2: block the operation (stderr = reason)
//! - Other exit codes: non-blocking warning
//!
//! ## Supported Events
//! SessionStart, SessionEnd, UserPromptSubmit, Stop, StopFailure,
//! PreToolUse, PostToolUse, PostToolUseFailure, InstructionsLoaded, CwdChanged
//!
//! ## Resume
//! Stop hooks can return {"resume": true, "additionalContext": "..."} to
//! trigger another LLM turn. max_resume prevents infinite loops.
//!
//! Compatible with Claude Code, Cursor, GitHub Copilot, and Gemini CLI.

pub mod config;
pub mod dispatch;
pub mod executor;
pub mod matcher;
pub mod types;

#[allow(unused_imports)]
pub use config::{HookConfig, HooksConfig};
#[allow(unused_imports)]
pub use dispatch::{dispatch_hooks, dispatch_hooks_with_count};
#[allow(unused_imports)]
pub use executor::execute_command_hook;
pub use matcher::CompiledMatcher;
pub use types::*;
