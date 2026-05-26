pub use zeroclaw_runtime::hands::*;

use crate::config::Config;
use anyhow::Result;

/// Bail with a clear error if the named agent isn't configured.
fn require_configured_agent(config: &Config, agent_alias: &str) -> Result<()> {
    if config.agent(agent_alias).is_none() {
        ::zeroclaw_log::record!(
            WARN,
            ::zeroclaw_log::Event::new(module_path!(), ::zeroclaw_log::Action::Reject)
                .with_outcome(::zeroclaw_log::EventOutcome::Failure)
                .with_attrs(::serde_json::json!({"agent_alias": agent_alias})),
            "hands CLI rejected: unknown agent alias"
        );
        anyhow::bail!("Unknown agent {agent_alias:?} (no [agents.{agent_alias}] entry configured)");
    }
    Ok(())
}

pub fn handle_command(command: crate::HandsCommands, config: &Config) -> Result<()> {
    match command {
        crate::HandsCommands::List => {
            let blueprints = BlueprintRegistry::list();
            if blueprints.is_empty() {
                println!("No prebuilt blueprint templates available.");
                return Ok(());
            }

            println!(
                "🚀 Available Pre-built Task Blueprints ({}):",
                blueprints.len()
            );
            for bp in blueprints {
                println!("\n• ID: {}", bp.id);
                println!("  Name       : {}", bp.name);
                println!("  Description: {}", bp.description);
                println!("  Schedule   : {} (recommended)", bp.default_schedule);
                println!("  Required Tools: {}", bp.required_tools.join(", "));
            }
            println!();
            Ok(())
        }
        crate::HandsCommands::Bind {
            blueprint_id,
            agent_alias,
            schedule,
            model,
        } => {
            require_configured_agent(config, &agent_alias)?;
            let job = bind_blueprint_to_cron(
                config,
                &blueprint_id,
                &agent_alias,
                schedule.as_deref(),
                model,
                None, // default delivery config
            )?;

            println!(
                "✓ Successfully bound blueprint '{}' to agent '{}'!",
                blueprint_id, agent_alias
            );
            println!("  Cron Job ID: {}", job.id);
            println!("  Schedule   : {}", job.expression);
            Ok(())
        }
    }
}
