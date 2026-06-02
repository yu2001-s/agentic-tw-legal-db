---
name: twlaw-regulations
description: Use when the user asks to query, verify, cite, or research Taiwan laws, regulations, articles, pcodes, law histories, legislative reasons, Chinese or English law/order text, or recent MOJ law/order/draft updates through the `twlaw` CLI. This skill is CLI-only and must not use MCP.
---

# twlaw regulations

Use the local Rust CLI `twlaw` as the agent interface. Do not start or configure an MCP server.

## Contract

- Use `twlaw ... --json` for agent calls.
- If availability is uncertain, run `twlaw --version`; if missing, stop and report that `twlaw` is not installed. Do not install from this skill.
- Preserve `source_url`, `retrieved_at`, cache fields, amendment/history metadata, and coverage limits when citing results.
- Treat output as legal research material, not legal advice.
- Prefer no-credential public sources and local cached/bundled data. Keep live government HTTP query concurrency low.

## Strategy

- Use `regulation pcode` when the user gives a law name and you need a stable pcode.
- Use `regulation query` for current article text or article ranges.
- Use `regulation query --include-history` when amendment history matters.
- Use `legislative history` when the user asks for 法律沿革, 立法理由, or Legislative Yuan source URLs. Use `--all-versions` with `--article` when the user asks for a specific article's amendment history and does not already know the ROC action date.
- For repeated MOJ law/order work, prefer `moj sync` once, then `moj search` or `moj get`; this uses the official no-token MOJ OpenAPI ZIP cache instead of repeated live page fetches.
- For recency-sensitive law/order questions, call `moj updates` before relying on cached data.
- Start with narrow metadata queries before fetching long text.

## Commands

```bash
twlaw regulation pcode --law "民法" --json
twlaw regulation search --keyword "勞動" --limit 20 --json
twlaw regulation query --law "民法" --article "184" --json
twlaw regulation query --law "民法" --from "184" --to "198" --json
twlaw regulation query --pcode "B0000001" --include-history --json
twlaw legislative history --law "入出國及移民法" --json
twlaw legislative history --law "入出國及移民法" --date 1120530 --article 3 --include-reasons --json
twlaw legislative history --law "中華民國刑法" --article 339-4 --include-reasons --all-versions --json
```

MOJ OpenAPI bulk law/order datasets:

```bash
twlaw moj datasets --json
twlaw moj status --dataset all --json
twlaw moj sync --dataset ch-law --json
twlaw moj search --dataset ch-order --keyword "勞動" --include-articles --limit 20 --json
twlaw moj get --dataset ch-law --law "民法" --article "184" --json
twlaw moj search --dataset en-law --keyword "Personal Data" --limit 10 --json
twlaw moj updates --kind order --limit 10 --json
twlaw moj updates --kind draft --keyword "個資" --json
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

Regulation commands use bundled pcode metadata but fetch article text from `law.moj.gov.tw`. MOJ OpenAPI commands cache extracted official ZIP JSON under the local twlaw cache directory and can auto-sync a missing dataset.
