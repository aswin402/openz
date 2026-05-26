# "Hands" Task Blueprints (Autonomous Task Kits)

ZeroClaw includes pre-packaged agent workflows (tools, prompt templates, and default cron schedules) for common background automation tasks.

## Concept

Instead of writing complex prompts and planning cron jobs from scratch, developers can bind specialized background tasks directly to a cron schedule with 1 click. These kits are also known as "Hands" because they represent pre-configured hands-off background agents.

## Built-in Blueprints

The registry defines three built-in task templates:

### 1. Daily OSINT & News Digest (`daily-osint`)
- **Description**: Periodically searches for specified keywords/topics (e.g. security news, industry updates), summarizes findings, and prepares a daily intelligence briefing.
- **Recommended Schedule**: Daily at 9:00 AM (`0 9 * * *`)
- **Required Tools**: `web_search`, `web_fetch`, `file_write`
- **System Prompt**: Focuses on fetching news, reading source pages, synthesizing Markdown briefings, and logging reports into a daily workspace file.

### 2. Website Status & Content Monitor (`site-monitoring`)
- **Description**: Monitors target web pages/endpoints for status changes, downtime, or content drift, and logs anomalies.
- **Recommended Schedule**: Every 30 minutes (`*/30 * * * *`)
- **Required Tools**: `http_request`, `file_write`
- **System Prompt**: Performs health checking on a set of URLs, compares content structure against previous logs, and outputs alerts on drift or outage.

### 3. Automated Lead Generator & Enrichment (`lead-generation`)
- **Description**: Searches targeted resources (directories, public listings) for potential leads, parses contact details, and updates a structured CSV/database of prospects.
- **Recommended Schedule**: Weekdays at 6:00 PM (`0 18 * * 1-5`)
- **Required Tools**: `web_search`, `web_fetch`, `file_write`
- **System Prompt**: Identifies targets matching ideal customer profiles, fetches public listings, extracts company names and emails, and appends unique records to a CSV.

---

## CLI Usage

Manage and schedule blueprints using the `zeroclaw hands` command suite:

### List available blueprints

```bash
zeroclaw hands list
```

### Bind a blueprint to an agent

Bind a template to an agent (which creates an agent-type recurring cron job):

```bash
zeroclaw hands bind daily-osint --agent researcher
```

You can optionally override the default schedule or LLM model:

```bash
zeroclaw hands bind site-monitoring --agent ops-bot --schedule "*/10 * * * *" --model gpt-4o
```
