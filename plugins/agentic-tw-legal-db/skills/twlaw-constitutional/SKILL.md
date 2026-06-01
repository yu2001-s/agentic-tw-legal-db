---
name: twlaw-constitutional
description: Use when the user asks to search, retrieve, verify, cite, or analyze Taiwan Constitutional Court interpretations, constitutional judgments, procedure rulings, non-acceptance decisions, terminal cases, citations, reasoning, opinions, or current Constitutional Court materials through the `twlaw` CLI. This skill is CLI-only and must not use MCP.
---

# twlaw constitutional

Use the local Rust CLI `twlaw` as the agent interface. Do not start or configure an MCP server.

## Contract

- Use `twlaw ... --json` for agent calls.
- If availability is uncertain, run `twlaw --version`; if missing, stop and report that `twlaw` is not installed. Do not install from this skill.
- Preserve `source_url`, `retrieved_at`, citation ids, dates, cache fields, and coverage limits when citing results.
- Treat output as legal research material, not legal advice.
- Prefer bundled/offline Constitutional Court data where it answers the question.

## Strategy

- Call `interpretation current` first when recency matters.
- Use `interpretation terminal` for procedure rulings, non-acceptance decisions, and terminal-case coverage.
- Use bundled `interpretation search` and `interpretation get` for detailed interpretation or judgment work.
- Prefer keyword snippets such as `--reasoning-keyword` before `--include-reasoning`.
- Use `interpretation citations` when the user asks how an interpretation is cited or connected to other materials.

## Commands

```bash
twlaw interpretation current --limit 10 --json
twlaw interpretation search --keyword "集會自由" --limit 10 --json
twlaw interpretation search --include-old=false --limit 10 --json
twlaw interpretation search --no-old --limit 10 --json
twlaw interpretation get "釋字748" --json
twlaw interpretation get "釋字748" --reasoning-keyword "婚姻" --json
twlaw interpretation get "111年憲判字第1號" --include-reasoning --json
twlaw interpretation terminal --kind procedure-ruling --limit 10 --json
twlaw interpretation terminal --kind non-acceptance --keyword "婚姻" --json
twlaw interpretation citations "釋字748" --include-context --json
```

## Exit Codes

- `0`: success
- `2`: invalid input
- `3`: not found
- `5`: upstream page structure changed or parser failed
- `6`: network error
- `7`: bundled data error

On non-zero exits, stdout is still JSON with `success: false` and an `error` object.

## Limits

Bundled Constitutional Court data is offline and fast. Live current and terminal-case commands depend on public Constitutional Court pages and should be used narrowly for fresh or procedure-specific coverage.
