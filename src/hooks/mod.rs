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
//! - Hooks marked with `async: true` run in the background and can only append context
//!   or request a follow-up turn; they never block or deny the active operation
//!
//! ## Supported Events
//! SessionStart, SessionEnd, UserPromptSubmit, Stop, StopFailure,
//! PreToolUse, PostToolUse, PostToolUseFailure, InstructionsLoaded, CwdChanged
//!
//! ## Resume
//! Stop hooks can return {"resume": true, "additionalContext": "..."} to
//! trigger another LLM turn. max_resume prevents infinite loops.
//! Async hook results are drained before each LLM turn. Results without
//! `resume: true` are queued until the next user-initiated turn, while results
//! with `resume: true` can trigger an immediate follow-up turn when the session
//! loop checks for completed async work.
//!
//! Compatible with Claude Code, Cursor, GitHub Copilot, and Gemini CLI.

pub mod async_manager;
pub mod config;
pub mod dispatch;
pub mod executor;
pub mod matcher;
pub mod persistent;
pub mod types;

pub use async_manager::AsyncHookManager;
#[allow(unused_imports)]
pub use config::{HookConfig, HooksConfig};
#[allow(unused_imports)]
pub use dispatch::{
    dispatch_hooks, dispatch_hooks_with_count, dispatch_hooks_with_count_and_manager,
    dispatch_hooks_with_manager, dispatch_hooks_with_managers,
};
#[allow(unused_imports)]
pub use executor::execute_command_hook;
pub use matcher::CompiledMatcher;
#[allow(unused_imports)]
pub use persistent::PersistentHookManager;
pub use types::*;
