use std::io::Write;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio::time::Duration;
use zeroclaw_config::schema::Config;
use zeroclaw_runtime::agent::loop_::AgentRunOverrides;
use zeroclaw_runtime::agent::tui_events::{RuntimeEvent, LspStatus};

// Simple activity state representation
#[derive(Debug, Clone, PartialEq, Eq)]
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

struct CliStatusState {
    active_model: Option<String>,
    active_provider: Option<String>,
    is_thinking: bool,
    current_activity: ActivityState,
    prompt_tokens: u64,
    completion_tokens: u64,
    estimated_cost_usd: Option<f64>,
    mcp_servers: Vec<String>,
    lsp_status: LspStatus,
    git_branch: Option<String>,
    git_dirty: bool,
    is_suspended: bool,
}

pub struct TuiApp {
    config: Config,
    agent_alias: String,
    session_state_file: Option<PathBuf>,
    system_prompt: String,
    final_temperature: Option<f64>,
    tui_sender: mpsc::Sender<RuntimeEvent>,
    tui_receiver: mpsc::Receiver<RuntimeEvent>,
}

impl TuiApp {
    pub fn new(
        config: Config,
        agent_alias: String,
        session_state_file: Option<PathBuf>,
        system_prompt: String,
        final_temperature: Option<f64>,
    ) -> anyhow::Result<Self> {
        let (tx, rx) = mpsc::channel(100);
        Ok(Self {
            config,
            agent_alias,
            session_state_file,
            system_prompt,
            final_temperature,
            tui_sender: tx,
            tui_receiver: rx,
        })
    }

    pub async fn run_loop(self) -> anyhow::Result<()> {
        let (tx_quit, mut rx_quit) = mpsc::channel::<()>(1);
        let mut tui_receiver = self.tui_receiver;

        // Initialize MCP servers list from config
        let mcp_servers: Vec<String> = self.config.mcp.servers.iter().map(|s| s.name.clone()).collect();

        // Query initial git branch and dirty flag
        let (git_branch, git_dirty) = Self::query_git_info();

        // Get initial model/provider from config
        let mut active_provider = None;
        let mut active_model = None;
        if let Some((provider_type, _, model_cfg)) = self.config.resolved_model_provider_for_agent(&self.agent_alias) {
            active_provider = Some(provider_type.to_string());
            active_model = model_cfg.model.clone();
        }

        // Spawn background status line drawer/updater
        tokio::spawn(async move {
            let mut state = CliStatusState {
                active_model,
                active_provider,
                is_thinking: false,
                current_activity: ActivityState::Idle,
                prompt_tokens: 0,
                completion_tokens: 0,
                estimated_cost_usd: Some(0.0),
                mcp_servers,
                lsp_status: LspStatus::Inactive,
                git_branch,
                git_dirty,
                is_suspended: false,
            };

            let (_, initial_h) = crossterm::terminal::size().unwrap_or((80, 24));
            let mut last_height = initial_h;
            let spinner_frames = ["◐", "◓", "◑", "◒"];
            let mut spinner_idx = 0;

            let mut interval = tokio::time::interval(Duration::from_millis(150));

            loop {
                tokio::select! {
                    _ = rx_quit.recv() => {
                        break;
                    }
                    _ = interval.tick() => {
                        if state.is_thinking {
                            spinner_idx = (spinner_idx + 1) % spinner_frames.len();
                            Self::draw_status_line(&state, spinner_frames[spinner_idx], &mut last_height);
                        }
                    }
                    Some(event) = tui_receiver.recv() => {
                        match event {
                            RuntimeEvent::ModelSelected { provider, model } => {
                                state.active_provider = Some(provider);
                                state.active_model = Some(model);
                            }
                            RuntimeEvent::ThinkingStarted => {
                                state.is_thinking = true;
                                state.current_activity = ActivityState::Thinking;
                            }
                            RuntimeEvent::ThinkingFinished { prompt_tokens, completion_tokens, cost_usd } => {
                                state.is_thinking = false;
                                state.current_activity = ActivityState::Idle;
                                if let Some(pt) = prompt_tokens {
                                    state.prompt_tokens += pt;
                                }
                                if let Some(ct) = completion_tokens {
                                    state.completion_tokens += ct;
                                }
                                if let Some(c) = cost_usd {
                                    state.estimated_cost_usd = Some(
                                        state.estimated_cost_usd.unwrap_or(0.0) + c
                                    );
                                }
                            }
                            RuntimeEvent::ToolStarted(name) => {
                                state.current_activity = ActivityState::RunningTool(name);
                            }
                            RuntimeEvent::ToolFinished { name: _, success: _ } => {
                                state.current_activity = ActivityState::Thinking;
                            }
                            RuntimeEvent::McpConnected(name) => {
                                if !state.mcp_servers.contains(&name) {
                                    state.mcp_servers.push(name);
                                }
                            }
                            RuntimeEvent::McpDisconnected(name) => {
                                state.mcp_servers.retain(|x| x != &name);
                            }
                            RuntimeEvent::LspChanged(lsp) => {
                                state.lsp_status = lsp;
                            }
                            RuntimeEvent::GitChanged { branch, dirty } => {
                                state.git_branch = branch;
                                state.git_dirty = dirty;
                            }
                            RuntimeEvent::Error(err) => {
                                state.is_thinking = false;
                                state.current_activity = ActivityState::Error(err);
                            }
                            RuntimeEvent::Suspended => {
                                state.is_suspended = true;
                            }
                            RuntimeEvent::Resumed => {
                                state.is_suspended = false;
                            }
                            _ => {}
                        }
                        Self::draw_status_line(&state, spinner_frames[spinner_idx], &mut last_height);
                    }
                }
            }

            // Cleanup status line at end
            if last_height > 0 {
                print!("\x1B[r\x1B[{};1H\x1B[K", last_height.saturating_sub(2));
                let _ = std::io::stdout().flush();
            }
        });

        // Set up the terminal scrolling region (1 to H-3) while saving and restoring cursor position
        let (_, initial_h) = crossterm::terminal::size().unwrap_or((80, 24));
        print!("\x1B[s\x1B[1;{}r\x1B[u", initial_h.saturating_sub(3));
        let _ = std::io::stdout().flush();

        // Run the interactive agent loop synchronously
        let tui_sender = self.tui_sender.clone();
        let mut overrides = AgentRunOverrides::default();
        overrides.tui_sender = Some(tui_sender);

        let res = Box::pin(zeroclaw_runtime::agent::run(
            self.config,
            &self.agent_alias,
            None, // message
            None, // provider override
            None, // model override
            self.final_temperature,
            Vec::new(),
            true, // interactive
            self.session_state_file,
            None, // allowed_tools
            overrides,
        ))
        .await;

        // Reset scroll region at exit
        let _ = tx_quit.send(()).await;
        print!("\x1B[r");
        let _ = std::io::stdout().flush();

        res.map(|_| ())
    }

