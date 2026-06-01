# agentic tw legal db

CLI-native Taiwan public legal data plugin for Codex, Claude Code, Cursor, Windsurf, Gemini CLI, and other coding agents that can run shell commands. The agent interface is the Rust CLI `twlaw`; this repository intentionally does not ship an MCP server.

`twlaw` is designed for agents: stable JSON stdout, JSON errors, no default government API credentials, source-status commands, bounded live requests, and cached/bundled workflows for high-volume use.

Traditional Chinese documentation: [README.zh-TW.md](README.zh-TW.md)

## Install By Pasting A Prompt

Most users should not install this by hand. Paste this exact prompt into the coding agent they already use:

```text
Install the public Taiwan legal research CLI plugin from https://github.com/yu2001-s/agentic-tw-legal-db.
```

Manual install:

```bash
git clone https://github.com/yu2001-s/agentic-tw-legal-db.git
cd agentic-tw-legal-db/plugins/agentic-tw-legal-db
scripts/install.sh
```

## What It Can Query

| Area | Commands | What it covers |
| --- | --- | --- |
| Source/agent guide | `twlaw sources ...`, `twlaw agent guide` | Coverage status, no-credential policy, gaps, recommended workflows. |
| MOJ laws and regulations | `twlaw regulation ...`, `twlaw moj ...` | Law names, pcodes, articles, history metadata, no-token MOJ OpenAPI ZIP datasets for Chinese/English laws and orders. |
| MOJ legal updates | `twlaw moj updates ...` | Recent law, order, rule, local-law, and draft notices with official links. |
| MOJ agreements | `twlaw moj agreements ...` | Treaty and cross-strait agreement listings, categories, and keyword search. |
| MOJ legal references | `twlaw mojlaw search ...` | Administrative interpretations, legal consultation opinions, legal issue seminars, objection decisions, Constitutional Court/Judicial Yuan references, and precedent materials from the MOJ retrieval system. |
| Judicial Yuan judgments | `twlaw judgment search/get/special ...` | Public judgment search, full-text fetch, simple cases, declaration judgments, and public-summons rulings. |
| Constitutional Court | `twlaw interpretation ...` | Bundled interpretations/rulings, live current judgments, terminal-case search, citations, reasoning/opinion snippets. |
| Government open data | `twlaw open-data legal-catalog ...` | Cached discovery of legal-related data.gov.tw datasets with source and license metadata. |

## Agent Contract

- Always call `twlaw ... --json`.
- stdout is JSON on success and error.
- Preserve `source_url`, `retrieved_at`, cache, pagination, and truncation fields.
- Prefer metadata search before full-text fetch.
- For repeated MOJ law/order work, run `twlaw moj sync --dataset <id> --json` first.
- Cached and bundled commands are parallel-friendly; live government HTTP queries should stay low-concurrency.
- Treat output as legal research material, not legal advice.

## Examples

```bash
twlaw sources status --json
twlaw agent guide --json
twlaw regulation query --pcode B0000001 --article 184 --json
twlaw moj sync --dataset ch-law --json
twlaw moj search --dataset en-law --keyword "Civil" --include-articles --limit 20 --json
twlaw moj updates --kind order --limit 10 --json
twlaw moj agreements --kind treaty --keyword "CEDAW" --json
twlaw mojlaw search --kind admin-interpretation --keyword "<term>" --limit 10 --json
twlaw judgment search --keyword "<term>" --max-results 10 --json
twlaw interpretation current --limit 10 --json
twlaw open-data legal-catalog --keyword "judgment" --limit 10 --json
```

## Agent Integrations

| Agent surface | Status | Files |
| --- | --- | --- |
| Codex | Native plugin skill | `.codex-plugin/plugin.json`, `skills/agentic-tw-legal-db/SKILL.md` |
| Claude Code | Supported | `CLAUDE.md`, `.claude/commands/*.md` |
| Generic terminal agents | Supported | `AGENTS.md` |
| MCP clients | Not shipped | Add a wrapper only if needed |

## Test And Publish

```bash
scripts/test.sh
scripts/release-check.sh --marketplace
```

This project does not claim affiliation with Taiwan government agencies, OpenAI, Anthropic, or any third-party marketplace. See `docs/PRIVACY.md`, `docs/TERMS.md`, and `docs/PUBLISHING.md`.
