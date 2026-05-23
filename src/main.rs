#![recursion_limit = "256"]
#![warn(clippy::all, clippy::pedantic)]

pub mod tui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dialoguer::Select;
use zeroclaw_config::schema::Config;

#[derive(Parser, Debug)]
#[command(
    name = "openz",
    author = "theonlyhennygod",
    version = env!("CARGO_PKG_VERSION"),
    about = "openz - The minimal, self-improving, cutting-edge AI Agent CLI & TUI.",
    disable_help_flag = true,
    disable_version_flag = true
)]
struct Cli {
    #[arg(short, long)]
    help: bool,

    #[arg(short = 'V', long)]
    version: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show version, logo, and description
    Version,
    /// Run the interactive configuration wizard
    Configure,
    /// View the runtime event logs
    Logs,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install default crypto provider for Rustls TLS.
    if let Err(_) = rustls::crypto::ring::default_provider().install_default() {
        // Ignore if already installed
    }

    let args = std::env::args().collect::<Vec<String>>();
    
    // Check if --help or -h was passed anywhere
    let has_help = args.iter().any(|arg| arg == "--help" || arg == "-h");
    let has_version = args.iter().any(|arg| arg == "--version" || arg == "-V" || arg == "version");

    if has_help {
        print_openz_help();
        return Ok(());
    }

    if has_version {
        print_openz_version();
        return Ok(());
    }

    let cli = Cli::parse();

    if let Some(cmd) = cli.command {
        match cmd {
            Commands::Version => {
                print_openz_version();
            }
            Commands::Configure => {
                let mut config = Config::load_or_init().await?;
                run_configure_wizard(&mut config).await?;
            }
            Commands::Logs => {
                let config = Config::load_or_init().await?;
                print_logs(&config)?;
            }
        }
        return Ok(());
    }

    // Default: run interactive agent loop
    let mut config = Config::load_or_init().await?;

    // Wire CLI channel for interactive mode
    zeroclaw_runtime::agent::loop_::register_cli_channel_fn(Box::new(|| {
        Box::new(zeroclaw_channels::cli::CliChannel::new("cli"))
    }));

    // Find configured agent alias or run configure
    let agent_alias = if config.agents.contains_key("assistant") {
        "assistant".to_string()
    } else if let Some(first_alias) = config.agents.keys().next() {
        first_alias.clone()
    } else {
        println!("No agent configured yet. Let's run configuration first!");
        run_configure_wizard(&mut config).await?;
        "assistant".to_string()
    };

    let final_temperature: Option<f64> = config
        .model_provider_for_agent(&agent_alias)
        .and_then(|e| e.temperature);

    // Session selection/resume wizard
    let sessions = list_sessions();
    let mut session_file = None;

    if !sessions.is_empty() {
        let mut items = vec!["Start a new session".to_string()];
        for (i, s) in sessions.iter().enumerate() {
            let local_time: chrono::DateTime<chrono::Local> = s.modified.into();
            let time_str = local_time.format("%Y-%m-%d %H:%M:%S");
            let label = if i == 0 {
                format!("Resume last session ({time_str}): \"{}\"", s.preview)
            } else {
                format!("Session from {time_str}: \"{}\"", s.preview)
            };
            items.push(label);
        }

        let selection = Select::new()
            .with_prompt("Select chat session")
            .items(&items)
            .default(0)
            .interact()?;

        if selection > 0 {
            session_file = Some(sessions[selection - 1].path.clone());
        }
    }

    let session_state_file = if let Some(path) = session_file {
        Some(path)
    } else {
        let dir = get_sessions_dir();
        let _ = std::fs::create_dir_all(&dir);
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        Some(dir.join(format!("session_{timestamp}.json")))
    };

    // Run the agent loop
    use std::io::IsTerminal;
    if std::io::stdout().is_terminal() {
        let system_prompt = "You are a helpful AI assistant.".to_string();
        let app = crate::tui::app::TuiApp::new(
            config,
            agent_alias,
            session_state_file,
            system_prompt,
            final_temperature,
        )?;
        app.run_loop().await?;
        Ok(())
    } else {
        Box::pin(zeroclaw_runtime::agent::run(
            config,
            &agent_alias,
            None, // message
            None, // provider override
            None, // model override
            final_temperature,
            Vec::new(),
            true, // interactive
            session_state_file,
            None, // allowed_tools
            zeroclaw_runtime::agent::loop_::AgentRunOverrides::default(),
        ))
        .await
        .map(|_| ())
    }
}

