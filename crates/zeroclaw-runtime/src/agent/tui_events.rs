use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

pub static TUI_SUSPENDED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LspStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum RuntimeEvent {
    ModelSelected {
        provider: String,
        model: String,
    },
    ThinkingStarted,
    ThinkingFinished {
        prompt_tokens: Option<u64>,
        completion_tokens: Option<u64>,
        cost_usd: Option<f64>,
    },
    ToolStarted(String),
    ToolFinished {
        name: String,
        success: bool,
    },
    McpConnected(String),
    McpDisconnected(String),
    LspChanged(LspStatus),
    FileModified(PathBuf),
    GitChanged {
        branch: Option<String>,
        dirty: bool,
    },
    Error(String),
    Suspended,
    Resumed,
}
