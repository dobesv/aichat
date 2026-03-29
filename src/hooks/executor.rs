use crate::hooks::{HookOutcome, HookPayload, HookResult, HookResultControl};

use std::io::{ErrorKind, Read};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub async fn execute_command_hook(
    payload: &HookPayload,
    command: &str,
    timeout_secs: Option<u64>,
) -> HookOutcome {
    let event_name = payload.hook_event.event_name();
    debug!(
        "Dispatching hook for event '{}': command='{}'",
        event_name, command
    );
    let started_at = std::time::Instant::now();

    let shell = default_shell();
    let shell_arg = default_shell_arg();

    let mut child = match Command::new(&shell)
        .arg(shell_arg)
        .arg(command)
        .current_dir(&payload.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            warn!("Failed to spawn hook command `{command}`: {err}");
            return continue_with_default();
        }
    };

    let payload_json = match serde_json::to_string(payload) {
        Ok(payload_json) => payload_json,
        Err(err) => {
            warn!("Failed to serialize hook payload for `{command}`: {err}");
            return continue_with_default();
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(err) = std::io::Write::write_all(&mut stdin, payload_json.as_bytes()) {
            if err.kind() != ErrorKind::BrokenPipe {
                warn!("Failed to write hook payload to `{command}` stdin: {err}");
                return continue_with_default();
            }
        }
    }

    let output = match wait_for_output(child, timeout_secs, command).await {
        Some(output) => output,
        None => return continue_with_default(),
    };

    let elapsed = started_at.elapsed().as_millis();
    let exit_code = output.status.code().unwrap_or(-1);
    debug!(
        "Hook for '{}' completed: exit_code={}, duration={}ms",
        event_name, exit_code, elapsed
    );

    match output.status.code() {
        Some(0) => parse_success_output(&output.stdout),
        Some(2) => HookOutcome {
            control: HookResultControl::Block {
                reason: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            },
            result: HookResult::default(),
        },
        Some(code) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.is_empty() {
                warn!("Hook command `{command}` exited with status {code}");
            } else {
                warn!("Hook command `{command}` exited with status {code}: {stderr}");
            }
            continue_with_default()
        }
        None => {
            warn!("Hook command `{command}` terminated without an exit code");
            continue_with_default()
        }
    }
}

fn parse_success_output(stdout: &[u8]) -> HookOutcome {
    if stdout.is_empty() {
        return continue_with_default();
    }

    match serde_json::from_slice::<HookResult>(stdout) {
        Ok(result) => HookOutcome {
            control: HookResultControl::Continue,
            result,
        },
        Err(_) => {
            let text = String::from_utf8_lossy(stdout).trim().to_string();
            if text.is_empty() {
                continue_with_default()
            } else {
                HookOutcome {
                    control: HookResultControl::Continue,
                    result: HookResult {
                        additional_context: Some(text),
                        ..HookResult::default()
                    },
                }
            }
        }
    }
}

fn continue_with_default() -> HookOutcome {
    HookOutcome {
        control: HookResultControl::Continue,
        result: HookResult::default(),
    }
}

async fn wait_for_output(
    mut child: Child,
    timeout_secs: Option<u64>,
    command: &str,
) -> Option<Output> {
    let timeout = timeout_secs.map(Duration::from_secs);
    let started_at = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => match collect_output(&mut child, status) {
                Ok(output) => return Some(output),
                Err(err) => {
                    warn!("Failed reading output from hook command `{command}`: {err}");
                    return None;
                }
            },
            Ok(None) => {
                if let Some(timeout) = timeout {
                    if started_at.elapsed() >= timeout {
                        warn!(
                            "Hook command `{command}` timed out after {}s",
                            timeout.as_secs()
                        );
                        if let Err(err) = child.kill() {
                            warn!("Failed to kill timed out hook command `{command}`: {err}");
                        }
                        let _ = child.wait();
                        return None;
                    }
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Err(err) => {
                warn!("Failed waiting for hook command `{command}`: {err}");
                return None;
            }
        }
    }
}

fn collect_output(child: &mut Child, status: std::process::ExitStatus) -> std::io::Result<Output> {
    let mut stdout = vec![];
    if let Some(mut handle) = child.stdout.take() {
        handle.read_to_end(&mut stdout)?;
    }

    let mut stderr = vec![];
    if let Some(mut handle) = child.stderr.take() {
        handle.read_to_end(&mut stderr)?;
    }

    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(unix)]