fn print_openz_help() {
    println!("{}", console::style("openz (ZeroClaw fork) - The minimal, self-improving, cutting-edge AI Agent CLI & TUI.").cyan().bold());
    println!();
    println!("{}", console::style("Usage:").yellow().bold());
    println!("  openz                   Run the agent in TUI/CLI interactive mode");
    println!("  openz --help, -h        Show this help message");
    println!("  openz version           Show logo, version, and description");
    println!("  openz configure         Run the minimal configuration wizard");
    println!("  openz logs              View full runtime logs");
    println!();
}

fn print_openz_version() {
    println!("{}", console::style("  ___  ____  _____ _   _ _____").cyan().bold());
    println!("{}", console::style(" / _ \\|  _ \\| ____| \\ | |__  /").cyan().bold());
    println!("{}", console::style("| | | | |_) |  _| |  \\| | / / ").cyan().bold());
    println!("{}", console::style("| |_| |  __/| |___| |\\  |/ /_ ").cyan().bold());
    println!("{}", console::style(" \\___/|_|   |_____|_| \\_/____|").cyan().bold());
    println!();
    println!("  {} v{}", console::style("openz").bold(), env!("CARGO_PKG_VERSION"));
    println!("  {}", console::style("openz (ZeroClaw fork) - The minimal, self-improving, cutting-edge AI Agent CLI & TUI.").dim());
    println!();
}

async fn run_configure_wizard(config: &mut Config) -> Result<()> {
    println!("{}", console::style("=== openz Configuration Setup ===").cyan().bold());
    println!("This will set up your model provider and default agent.");
    println!();

    let providers = vec![
        "anthropic",
        "openai",
        "gemini",
        "groq",
        "deepseek",
        "ollama",
        "openrouter",
        "lmstudio",
        "Other"
    ];

    let selection = Select::new()
        .with_prompt("Select AI Model Provider")
        .items(&providers)
        .default(0)
        .interact()?;

    let picked = if selection == providers.len() - 1 {
        let custom: String = dialoguer::Input::new()
            .with_prompt("Enter Custom Provider Name")
            .interact_text()?;
        custom.trim().to_lowercase()
    } else {
        providers[selection].to_string()
    };

    let needs_key = !matches!(picked.as_str(), "ollama" | "lmstudio");

    let api_key = if needs_key {
        let key: String = dialoguer::Password::new()
            .with_prompt("Enter API Key")
            .interact()?;
        key.trim().to_string()
    } else {
        String::new()
    };

    let default_model = match picked.as_str() {
        "anthropic" => "claude-3-5-sonnet-20241022",
        "openai" => "gpt-4o",
        "gemini" => "gemini-1.5-pro",
        "groq" => "llama3-70b-8192",
        "deepseek" => "deepseek-chat",
        "ollama" => "llama3",
        "lmstudio" => "model-id",
        _ => "model-id"
    };

    let model: String = dialoguer::Input::new()
        .with_prompt("Enter Model ID")
        .default(default_model.to_string())
        .interact_text()?;
    let model = model.trim().to_string();

    let alias = "default";
    config.providers.models.ensure(&picked, alias);

    let prefix = format!("providers.models.{picked}.{alias}");
    if !api_key.is_empty() {
        config.set_secret_persistent(&format!("{prefix}.api_key"), api_key)?;
    }
    config.set_prop_persistent(&format!("{prefix}.model"), &model)?;

    if config.risk_profiles.get("default").is_none() {
        let mut default_profile = zeroclaw_config::schema::RiskProfileConfig::default();
        default_profile.ensure_default_auto_approve();
        config.risk_profiles.insert("default".to_string(), default_profile);
        config.mark_dirty("risk_profiles.default");
    }

    if config.runtime_profiles.get("default").is_none() {
        config.runtime_profiles.insert("default".to_string(), zeroclaw_config::schema::RuntimeProfileConfig::default());
        config.mark_dirty("runtime_profiles.default");
    }

    let agent_prefix = "agents.assistant";
    config.set_prop_persistent(&format!("{agent_prefix}.model_provider"), &format!("{picked}.{alias}"))?;
    config.set_prop_persistent(&format!("{agent_prefix}.risk_profile"), "default")?;
    config.set_prop_persistent(&format!("{agent_prefix}.runtime_profile"), "default")?;

    config.save_dirty().await?;
    println!();
    println!("{}", console::style("✓ Configuration saved successfully!").green().bold());
    println!("Config file: {}", config.config_path.display());
    println!();

    Ok(())
}

