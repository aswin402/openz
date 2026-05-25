use async_trait::async_trait;
use notify::{RecursiveMode, Watcher};
use serde_json::json;
use std::fmt::Write;
use std::sync::Arc;
use std::time::Duration;
use zeroclaw_api::tool::{Tool, ToolResult};
use zeroclaw_config::policy::SecurityPolicy;

/// Tool for actively monitoring file changes in the workspace.
pub struct FsEventMonitorTool {
    security: Arc<SecurityPolicy>,
}

impl FsEventMonitorTool {
    pub fn new(security: Arc<SecurityPolicy>) -> Self {
        Self { security }
    }
}

#[async_trait]
impl Tool for FsEventMonitorTool {
    fn name(&self) -> &str {
        "fs_event_monitor"
    }

    fn description(&self) -> &str {
        "Actively capture and list filesystem events in the workspace for a short monitoring duration."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "timeout_secs": {
                    "type": "integer",
                    "description": "Duration in seconds to watch the workspace (default: 5, max: 10)"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "True to watch subdirectories recursively (default: true)"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let workspace_dir = self.security.workspace_dir.clone();
        let timeout_val = args
            .get("timeout_secs")
            .and_then(|t| t.as_u64())
            .unwrap_or(5)
            .min(10);
        let recursive = args
            .get("recursive")
            .and_then(|r| r.as_bool())
            .unwrap_or(true);

        let mode = if recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(move |res| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to initialize watcher: {e}")),
                });
            }
        };

        if let Err(e) = watcher.watch(&workspace_dir, mode) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to register directory watch: {e}")),
            });
        }

        let start = std::time::Instant::now();
        let mut events = Vec::new();

        while start.elapsed() < Duration::from_secs(timeout_val) {
            if let Ok(event) = rx.recv_timeout(Duration::from_millis(100)) {
                events.push(event);
                if events.len() >= 30 {
                    break;
                }
            }
        }

        let mut out = String::new();
        if events.is_empty() {
            let _ = write!(
                out,
                "No filesystem events detected in the workspace within {timeout_val} seconds."
            );
        } else {
            let _ = writeln!(
                out,
                "Detected {} filesystem events in the workspace:",
                events.len()
            );
            for (i, e) in events.iter().enumerate() {
                let paths_str: Vec<_> = e
                    .paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                let _ = writeln!(out, "- Event #{}: {:?} -> {:?}", i + 1, e.kind, paths_str);
            }
        }

        Ok(ToolResult {
            success: true,
            output: out,
            error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeroclaw_config::autonomy::AutonomyLevel;

    fn test_security() -> Arc<SecurityPolicy> {
        Arc::new(SecurityPolicy {
            autonomy: AutonomyLevel::Full,
            workspace_dir: std::env::temp_dir(),
            ..SecurityPolicy::default()
        })
    }

    #[test]
    fn fs_event_monitor_tool_metadata() {
        let tool = FsEventMonitorTool::new(test_security());
        assert_eq!(tool.name(), "fs_event_monitor");
    }
}
