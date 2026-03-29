use crate::hooks::{
    executor::execute_command_hook, CompiledMatcher, HookConfig, HookEvent, HookOutcome,
    HookPayload, HookResult, HookResultControl,
};

use std::path::Path;

pub async fn dispatch_hooks(
    event: &HookEvent,
    hooks: &[HookConfig],
    session_id: &str,
    cwd: &Path,
) -> HookOutcome {
    let payload = HookPayload {
        session_id: session_id.to_string(),
        cwd: cwd.to_path_buf(),
        hook_event: event.clone(),
    };

    let mut additional_contexts = vec![];
    let mut auto_continue = false;

    for hook in hooks {
        if hook.event != event.event_name() || !hook.is_valid_protocol() {
            continue;
        }

        let matcher = match CompiledMatcher::compile(&hook.matcher) {
            Ok(matcher) => matcher,
            Err(err) => {
                warn!(
                    "Skipping hook `{}` for event `{}` because matcher compilation failed: {err}",
                    hook.command, hook.event
                );
                continue;
            }
        };

        if !matcher.matches(event) {
            continue;
        }

        let outcome = execute_command_hook(&payload, &hook.command, hook.timeout).await;
        let HookOutcome { control, result } = outcome;

        match control {
            HookResultControl::Block { reason } => {
                return HookOutcome {
                    control: HookResultControl::Block { reason },
                    result,
                };
            }
            HookResultControl::Continue => {
                if let Some(context) = result.additional_context.filter(|value| !value.is_empty()) {
                    additional_contexts.push(context);
                }
                auto_continue |= result.auto_continue.unwrap_or(false);
            }
        }
    }

    HookOutcome {
        control: HookResultControl::Continue,
        result: HookResult {
            additional_context: (!additional_contexts.is_empty())
                .then(|| additional_contexts.join("\n")),
            auto_continue: auto_continue.then_some(true),
            ..HookResult::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::dispatch_hooks;
    use crate::hooks::{HookConfig, HookEvent, HookResultControl};
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_test_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("aichat-dispatch-tests-{name}-{suffix}"));
        fs::create_dir_all(&dir).expect("create temp dispatch dir");
        dir
    }

    fn pre_tool_use_event(tool_name: &str) -> HookEvent {
        HookEvent::PreToolUse {
            tool_name: tool_name.to_string(),
            tool_input: json!({"command": "pwd"}),
            tool_use_id: "call-1".to_string(),
        }
    }

    fn hook_config(event: &str, command: String) -> HookConfig {
        HookConfig {
            event: event.to_string(),
            matcher: None,
            command,
            timeout: Some(5),
            status_message: None,
            protocol: None,
        }
    }

    #[cfg(unix)]
    fn write_line_command(path: &Path, line: &str) -> String {
        format!("printf '%s\\n' '{line}' >> '{}'", path.display())
    }

    #[cfg(windows)]
    fn write_line_command(path: &Path, line: &str) -> String {
        format!("echo {line}>>{}", path.display())
    }

    #[cfg(unix)]
    fn block_command(path: &Path) -> String {
        format!(
            "printf '%s\\n' 'blocked' > '{}' && echo 'blocked' >&2 && exit 2",
            path.display()
        )
    }

    #[cfg(windows)]
    fn block_command(path: &Path) -> String {
        format!(
            "echo blocked>{} && echo blocked 1>&2 && exit 2",
            path.display()
        )
    }

    #[tokio::test]
    async fn test_dispatch_filters_by_event() {
        let cwd = temp_test_dir("filter-by-event");
        let marker = cwd.join("hook-runs.txt");
        let hooks = vec![
            hook_config("PreToolUse", write_line_command(&marker, "pre-tool")),
            hook_config("SessionStart", write_line_command(&marker, "session-start")),
        ];

        let outcome = dispatch_hooks(&pre_tool_use_event("shell"), &hooks, "session-1", &cwd).await;

        assert!(matches!(outcome.control, HookResultControl::Continue));
        let contents = fs::read_to_string(&marker).expect("read marker file");
        assert_eq!(contents.trim(), "pre-tool");
    }

    #[tokio::test]
    async fn test_dispatch_block_short_circuit() {
        let cwd = temp_test_dir("block-short-circuit");
        let blocked_marker = cwd.join("blocked.txt");
        let second_marker = cwd.join("second.txt");
        let hooks = vec![
            hook_config("PreToolUse", block_command(&blocked_marker)),
            hook_config("PreToolUse", write_line_command(&second_marker, "second")),
        ];

        let outcome = dispatch_hooks(&pre_tool_use_event("shell"), &hooks, "session-2", &cwd).await;

        match outcome.control {
            HookResultControl::Block { reason } => assert_eq!(reason, "blocked"),
            HookResultControl::Continue => panic!("expected blocked hook outcome"),
        }
        assert!(blocked_marker.exists());
        assert!(!second_marker.exists());
    }
}
