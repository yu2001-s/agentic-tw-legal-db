---
name: twlaw-judgments
description: Use when the user asks to search, retrieve, verify, cite, or analyze Taiwan Judicial Yuan judgments, court decisions, case numbers, simple cases, declaration judgments, public-summons rulings, or judgment source URLs through the `twlaw` CLI. This skill is CLI-only and must not use MCP.
---

# twlaw judgments

Use the local Rust CLI `twlaw` as the agent interface. Do not start or configure an MCP server.

## Contract

- Use `twlaw ... --json` for agent calls.
- If availability is uncertain, run `twlaw --version`; if missing, stop and report that `twlaw` is not installed. Do not install from this skill.
- Preserve `source_url`, `retrieved_at`, `jid`, court, date, case metadata, cache fields, and coverage limits when citing results.
- Treat output as legal research material, not legal advice.
- Prefer narrow metadata searches before fetching long judgment text. Keep live Judicial Yuan HTTP query concurrency low.

## Strategy

- Use `judgment search` first and only call `judgment get` when a specific `jid` or source URL is needed.
- For a known case number, use `--case-word` and `--case-number`; do not put a case number into `--keyword`.
- Add filters such as `--court`, `--case-type`, `--year-from`, and result limits when available.
- Use `judgment special` for simple cases, declaration judgments, and public-summons rulings instead of overloading general judgment search.
- If a user asks for legal support or trend research, collect several metadata hits first, then fetch only the most relevant full texts.

## Commands

```bash
twlaw judgment search --keyword "預售屋 遲延交屋" --case-type "民事" --max-results 10 --json
twlaw judgment search --case-word "台上" --case-number "3753" --year-from 114 --court "最高法院" --json
twlaw judgment get --jid "TPSV,114,台上,3753,20251112,1" --json
twlaw judgment get --url "https://judgment.judicial.gov.tw/FJUD/data.aspx?ty=JD&id=..." --json
```

Special judgment sources:

```bash
twlaw judgment special --kind simple --keyword "小額" --max-results 10 --json
twlaw judgment special --kind public-summons --keyword "本票" --max-results 10 --json
```

## Exit Codes

- `0`: success
- `2`: invalid input
- `3`: not found
- `4`: upstream WAF or block page
- `5`: upstream page structure changed or parser failed
- `6`: network error

On non-zero exits, stdout is still JSON with `success: false` and an `error` object.

## Limits

Judicial Yuan pages may be protected by F5 WAF. The CLI detects block pages and returns `upstream_blocked`; it does not hide browser automation or MCP fallback behind the command.
