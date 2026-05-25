use async_trait::async_trait;
use serde_json::json;
use std::fmt::Write;
use std::sync::Arc;
use sysinfo::{Pid, System};
use zeroclaw_api::tool::{Tool, ToolResult};
use zeroclaw_config::policy::SecurityPolicy;

/// Tool for querying system specifications and process details.
pub struct SysInfoTool {
    security: Arc<SecurityPolicy>,
}

impl SysInfoTool {
    pub fn new(security: Arc<SecurityPolicy>) -> Self {
        Self { security }
    }
}

#[async_trait]
impl Tool for SysInfoTool {
    fn name(&self) -> &str {
        "sys_info"
    }

    fn description(&self) -> &str {
        "Retrieve system specs (OS, CPU usage, Memory stats) or list running processes and terminate them."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["system_stats", "process_list", "kill_process"],
                    "description": "The diagnostic action to perform"
                },
                "pid": {
                    "type": "integer",
                    "description": "Process ID to terminate (required for 'kill_process')"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let action = match args.get("action").and_then(|a| a.as_str()) {
            Some(a) => a.to_string(),
            None => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("Missing action parameter".into()),
                });
            }
        };

        let security = self.security.clone();
        let res = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
            match action.as_str() {
                "system_stats" => {
                    let mut sys = System::new_all();
                    sys.refresh_all();

                    let mut out = String::new();
                    let _ = writeln!(out, "OS Name: {:?}", System::name());
                    let _ = writeln!(out, "OS Version: {:?}", System::os_version());
                    let _ = writeln!(out, "Host Name: {:?}", System::host_name());
                    let _ = writeln!(out, "CPU Count: {}", sys.cpus().len());
                    let _ = writeln!(
                        out,
                        "Total Memory: {} MB",
                        sys.total_memory() / (1024 * 1024)
                    );
                    let _ = writeln!(out, "Used Memory: {} MB", sys.used_memory() / (1024 * 1024));
                    let _ = writeln!(out, "Total Swap: {} MB", sys.total_swap() / (1024 * 1024));
                    let _ = writeln!(out, "Used Swap: {} MB", sys.used_swap() / (1024 * 1024));
                    Ok(out)
                }
                "process_list" => {
                    let mut sys = System::new_all();
                    sys.refresh_all();

                    let mut out = String::new();
                    let _ = writeln!(
                        out,
                        "{:<8} {:<10} {:<10} {:<30}",
                        "PID", "CPU %", "MEM (MB)", "COMMAND"
                    );
                    let _ = writeln!(
                        out,
                        "------------------------------------------------------------"
                    );

                    let mut processes: Vec<_> = sys.processes().values().collect();
                    processes.sort_by(|a, b| {
                        b.cpu_usage()
                            .partial_cmp(&a.cpu_usage())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    for p in processes.iter().take(25) {
                        let cmd_str = p
                            .cmd()
                            .iter()
                            .map(|s| s.to_string_lossy())
                            .collect::<Vec<_>>()
                            .join(" ");
                        let cmd_truncated = if cmd_str.len() > 30 {
                            format!("{}...", &cmd_str[..27])
                        } else {
                            cmd_str
                        };
                        let _ = writeln!(
                            out,
                            "{:<8} {:<10.1} {:<10} {:<30}",
                            p.pid().as_u32(),
                            p.cpu_usage(),
                            p.memory() / (1024 * 1024),
                            cmd_truncated
                        );
                    }
                    Ok(out)
                }
                "kill_process" => {
                    if !security.can_act() {
                        return Err(anyhow::Error::msg("Action blocked: autonomy is read-only"));
                    }
                    let pid_val = args
                        .get("pid")
                        .and_then(|p| p.as_i64())
                        .ok_or_else(|| anyhow::Error::msg("Missing pid parameter"))?;
                    let mut sys = System::new();
                    let pid = Pid::from(pid_val as usize);
                    sys.refresh_processes_specifics(
                        sysinfo::ProcessesToUpdate::Some(&[pid]),
                        true,
                        sysinfo::ProcessRefreshKind::everything(),
                    );

                    if let Some(process) = sys.process(pid) {
                        process.kill();
                        Ok(format!(
                            "Successfully sent kill signal to process {} ({})",
                            pid_val,
                            process.name().to_string_lossy()
                        ))
                    } else {
                        Err(anyhow::Error::msg(format!(
                            "Process with PID {} not found",
                            pid_val
                        )))
                    }
                }
                _ => Err(anyhow::Error::msg(format!("Unsupported action: {action}"))),
            }
        })
        .await
        .map_err(|e| anyhow::Error::msg(format!("Execution thread panicked: {e}")))?;

        match res {
            Ok(output) => Ok(ToolResult {
                success: true,
                output,
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(e.to_string()),
            }),
        }
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
    fn sys_info_tool_metadata() {
        let tool = SysInfoTool::new(test_security());
        assert_eq!(tool.name(), "sys_info");
        assert!(tool.description().contains("system specs"));
    }
}
