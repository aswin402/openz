use std::path::PathBuf;
pub use zeroclaw_runtime::agent::tui_events::LspStatus;

#[derive(Debug, Clone)]
pub struct McpServerStatus {
    pub name: String,
    pub connected: bool,
}

#[derive(Debug, Clone)]
pub enum ActivityState {
    Idle,
    Thinking,
    CallingModel,
    RunningTool(String),
    RunningMcp(String),
    IndexingProject,
    WritingFiles,
    WaitingForResponse,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct RuntimeStatus {
    pub active_model: Option<String>,
    pub active_provider: Option<String>,
    pub is_thinking: bool,
    pub current_activity: ActivityState,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub estimated_cost_usd: Option<f64>,
    pub mcp_servers: Vec<McpServerStatus>,
    pub lsp_status: LspStatus,
    pub git_branch: Option<String>,
    pub git_dirty: bool,
    pub modified_files: Vec<PathBuf>,
    pub available_skills_count: usize,
    pub last_error: Option<String>,
}

impl Default for RuntimeStatus {
    fn default() -> Self {
        Self {
            active_model: None,
            active_provider: None,
            is_thinking: false,
            current_activity: ActivityState::Idle,
            prompt_tokens: 0,
            completion_tokens: 0,
            estimated_cost_usd: Some(0.0),
            mcp_servers: Vec::new(),
            lsp_status: LspStatus::Inactive,
            git_branch: None,
            git_dirty: false,
            modified_files: Vec::new(),
            available_skills_count: 0,
            last_error: None,
        }
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{truncated}…")
    } else {
        s.to_string()
    }
}

pub fn format_tokens(n: Option<u64>) -> String {
    match n {
        Some(val) => {
            if val >= 1000 {
                format!("{:.1}k tok", val as f64 / 1000.0)
            } else {
                format!("{val} tok")
            }
        }
        None => "tokens unknown".to_string(),
    }
}

pub fn format_cost(c: Option<f64>) -> String {
    match c {
        Some(val) => format!("${:.2}", val.max(0.0)),
        None => "cost unknown".to_string(),
    }
}

pub fn format_git(branch: Option<&str>, dirty: bool) -> String {
    match branch {
        Some(b) => {
            if dirty {
                format!("{b}*")
            } else {
                b.to_string()
            }
        }
        None => "no git".to_string(),
    }
}
