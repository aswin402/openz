use async_trait::async_trait;
use git2::{Repository, StatusOptions};
use serde_json::json;
use std::fmt::Write;
use std::sync::Arc;
use zeroclaw_api::tool::{Tool, ToolResult};
use zeroclaw_config::policy::SecurityPolicy;

/// Tool for natively querying Git repository specifications.
pub struct GitNativeTool {
    security: Arc<SecurityPolicy>,
}

impl GitNativeTool {
    pub fn new(security: Arc<SecurityPolicy>) -> Self {
        Self { security }
    }
}

#[async_trait]
impl Tool for GitNativeTool {
    fn name(&self) -> &str {
        "git_native"
    }

    fn description(&self) -> &str {
        "Natively inspect repository status, current active branch, and commit log details."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["status", "branch", "log"],
                    "description": "Git repository details to query"
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

        let workspace_dir = self.security.workspace_dir.clone();
        let res = tokio::task::spawn_blocking(move || -> anyhow::Result<String> {
            let repo = Repository::open(&workspace_dir)
                .map_err(|e| anyhow::Error::msg(format!("Failed to open repository: {e}")))?;

            match action.as_str() {
                "branch" => {
                    let head = repo
                        .head()
                        .map_err(|e| anyhow::Error::msg(format!("Failed to get HEAD ref: {e}")))?;
                    let name = head.shorthand().unwrap_or("unknown");
                    Ok(format!("Current branch: {name}"))
                }
                "status" => {
                    let mut status_opts = StatusOptions::new();
                    status_opts.include_untracked(true);
                    let statuses = repo
                        .statuses(Some(&mut status_opts))
                        .map_err(|e| anyhow::Error::msg(format!("Failed to query status: {e}")))?;

                    if statuses.is_empty() {
                        return Ok("Repository is clean (no changes).".into());
                    }

                    let mut out = String::new();
                    let _ = writeln!(out, "File Statuses:");
                    for entry in statuses.iter() {
                        let path = entry.path().unwrap_or("unknown");
                        let status = entry.status();
                        let _ = writeln!(out, "- {path}: {status:?}");
                    }
                    Ok(out)
                }
                "log" => {
                    let mut revwalk = repo
                        .revwalk()
                        .map_err(|e| anyhow::Error::msg(format!("Failed to walk commits: {e}")))?;
                    revwalk.push_head().map_err(|e| {
                        anyhow::Error::msg(format!("Failed to push HEAD to commit walker: {e}"))
                    })?;

                    let mut out = String::new();
                    let _ = writeln!(out, "Commit History (last 10):");
                    for oid_res in revwalk.take(10) {
                        let oid = oid_res?;
                        let commit = repo.find_commit(oid)?;
                        let msg = commit.message().unwrap_or("").trim();
                        let author = commit.author();
                        let author_name = author.name().unwrap_or("unknown");
                        let _ = writeln!(
                            out,
                            "- [{}] {} (by {})",
                            &oid.to_string()[..8],
                            msg,
                            author_name
                        );
                    }
                    Ok(out)
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
            workspace_dir: std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir()),
            ..SecurityPolicy::default()
        })
    }

    #[test]
    fn git_native_tool_metadata() {
        let tool = GitNativeTool::new(test_security());
        assert_eq!(tool.name(), "git_native");
    }
}
