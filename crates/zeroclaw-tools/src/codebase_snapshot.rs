use async_trait::async_trait;
use serde_json::json;
use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;
use zeroclaw_api::tool::{Tool, ToolResult};
use zeroclaw_config::policy::SecurityPolicy;

/// A codebase snapshot/packer tool that aggregates repository source files into a single context.
pub struct CodebaseSnapshotTool {
    security: Arc<SecurityPolicy>,
}

impl CodebaseSnapshotTool {
    pub fn new(security: Arc<SecurityPolicy>) -> Self {
        Self { security }
    }
}

// Check if the file is binary by looking for null bytes in the first 1024 bytes.
fn is_binary_file(path: &Path) -> bool {
    use std::io::Read;
    if let Ok(mut file) = std::fs::File::open(path) {
        let mut buffer = [0; 1024];
        if let Ok(bytes_read) = file.read(&mut buffer) {
            return bytes_read > 0 && buffer[..bytes_read].contains(&0);
        }
    }
    false
}

#[async_trait]
impl Tool for CodebaseSnapshotTool {
    fn name(&self) -> &str {
        "codebase_snapshot"
    }

    fn description(&self) -> &str {
        "Recursively scan the codebase directory and combine all non-binary, within-size-limit source files into a single formatted markdown snapshot."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "target_dir": {
                    "type": "string",
                    "description": "Directory to scan relative to workspace root (default: workspace root)"
                },
                "max_file_size_kb": {
                    "type": "integer",
                    "description": "Maximum size of a single file in KB to include (default: 50, max: 200)"
                },
                "exclude_patterns": {
                    "type": "array",
                    "items": {
                        "type": "string"
                    },
                    "description": "List of simple string/substring patterns to exclude from path"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let target_dir_param = args.get("target_dir").and_then(|v| v.as_str());
        let max_size_kb = args
            .get("max_file_size_kb")
            .and_then(|v| v.as_u64())
            .unwrap_or(50)
            .min(200);
        let max_size_bytes = max_size_kb * 1024;

        let exclude_patterns: Vec<String> = args
            .get("exclude_patterns")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|val| val.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Resolve scan root path using resolve_tool_path to respect paths/workspace boundaries
        let scan_root = if let Some(dir) = target_dir_param {
            // Reject traversal
            if dir.contains("../") || dir.contains("..\\") || dir == ".." {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("Path traversal ('..') is not allowed in target_dir.".into()),
                });
            }
            self.security.resolve_tool_path(dir)
        } else {
            self.security.workspace_dir.clone()
        };

        let resolved_root = match std::fs::canonicalize(&scan_root) {
            Ok(path) => path,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Invalid target directory path: {e}")),
                });
            }
        };

        // Safety verification: path must be readable according to security policy
        if !self.security.is_resolved_path_readable(&resolved_root) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(
                    "Security Policy violation: Read access denied for the requested target_dir."
                        .into(),
                ),
            });
        }

        let mut out = String::new();
        let mut file_count = 0;
        let mut total_bytes = 0;
        let max_total_output_bytes = 2_000_000; // Limit total packed payload to 2MB to prevent large memory load
        let mut truncated = false;

        // Custom default directory exclusions
        let default_excludes = [
            ".git",
            "node_modules",
            "target",
            "dist",
            "build",
            ".claude",
            ".gemini",
            ".planning",
            ".cargo",
        ];

        let walker = WalkDir::new(&resolved_root)
            .into_iter()
            .filter_entry(|entry| {
                let file_name = entry.file_name().to_string_lossy();
                // Skip default exclusions
                for excl in &default_excludes {
                    if file_name == *excl {
                        return false;
                    }
                }
                true
            });

        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue, // ignore walk errors
            };

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Path exclusion patterns
            let path_str = path.to_string_lossy();
            let mut matches_exclude = false;
            for pattern in &exclude_patterns {
                if path_str.contains(pattern) {
                    matches_exclude = true;
                    break;
                }
            }
            if matches_exclude {
                continue;
            }

            // Verify with security policy
            let resolved_file = match std::fs::canonicalize(path) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if !self.security.is_resolved_path_readable(&resolved_file) {
                continue;
            }

            // Check file size
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let size = metadata.len();
            if size > max_size_bytes {
                continue;
            }

            // Filter binary files
            if is_binary_file(path) {
                continue;
            }

            // Read file content
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Get path relative to the scan root (or workspace root if cleaner)
            let relative_path = path
                .strip_prefix(&resolved_root)
                .unwrap_or(path)
                .to_string_lossy();

            let file_block_header = format!("\n---\n### File: `{relative_path}`\n");
            // Estimate size
            if out.len() + file_block_header.len() + content.len() + 20 > max_total_output_bytes {
                truncated = true;
                break;
            }

            let _ = write!(out, "{}", file_block_header);

            // Add markdown codeblock wrapper
            // Choose language tag if possible (or default to empty)
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            let _ = writeln!(out, "```{}", extension);
            let _ = writeln!(out, "{}", content);
            let _ = writeln!(out, "```");

            file_count += 1;
            total_bytes += size;
        }

        let mut header = format!(
            "Codebase snapshot of `{}` ({} files included, total {} bytes of raw files)\n",
            resolved_root.to_string_lossy(),
            file_count,
            total_bytes
        );

        if truncated {
            let _ = writeln!(
                header,
                "> [!WARNING]\n> Output was truncated because it exceeded the size limit of {} characters.",
                max_total_output_bytes
            );
        }

        Ok(ToolResult {
            success: true,
            output: format!("{}{}", header, out),
            error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use zeroclaw_config::autonomy::AutonomyLevel;

    fn test_security(workspace: PathBuf) -> Arc<SecurityPolicy> {
        Arc::new(SecurityPolicy {
            autonomy: AutonomyLevel::Supervised,
            workspace_dir: workspace,
            ..SecurityPolicy::default()
        })
    }

    #[tokio::test]
    async fn codebase_snapshot_basic() {
        let dir = TempDir::new().unwrap();
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(dir.path().join("README.md"), "# OpenZ Project").unwrap();

        // Let's create an excluded file too
        let target_dir = dir.path().join("target");
        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("debug_binary"), vec![0u8; 100]).unwrap();

        let tool = CodebaseSnapshotTool::new(test_security(dir.path().to_path_buf()));
        let result = tool.execute(json!({})).await.unwrap();

        assert!(result.success);
        let out = result.output;
        assert!(out.contains("main.rs"));
        assert!(out.contains("README.md"));
        assert!(!out.contains("debug_binary"));
        assert!(out.contains("fn main() {}"));
        assert!(out.contains("# OpenZ Project"));
    }
}
