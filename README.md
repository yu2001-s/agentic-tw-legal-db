# agentic tw legal db

`agentic-tw-legal-db` gives coding agents a local CLI, `twlaw`, for Taiwan legal research. It is built for Codex, Claude Code, Cursor, Windsurf, Gemini CLI, and other agents that can run shell commands.

The project is CLI-first: agents call `twlaw ... --json`, get structured JSON back, and preserve official source URLs in their answers. It does not require users to apply for government API credentials, and it does not require MCP setup.

Traditional Chinese documentation: [README.zh-TW.md](README.zh-TW.md)

## What It Does

- Helps agents query Taiwan laws, regulations, orders, judgments, constitutional decisions, MOJ legal-reference material, and legal open-data catalog records.
- Returns machine-readable JSON for both successful results and errors.
- Exposes source-discovery commands so agents can explain coverage, gaps, freshness, and credential requirements before research starts.
- Uses cached or bundled data where practical, while keeping live government-site requests bounded.
- Treats legal data as research material, not legal advice.

## Data Sources

| Area | Official source | Access model | Current coverage |
| --- | --- | --- | --- |
| Laws and regulations | Ministry of Justice, `law.moj.gov.tw` | Public HTML plus no-token ZIP datasets | Law-name search, pcode lookup, article/full-law fetch, law-history metadata, Chinese and English law/order bulk sync and search. |
| Legal updates and agreements | Ministry of Justice, `law.moj.gov.tw` | Public HTML | Recent law/order/rule/local/draft notices, Gazette links returned by MOJ pages, treaty listings, and cross-strait agreement listings. |
| MOJ legal references | Ministry of Justice, `mojlaw.moj.gov.tw` | Public HTML | Administrative interpretations, legal consultation opinions, legal issue seminars, objection decisions, and related legal-reference search results. |
| Judicial Yuan judgments | Judicial Yuan, `judgment.judicial.gov.tw` | Bounded public HTML | Public judgment search, full-text fetch by returned id or URL, and special searches such as simple cases, declaration judgments, and public-summons rulings. |
| Constitutional Court | Judicial Yuan Constitutional Court, `cons.judicial.gov.tw` | Bundled snapshots plus bounded public HTML/AJAX | Search bundled interpretations and rulings, fetch current judgment lists, search terminal cases, and extract citations or reasoning snippets. |
| Government open-data catalog | `data.gov.tw` | No-token public catalog export | Discovery of legal-related datasets with agency, license, source URL, and catalog metadata. |
| Judicial Yuan JList/JDoc API | `data.judicial.gov.tw/jdg/api` | Token-required official API | Tracked as reference coverage only; not used by default because the plugin must work without an application step. |

## Install By Pasting A Prompt

Paste this exact prompt into the coding agent you use:

```text
Install the public Taiwan legal research CLI plugin from https://github.com/yu2001-s/agentic-tw-legal-db.
```

Start here:

- [Plugin README](plugins/agentic-tw-legal-db/README.md)
- [Plugin README in Traditional Chinese](plugins/agentic-tw-legal-db/README.zh-TW.md)
- [Generic agent instructions](plugins/agentic-tw-legal-db/AGENTS.md)
- [Claude Code instructions](plugins/agentic-tw-legal-db/CLAUDE.md)
- [Publishing checklist](plugins/agentic-tw-legal-db/docs/PUBLISHING.md)

## Agent Compatibility

The plugin currently supports:

- Codex through native plugin skills in `plugins/agentic-tw-legal-db/skills/`.
- Claude Code through `CLAUDE.md` and `.claude/commands/`.
- Generic terminal agents through `AGENTS.md`.
- Other agents through direct shell use of `twlaw`.

The published package intentionally keeps installation simple: users paste one prompt into their agent, and the agent performs the clone, install, and verification steps.
