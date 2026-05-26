use crate::cron::{self, CronJob, DeliveryConfig, Schedule, SessionTarget};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use zeroclaw_config::schema::Config;

/// Pre-packaged agent workflow template (pre-built task kit)
///
/// Pre-edit ritual:
/// This is the source of truth — created here. It represents the static task blueprint templates definition structure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlueprintTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_schedule: String,
    pub required_tools: Vec<String>,
    pub system_prompt: String,
}

pub struct BlueprintRegistry;

impl BlueprintRegistry {
    /// Return all default/pre-built blueprint templates.
    pub fn list() -> Vec<BlueprintTemplate> {
        vec![
            BlueprintTemplate {
                id: "daily-osint".to_string(),
                name: "Daily OSINT & News Digest".to_string(),
                description: "Periodically searches for specified keywords/topics (e.g. security news, industry updates), summarizes findings, and prepares a daily intelligence briefing.".to_string(),
                default_schedule: "0 9 * * *".to_string(),
                required_tools: vec![
                    "web_search".to_string(),
                    "web_fetch".to_string(),
                    "file_write".to_string(),
                ],
                system_prompt: "You are an OSINT (Open Source Intelligence) collection agent. Your task is to:\n1. Search for the latest news and updates on target topics (default: AI advancements and cybersecurity news from the last 24 hours).\n2. Retrieve and read content from relevant sources using search results.\n3. Synthesize the findings into a concise, high-value Markdown intelligence report.\n4. Save the report to a daily log file in the workspace (e.g. `osint_report_YYYY-MM-DD.md`).\nMake sure to highlight critical risks, security alerts, and key trends.".to_string(),
            },
            BlueprintTemplate {
                id: "site-monitoring".to_string(),
                name: "Website Status & Content Monitor".to_string(),
                description: "Monitors target web pages/endpoints for status changes, downtime, or content drift, and logs anomalies.".to_string(),
                default_schedule: "*/30 * * * *".to_string(),
                required_tools: vec![
                    "http_request".to_string(),
                    "file_write".to_string(),
                ],
                system_prompt: "You are a Site Reliability and Monitoring agent. Your task is to:\n1. Perform HTTP requests to monitor the configured list of URLs/endpoints.\n2. Check the response status codes, latency, and parse the content to check for structural anomalies, defacement, or unexpected changes.\n3. Compare the current state with previous logs (stored in `site_monitor_history.json` or similar) to detect drifts or outages.\n4. Log any failures, performance warnings, or content drift immediately. If errors are found, write an incident report to the alerts log file.".to_string(),
            },
            BlueprintTemplate {
                id: "lead-generation".to_string(),
                name: "Automated Lead Generator & Enrichment".to_string(),
                description: "Searches targeted resources (directories, public listings) for potential leads, parses contact details, and updates a structured CSV/database of prospects.".to_string(),
                default_schedule: "0 18 * * 1-5".to_string(),
                required_tools: vec![
                    "web_search".to_string(),
                    "web_fetch".to_string(),
                    "file_write".to_string(),
                ],
                system_prompt: "You are a Lead Generation and B2B Enrichment agent. Your task is to:\n1. Perform targeted searches for companies, organizations, or professionals matching specified ICP (Ideal Customer Profile) criteria in the target geographic area or sector.\n2. Fetch information from public listings, company websites, or profile directories.\n3. Extract key details such as company name, contact person, email, phone number, industry, and description.\n4. Write or append the extracted details in a structured CSV format (`leads_generated.csv`), ensuring duplicate entries are merged or skipped.".to_string(),
            },
        ]
    }

    /// Retrieve a template by its ID.
    pub fn get(id: &str) -> Option<BlueprintTemplate> {
        Self::list().into_iter().find(|t| t.id == id)
    }
}