    fn draw_status_line(state: &CliStatusState, spinner_frame: &str, last_height: &mut u16) {
        if state.is_suspended || zeroclaw_runtime::agent::tui_events::TUI_SUSPENDED.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
        if *last_height != h {
            print!("\x1B[s"); // Save cursor position
            if *last_height > 0 {
                // Clear old bottom row and reset scroll region
                print!("\x1B[r\x1B[{};1H\x1B[K", (*last_height).saturating_sub(2));
            }
            // Set scroll region to 1 .. h-3
            print!("\x1B[1;{}r", h.saturating_sub(3));
            print!("\x1B[u"); // Restore cursor position
            *last_height = h;
        }

        let mut spans = Vec::new();

        // 1. Spinner & Activity
        if state.is_thinking {
            match &state.current_activity {
                ActivityState::Thinking => {
                    spans.push(format!("\x1B[36m{} Thinking...\x1B[0m", spinner_frame));
                }
                ActivityState::CallingModel => {
                    spans.push(format!("\x1B[36m{} Calling model...\x1B[0m", spinner_frame));
                }
                ActivityState::RunningTool(name) => {
                    spans.push(format!("\x1B[33m{} tool: {}\x1B[0m", spinner_frame, truncate(name, 22)));
                }
                ActivityState::RunningMcp(name) => {
                    spans.push(format!("\x1B[33m{} tool: {}\x1B[0m", spinner_frame, truncate(name, 22)));
                }
                _ => {
                    spans.push(format!("\x1B[36m{} Thinking...\x1B[0m", spinner_frame));
                }
            }
        } else {
            match &state.current_activity {
                ActivityState::Idle => {
                    spans.push("\x1B[32m✓ Ready\x1B[0m".to_string());
                }
                ActivityState::Error(err) => {
                    spans.push(format!("\x1B[31m✗ Error: {}\x1B[0m", truncate(err, 30)));
                }
                _ => {
                    spans.push("\x1B[32m✓ Ready\x1B[0m".to_string());
                }
            }
        }

        // Divider
        spans.push(" · ".to_string());

        // 2. Model
        let model = state.active_model.as_deref().unwrap_or("unknown");
        spans.push(format!("\x1B[1;35m{}\x1B[0m", model));

        // Divider
        spans.push(" · ".to_string());

        // 3. Tokens
        let total_tokens = state.prompt_tokens + state.completion_tokens;
        spans.push(format_tokens(Some(total_tokens)));

        // Divider
        spans.push(" · ".to_string());

        // 4. Cost
        let cost_str = format_cost(state.estimated_cost_usd);
        spans.push(format!("\x1B[32m{}\x1B[0m", cost_str));

        // Divider
        spans.push(" · ".to_string());

        // 5. MCP count
        let mcp_count = state.mcp_servers.len();
        spans.push(format!("MCP {}", mcp_count));

        // Divider
        spans.push(" · ".to_string());

        // 6. Git info
        let git_str = format_git(state.git_branch.as_deref(), state.git_dirty);
        spans.push(format!("\x1B[34m{}\x1B[0m", git_str));

        let status_line = spans.join("");

        // Draw at row h-2 (just above prompt row h-1)
        print!("\x1B[s\x1B[{};1H\x1B[K{}\x1B[u", h.saturating_sub(2), status_line);
        let _ = std::io::stdout().flush();
    }

    fn query_git_info() -> (Option<String>, bool) {
        let branch = std::process::Command::new("git")
            .args(&["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        let dirty = std::process::Command::new("git")
            .args(&["status", "--porcelain"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
            .unwrap_or(false);

        (branch, dirty)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{truncated}…")
    } else {
        s.to_string()
    }
}

fn format_tokens(n: Option<u64>) -> String {
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

fn format_cost(c: Option<f64>) -> String {
    match c {
        Some(val) => format!("${:.2}", val.max(0.0)),
        None => "cost unknown".to_string(),
    }
}

fn format_git(branch: Option<&str>, dirty: bool) -> String {
    match branch {
        Some(b) => {
            if dirty {
                format!("git {b}*")
            } else {
                format!("git {b}")
            }
        }
        None => "no git".to_string(),
    }
}
