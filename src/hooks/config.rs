use serde::{Deserialize, Serialize};

/// Default timeout for hook execution in seconds
fn default_timeout() -> Option<u64> {
    Some(30)
}

/// Configuration for a single hook entry
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HookConfig {
    /// Hook event name (e.g., "PreToolUse", "Stop")
    pub event: String,

    /// Optional regex pattern to match against tool_name (for tool-bearing events)
    #[serde(default)]
    pub matcher: Option<String>,

    /// Shell command to execute
    pub command: String,

    /// Timeout in seconds (default 30)
    #[serde(default = "default_timeout")]
    pub timeout: Option<u64>,

    /// Optional status message to display
    #[serde(default)]
    pub status_message: Option<String>,

    /// Protocol version (default "claude-code")
    #[serde(default)]
    pub protocol: Option<String>,
}

impl HookConfig {
    /// Check if the protocol is valid
    /// Returns true if protocol is None (defaults to "claude-code") or explicitly "claude-code"
    /// Returns false for unknown protocols
    pub fn is_valid_protocol(&self) -> bool {
        match &self.protocol {
            None => true, // Defaults to "claude-code"
            Some(p) => p == "claude-code",
        }
    }
}

/// Configuration for all hooks (global or per-agent)
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct HooksConfig {
    /// Maximum number of resume iterations
    #[serde(default)]
    pub max_resume: Option<u32>,

    /// List of hook entries
    #[serde(default)]
    pub entries: Vec<HookConfig>,
}

impl HooksConfig {
    /// Merge global and agent hooks
    ///
    /// Rules:
    /// - Agent entries extend global entries (both lists are combined)
    /// - If agent and global have entries with the same `event` AND same `matcher`, agent takes priority (replaces)
    /// - `max_resume`: agent value overrides global if agent value is Some
    pub fn merge(global: &HooksConfig, agent: &HooksConfig) -> HooksConfig {
        // Start with global entries
        let mut merged_entries = global.entries.clone();

        // Process agent entries
        for agent_entry in &agent.entries {
            // Check if there's a matching entry in global (same event and matcher)
            if let Some(pos) = merged_entries
                .iter()
                .position(|e| e.event == agent_entry.event && e.matcher == agent_entry.matcher)
            {
                // Replace the global entry with the agent entry
                merged_entries[pos] = agent_entry.clone();
            } else {
                // No conflict, add the agent entry
                merged_entries.push(agent_entry.clone());
            }
        }

        // Determine max_resume: agent overrides global if Some
        let max_resume = agent.max_resume.or(global.max_resume);

        HooksConfig {
            max_resume,
            entries: merged_entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hooks_config_parse() {
        let yaml = r#"
max_resume: 3
entries:
  - event: Stop
    command: "/path/to/hook.sh"
    timeout: 10
    protocol: claude-code
"#;

        let config: HooksConfig = serde_yaml::from_str(yaml).expect("parse hooks config");

        assert_eq!(config.max_resume, Some(3));
        assert_eq!(config.entries.len(), 1);

        let entry = &config.entries[0];
        assert_eq!(entry.event, "Stop");
        assert_eq!(entry.command, "/path/to/hook.sh");
        assert_eq!(entry.timeout, Some(10));
        assert_eq!(entry.protocol, Some("claude-code".to_string()));
        assert!(entry.matcher.is_none());
        assert!(entry.status_message.is_none());
    }

    #[test]
    fn test_hooks_config_merge() {
        let global = HooksConfig {
            max_resume: Some(5),
            entries: vec![
                HookConfig {
                    event: "Stop".to_string(),
                    matcher: None,
                    command: "global-stop.sh".to_string(),
                    timeout: Some(30),
                    status_message: None,
                    protocol: None,
                },
                HookConfig {
                    event: "SessionStart".to_string(),
                    matcher: None,
                    command: "global-start.sh".to_string(),
                    timeout: Some(30),
                    status_message: None,
                    protocol: None,
                },
            ],
        };

        let agent = HooksConfig {
            max_resume: Some(3),
            entries: vec![HookConfig {
                event: "PreToolUse".to_string(),
                matcher: Some("shell".to_string()),
                command: "agent-tool.sh".to_string(),
                timeout: Some(15),
                status_message: None,
                protocol: None,
            }],
        };

        let merged = HooksConfig::merge(&global, &agent);

        // Agent max_resume should win
        assert_eq!(merged.max_resume, Some(3));

        // Should have 3 entries: 2 from global + 1 from agent
        assert_eq!(merged.entries.len(), 3);

        // Check that all events are present
        let events: Vec<&str> = merged.entries.iter().map(|e| e.event.as_str()).collect();
        assert!(events.contains(&"Stop"));
        assert!(events.contains(&"SessionStart"));
        assert!(events.contains(&"PreToolUse"));
    }

    #[test]
    fn test_hooks_config_merge_conflict() {
        let global = HooksConfig {
            max_resume: Some(5),
            entries: vec![HookConfig {
                event: "PreToolUse".to_string(),
                matcher: Some("shell".to_string()),
                command: "global-shell.sh".to_string(),
                timeout: Some(30),
                status_message: None,
                protocol: None,
            }],
        };

        let agent = HooksConfig {
            max_resume: None,
            entries: vec![HookConfig {
                event: "PreToolUse".to_string(),
                matcher: Some("shell".to_string()),
                command: "agent-shell.sh".to_string(),
                timeout: Some(10),
                status_message: Some("Agent override".to_string()),
                protocol: Some("claude-code".to_string()),
            }],
        };

        let merged = HooksConfig::merge(&global, &agent);

        // Global max_resume should be used (agent is None)
        assert_eq!(merged.max_resume, Some(5));

        // Should have only 1 entry (agent replaced global)
        assert_eq!(merged.entries.len(), 1);

        let entry = &merged.entries[0];
        assert_eq!(entry.command, "agent-shell.sh");
        assert_eq!(entry.timeout, Some(10));
        assert_eq!(entry.status_message, Some("Agent override".to_string()));
    }

    #[test]
    fn test_hooks_config_default() {
        let config = HooksConfig::default();

        assert!(config.max_resume.is_none());
        assert!(config.entries.is_empty());
    }

    #[test]
    fn test_valid_protocol() {
        // None should be valid (defaults to "claude-code")
        let hook1 = HookConfig {
            event: "Stop".to_string(),
            matcher: None,
            command: "test.sh".to_string(),
            timeout: Some(30),
            status_message: None,
            protocol: None,
        };
        assert!(hook1.is_valid_protocol());

        // "claude-code" should be valid
        let hook2 = HookConfig {
            event: "Stop".to_string(),
            matcher: None,
            command: "test.sh".to_string(),
            timeout: Some(30),
            status_message: None,
            protocol: Some("claude-code".to_string()),
        };
        assert!(hook2.is_valid_protocol());

        // Unknown protocol should be invalid
        let hook3 = HookConfig {
            event: "Stop".to_string(),
            matcher: None,
            command: "test.sh".to_string(),
            timeout: Some(30),
            status_message: None,
            protocol: Some("future-v2".to_string()),
        };
        assert!(!hook3.is_valid_protocol());
    }
}