/// Bind/schedule a pre-packaged blueprint template into a `CronJob` in the database.
pub fn bind_blueprint_to_cron(
    config: &Config,
    blueprint_id: &str,
    agent_alias: &str,
    schedule_override: Option<&str>,
    model: Option<String>,
    delivery: Option<DeliveryConfig>,
) -> Result<CronJob> {
    let template = BlueprintRegistry::get(blueprint_id)
        .with_context(|| format!("Blueprint template '{}' not found", blueprint_id))?;

    let cron_expression = schedule_override.unwrap_or(&template.default_schedule);
    let schedule = Schedule::Cron {
        expr: cron_expression.to_string(),
        tz: None,
    };

    cron::add_agent_job(
        config,
        agent_alias,
        Some(template.name.clone()),
        schedule,
        &template.system_prompt,
        SessionTarget::Isolated,
        model,
        delivery,
        false, // delete_after_run
        Some(template.required_tools.clone()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_config(tmp: &TempDir) -> Config {
        let mut config = Config {
            data_dir: tmp.path().join("data"),
            config_path: tmp.path().join("config.toml"),
            ..Config::default()
        };
        // Add a mock agent config so validation passes
        config.agents.insert(
            "test-agent".to_string(),
            zeroclaw_config::schema::AliasedAgentConfig {
                model_provider: zeroclaw_config::providers::ModelProviderRef::new(
                    "openai.default".to_string(),
                ),
                risk_profile: "default".to_string(),
                runtime_profile: "default".to_string(),
                ..Default::default()
            },
        );
        std::fs::create_dir_all(&config.data_dir).unwrap();
        config
    }

    #[test]
    fn list_returns_prebuilt_blueprints() {
        let blueprints = BlueprintRegistry::list();
        assert_eq!(blueprints.len(), 3);
        assert!(blueprints.iter().any(|b| b.id == "daily-osint"));
        assert!(blueprints.iter().any(|b| b.id == "site-monitoring"));
        assert!(blueprints.iter().any(|b| b.id == "lead-generation"));
    }

    #[test]
    fn get_returns_correct_blueprint() {
        let blueprint = BlueprintRegistry::get("site-monitoring");
        assert!(blueprint.is_some());
        let b = blueprint.unwrap();
        assert_eq!(b.name, "Website Status & Content Monitor");
        assert_eq!(b.default_schedule, "*/30 * * * *");
        assert!(b.required_tools.contains(&"http_request".to_string()));
    }

    #[test]
    fn get_nonexistent_returns_none() {
        assert!(BlueprintRegistry::get("invalid-id").is_none());
    }

    #[test]
    fn test_bind_blueprint_creates_cron_job() {
        let tmp = TempDir::new().unwrap();
        let config = test_config(&tmp);

        let job =
            bind_blueprint_to_cron(&config, "daily-osint", "test-agent", None, None, None).unwrap();

        assert_eq!(job.job_type, cron::JobType::Agent);
        assert_eq!(job.agent_alias, "test-agent");
        assert_eq!(job.expression, "0 9 * * *");
        assert_eq!(
            job.prompt.as_deref(),
            Some(
                "You are an OSINT (Open Source Intelligence) collection agent. Your task is to:\n1. Search for the latest news and updates on target topics (default: AI advancements and cybersecurity news from the last 24 hours).\n2. Retrieve and read content from relevant sources using search results.\n3. Synthesize the findings into a concise, high-value Markdown intelligence report.\n4. Save the report to a daily log file in the workspace (e.g. `osint_report_YYYY-MM-DD.md`).\nMake sure to highlight critical risks, security alerts, and key trends."
            )
        );
        assert_eq!(
            job.allowed_tools,
            Some(vec![
                "web_search".to_string(),
                "web_fetch".to_string(),
                "file_write".to_string()
            ])
        );

        // Check it was persisted in SQLite
        let persisted = cron::get_job(&config, &job.id).unwrap();
        assert_eq!(persisted.id, job.id);
        assert_eq!(persisted.expression, "0 9 * * *");
    }

    #[test]
    fn test_bind_blueprint_with_schedule_override() {
        let tmp = TempDir::new().unwrap();
        let config = test_config(&tmp);

        let job = bind_blueprint_to_cron(
            &config,
            "daily-osint",
            "test-agent",
            Some("*/15 * * * *"),
            Some("gpt-4".to_string()),
            None,
        )
        .unwrap();

        assert_eq!(job.expression, "*/15 * * * *");
        assert_eq!(job.model, Some("gpt-4".to_string()));
    }
}