fn print_logs(config: &Config) -> Result<()> {
    let log_path = config.config_path.parent()
        .context("Failed to get config path parent")?
        .join("state/runtime-trace.jsonl");

    if !log_path.exists() {
        println!("No logs found at {}", log_path.display());
        return Ok(());
    }

    let file = std::fs::File::open(&log_path)?;
    let reader = std::io::BufReader::new(file);

    for line in std::io::BufRead::lines(reader) {
        let line = line?;
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line) {
            let timestamp = event["@timestamp"].as_str().unwrap_or("");
            let severity = event["severity_text"].as_str().unwrap_or("INFO");
            let category = event["event"]["category"].as_str().unwrap_or("");
            let action = event["event"]["action"].as_str().unwrap_or("");
            let message = event["message"].as_str().unwrap_or("");

            let severity_styled = match severity {
                "ERROR" => console::style(severity).red().bold(),
                "WARN" => console::style(severity).yellow().bold(),
                "INFO" => console::style(severity).green(),
                _ => console::style(severity).dim(),
            };

            let ts_display = if timestamp.len() >= 19 {
                format!("{} {}", &timestamp[0..10], &timestamp[11..19])
            } else {
                timestamp.to_string()
            };

            println!(
                "[{}] {} [{}:{}] {}",
                console::style(ts_display).dim(),
                severity_styled,
                console::style(category).cyan(),
                console::style(action).blue(),
                message
            );
        } else {
            println!("{}", line);
        }
    }

    Ok(())
}

struct SessionFile {
    path: std::path::PathBuf,
    modified: std::time::SystemTime,
    preview: String,
}

fn get_sessions_dir() -> std::path::PathBuf {
    let base = std::env::var("OPENZ_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            directories::BaseDirs::new()
                .map(|bd| bd.home_dir().join(".openz"))
                .unwrap_or_else(|| std::path::PathBuf::from(".openz"))
        });
    base.join("sessions")
}

fn list_sessions() -> Vec<SessionFile> {
    let mut sessions = Vec::new();
    let dirs = vec![
        get_sessions_dir(),
        // Fallback
        directories::BaseDirs::new()
            .map(|bd| bd.home_dir().join(".zeroclaw").join("sessions"))
            .unwrap_or_else(|| std::path::PathBuf::from(".zeroclaw/sessions")),
    ];

    for dir in dirs {
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                        if let Ok(metadata) = entry.metadata() {
                            let modified = metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                            
                            // Try to get a short preview of the last message
                            let mut preview = "No messages".to_string();
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                if let Ok(state) = serde_json::from_str::<serde_json::Value>(&content) {
                                    if let Some(history) = state["history"].as_array() {
                                        let mut last_msg = None;
                                        for msg in history.iter().rev() {
                                            let role = msg["role"].as_str().unwrap_or("");
                                            if role == "user" || role == "assistant" {
                                                last_msg = Some(msg["content"].as_str().unwrap_or(""));
                                                break;
                                            }
                                        }
                                        if let Some(msg) = last_msg {
                                            let trimmed_msg = msg.trim().replace('\n', " ");
                                            if trimmed_msg.len() > 50 {
                                                preview = format!("{}...", &trimmed_msg[0..50]);
                                            } else {
                                                preview = trimmed_msg.to_string();
                                            }
                                        }
                                    }
                                }
                            }

                            sessions.push(SessionFile {
                                path,
                                modified,
                                preview,
                            });
                        }
                    }
                }
            }
        }
    }

    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
    sessions
}
