---
name: twlaw-open-data
description: Use when the user asks to discover, list, verify, cite, or evaluate Taiwan government open-data datasets related to law, courts, justice, judgments, regulations, agencies, licenses, or source URLs through the `twlaw open-data legal-catalog` CLI. This skill is CLI-only and must not use MCP.
---

# twlaw open data

Use the local Rust CLI `twlaw` as the agent interface. Do not start or configure an MCP server.

## Contract

- Use `twlaw ... --json` for agent calls.
- If availability is uncertain, run `twlaw --version`; if missing, stop and report that `twlaw` is not installed. Do not install from this skill.
- Preserve dataset title, agency, license, `source_url`, `retrieved_at`, cache fields, and coverage limits when citing results.
- Treat catalog output as discovery metadata, not proof that a dataset has the needed records.
- Inspect returned licenses and source URLs before reusing content.

## Strategy

- Use `open-data legal-catalog` to discover official legal datasets from the cached data.gov.tw catalog export.
- Filter by keyword or agency for targeted discovery.
- Report whether results are catalog metadata, cached data, or live data.
- If the user asks to use a discovered dataset, inspect the source URL/license and explain any credential, format, or coverage gap before relying on it.

## Commands

```bash
twlaw open-data legal-catalog --limit 30 --json
twlaw open-data legal-catalog --keyword "判決" --limit 10 --json
twlaw open-data legal-catalog --keyword "judgment" --limit 10 --json
twlaw open-data legal-catalog --agency "司法院" --json
twlaw open-data legal-catalog --agency "法務部" --json
```

## Exit Codes

- `0`: success
- `2`: invalid input
- `3`: not found
- `6`: network error
- `7`: bundled data error

On non-zero exits, stdout is still JSON with `success: false` and an `error` object.

## Limits

The open-data catalog command caches the official data.gov.tw CSV export locally. Results are catalog records and license/source metadata; they are not a substitute for inspecting the underlying dataset.
