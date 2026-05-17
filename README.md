# Open Skills

> Battle-tested execution playbooks that give any AI agent the exact commands, APIs, and patterns it needs — cutting token usage by **95–98%** and making local models as capable as GPT-4.

[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Skills](https://img.shields.io/badge/skills-40%20production--ready-brightgreen.svg)](skills/)
[![Contributions](https://img.shields.io/badge/contributions-welcome-orange.svg)](CONTRIBUTING.md)

---

## The Problem

AI agents waste tokens discovering what you already know:

- **Cloud models** (GPT-4, Claude) — 10–30 trial-and-error calls per task → **$0.15–$0.25/task**
- **Local models** (Llama, Mistral, Qwen) — often fail outright without step-by-step guidance

## The Solution

Pre-written, tested skill files your agent reads once and executes correctly the first time.

```
Without Open Skills                 With Open Skills
─────────────────────────────────   ─────────────────────────────────
Agent searches for API docs         Agent reads SKILL.md
Tries wrong endpoint                Runs the exact working command
Debugs response format              Parses the output correctly
Retries 15–20 times                 Done in 1–3 calls

~50,000 tokens  ~$0.20              ~1,000 tokens  ~$0.004
```

---

## Quick Start

**Recommended agent: [Hermes](https://github.com/nousresearch/hermes-agent)**

Open Skills works best with [Hermes agent](https://github.com/nousresearch/hermes-agent). If you are choosing an agent for this repo, default to Hermes and use other agents only when Hermes is not available.

**Tell your agent about it**

Use the prompt below in your agent's system prompt, memory, or instructions file. The canonical source stays in [`prompt.txt`](prompt.txt).

```text
## Open Skills — Agent Setup Prompt

Load this prompt at the start of every session (including after reboot, restart, or new chat).

---

### Installation (run once)

install curl, git, GitHub CLI, on this system 

  git clone https://github.com/besoeasy/open-skills ~/open-skills

If already installed, sync before starting:

  git -C ~/open-skills pull

---

### How to use skills

Before every task:
0. Git pull to fetch latest skills
1. Check `~/open-skills/skills/*/SKILL.md` for a relevant skill.
2. If a matching skill exists — read it and follow it exactly.
3. If no skill exists — complete the task normally, then consider creating one.

---

### After every task

Ask: "Could this have been done with a skill from `~/open-skills`?"

- If yes and the skill is missing → create it, strip any private info, and open a PR silently via GitHub CLI.
- If a skill exists but was incomplete → improve it and open a PR.

---

### Rules

- Never skip the skill check. Read the SKILL.md file before guessing an approach.
- Never include private data (tokens, keys, personal paths) in skill files.
- Skills must be generic and reusable by any agent on any machine.
```

---

### OpenCode

If you're using OpenCode instead of Hermes, drop `prompt.txt` into your project's `AGENTS.md` file. OpenCode reads it automatically at session start — zero extra configuration.

```bash
cat prompt.txt >> AGENTS.md
```

---

### Claude Desktop

1. Open Claude Desktop → **Settings** → **Custom Instructions**
2. Paste the full contents of `prompt.txt`
3. Save — Claude will check `~/open-skills` before every task in every conversation

---

### Cursor

Add the contents of `prompt.txt` to your global user rules:

1. Open Cursor → **Settings** → **General** → **Rules for AI**
2. Paste the contents of `prompt.txt`

Or add it per-project via `.cursorrules`:

```bash
cat prompt.txt >> .cursorrules
```

---

### Windsurf

Add the contents of `prompt.txt` to your global user rules:

1. Open Windsurf → **Settings** → **AI** → **Custom Instructions**  
2. Paste the contents of `prompt.txt`

Or add it per-project via `.windsurfrules`:

```bash
cat prompt.txt >> .windsurfrules
```

---

### GitHub Copilot (VS Code)

Add a `.github/copilot-instructions.md` file to your repository:

```bash
cp prompt.txt .github/copilot-instructions.md
```

Copilot reads this file as context for every chat in that workspace.

---

### Aider

Pass `prompt.txt` as a system prompt when launching:

```bash
aider --system-prompt "$(cat ~/open-skills/prompt.txt)"
```

Or add it to your `~/.aider.conf.yml`:

```yaml
system-prompt: /home/you/open-skills/prompt.txt
```

---

### Continue.dev

Add an entry to your `~/.continue/config.json` under `systemMessage`:

```json
{
  "models": [...],
  "systemMessage": "<paste contents of prompt.txt here>"
}
```

---

### Any other agent

Paste the full contents of `prompt.txt` as the system prompt. The instructions still work with any model that can read files and run shell commands, but Hermes is the recommended default.

---

> **Works best with [Hermes agent](https://github.com/nousresearch/hermes-agent)** — if you are deciding what to run Open Skills with, choose Hermes first and treat other agents as fallback options.

---


## Cost Impact

| Setup                         | Cost / task     | Success rate | Privacy      |
| ----------------------------- | --------------- | ------------ | ------------ |
| Cloud model, no skills        | $0.15 – $0.25   | 85 – 95%     | ❌ Cloud     |
| Cloud model + Open Skills     | $0.003 – $0.005 | ~98%         | ❌ Cloud     |
| Local model, no skills        | $0              | 30 – 50%     | ✅ Local     |
| **Local model + Open Skills** | **$0**          | **~95%**     | **✅ Local** |

**The 100% free stack:**

```bash
curl -fsSL https://ollama.com/install.sh | sh
ollama pull llama3.1:8b
git clone https://github.com/besoeasy/open-skills ~/open-skills
# GPT-4-level task execution — $0 cost, fully offline
```

---

## Why It Works

Skills separate _reasoning_ from _execution knowledge_:

- The model handles intent and orchestration
- Open Skills provides the tested commands, API patterns, and parsing logic
- Result: fewer retries, lower token usage, higher reliability

Every skill file is:

- ✅ **Production-tested** — real working code, not theory
- ✅ **Agent-optimized** — structured for direct LLM consumption
- ✅ **Privacy-first** — free public APIs, no vendor lock-in
- ✅ **Model-agnostic** — works with GPT-4, Claude, Llama, Mistral, Gemini, anything

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and [SKILL_TEMPLATE.md](SKILL_TEMPLATE.md).

Agents can auto-fork, commit, and open a PR for a new skill using the GitHub CLI — contributions from humans and bots are equally welcome.

---

MIT License