pub(crate) fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}

#[cfg(windows)]
pub(crate) fn default_shell() -> String {
    "cmd".to_string()
}

#[cfg(unix)]
pub(crate) fn default_shell_arg() -> &'static str {
    "-c"
}

#[cfg(windows)]
pub(crate) fn default_shell_arg() -> &'static str {
    "/C"
}

#[cfg(test)]
mod tests {
    use super::execute_command_hook;
    use crate::hooks::{HookEvent, HookPayload, HookResultControl};
    use serde_json::json;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn test_payload(cwd: &Path) -> HookPayload {
        HookPayload {
            session_id: "session-123".to_string(),
            cwd: cwd.to_path_buf(),
            resume_count: 0,
            hook_event: HookEvent::PreToolUse {
                tool_name: "shell".to_string(),
                tool_input: json!({"command": "pwd"}),
                tool_use_id: "call-1".to_string(),
            },
        }
    }

    fn temp_test_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("aichat-hook-tests-{name}-{suffix}"));
        fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[cfg(unix)]
    fn success_json_command() -> &'static str {
        "echo '{}'"
    }

    #[cfg(windows)]
    fn success_json_command() -> &'static str {
        "echo {}"
    }

    #[cfg(unix)]
    fn plain_text_command() -> &'static str {
        "echo 'hello world'"
    }

    #[cfg(windows)]
    fn plain_text_command() -> &'static str {
        "echo hello world"
    }

    #[cfg(unix)]
    fn exit_2_command() -> &'static str {
        "echo 'blocked' >&2; exit 2"
    }

    #[cfg(windows)]
    fn exit_2_command() -> &'static str {
        "echo blocked 1>&2 && exit 2"
    }

    #[cfg(unix)]
    fn timeout_command() -> &'static str {
        "sleep 60"
    }

    #[cfg(windows)]
    fn timeout_command() -> &'static str {
        "powershell -Command \"Start-Sleep -Seconds 60\""
    }

    #[cfg(unix)]
    fn command_not_found() -> &'static str {
        "/nonexistent/hook"
    }

    #[cfg(windows)]
    fn command_not_found() -> &'static str {
        "C:\\nonexistent\\hook.exe"
    }

    #[tokio::test]
    async fn test_executor_echo_hook() {
        let cwd = temp_test_dir("echo-hook");
        let payload = test_payload(&cwd);

        let outcome = execute_command_hook(&payload, success_json_command(), Some(5)).await;

        assert!(matches!(outcome.control, HookResultControl::Continue));
        assert!(outcome.result.additional_context.is_none());
        assert!(outcome.result.resume.is_none());
        assert!(outcome.result.updated_input.is_none());
    }

    #[tokio::test]
    async fn test_executor_plain_text() {
        let cwd = temp_test_dir("plain-text");
        let payload = test_payload(&cwd);

        let outcome = execute_command_hook(&payload, plain_text_command(), Some(5)).await;

        assert!(matches!(outcome.control, HookResultControl::Continue));
        assert_eq!(
            outcome.result.additional_context.as_deref(),
            Some("hello world")
        );
    }

    #[tokio::test]
    async fn test_executor_exit_2() {
        let cwd = temp_test_dir("exit-2");
        let payload = test_payload(&cwd);

        let outcome = execute_command_hook(&payload, exit_2_command(), Some(5)).await;

        match outcome.control {
            HookResultControl::Block { reason } => assert_eq!(reason, "blocked"),
            HookResultControl::Continue => panic!("expected blocked hook outcome"),
        }
    }

    #[tokio::test]
    async fn test_executor_timeout() {
        let cwd = temp_test_dir("timeout");
        let payload = test_payload(&cwd);
        let start = tokio::time::Instant::now();

        let outcome = execute_command_hook(&payload, timeout_command(), Some(1)).await;

        assert!(matches!(outcome.control, HookResultControl::Continue));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn test_executor_command_not_found() {
        let cwd = temp_test_dir("not-found");
        let payload = test_payload(&cwd);

        let outcome = execute_command_hook(&payload, command_not_found(), Some(5)).await;

        assert!(matches!(outcome.control, HookResultControl::Continue));
    }
}
