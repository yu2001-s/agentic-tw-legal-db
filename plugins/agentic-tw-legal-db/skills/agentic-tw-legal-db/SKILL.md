---
name: agentic-tw-legal-db
description: Use when the user asks to query, verify, cite, or research Taiwan legal materials through the `twlaw` CLI, including Judicial Yuan judgments, Ministry of Justice regulations, and Constitutional Court interpretations or rulings. This skill is CLI-only and must not use MCP.
---

# agentic tw legal db

Use the local Rust CLI `twlaw` as the agent interface. Do not start or configure an MCP server for this plugin.

## First Check

Run:

```bash
twlaw --version
twlaw sources status --json
```

If the command is missing, run the plugin installer:

```bash
scripts/install.sh
```

All agent calls should include `--json`. The CLI always emits JSON, but the flag makes the contract explicit in transcripts and scripts.

## Query Strategy

- Start every broad research task with `twlaw sources status --json` or `twlaw agent guide --json` so coverage limits and no-credential source policy are explicit.
- Start with narrow metadata queries before fetching long text.
- For MOJ bulk law/order work, prefer `moj sync` once, then `moj search`/`moj get`; this uses the official no-token OpenAPI ZIP cache instead of repeated live page fetches.
- For recency-sensitive law/order questions, call `moj updates` before relying on cached data.
- For treaties and cross-strait agreements, use `moj agreements`; keep the returned `first_page_only`, `truncated`, `total_pages`, and category metadata in mind before claiming exhaustive coverage.
- For MOJ administrative interpretations, legal consultation opinions, legal issue seminars, or objection decisions, use `mojlaw search`; this is distinct from current law/order text.
- For judgments, use `judgment search` first and only call `judgment get` when a specific `jid` or source URL is needed.
- For simple cases, declaration judgments, or public-summons rulings, use `judgment special` instead of overloading general judgment search.
- For a known judgment case number, use `--case-word` and `--case-number`; do not put a case number into `--keyword`.
- For Constitutional Court materials, call `interpretation current` first when recency matters, use `interpretation terminal` for procedure rulings/non-acceptance decisions/terminal-case coverage, then use bundled `search`/`get` for details.
- For Constitutional Court materials, prefer keyword snippets such as `--reasoning-keyword` before `--include-reasoning`.
- For discovering more official legal datasets, use `open-data legal-catalog`; treat results as catalog metadata and inspect returned licenses/source URLs before reusing content.
- Treat output as research material, not legal advice. Preserve `source_url`, `retrieved_at`, and cache fields in user-facing citations when relevant.
- Do not require users to apply for external government API access. Token-required APIs may be mentioned as optional coverage gaps, but default workflows must use no-credential public sources or bundled snapshots.
- For repeated or parallel work, prefer bundled/offline queries or a local sync/index path. Keep live government HTTP query concurrency low.

## Commands

Agent/source discovery:

```bash
twlaw agent guide --json
twlaw sources status --json
twlaw sources list --no-credentials --json
twlaw sources gaps --json
```

Regulations:

```bash
twlaw regulation pcode --law "民法" --json
twlaw regulation search --keyword "勞動" --limit 20 --json
twlaw regulation query --law "民法" --article "184" --json
twlaw regulation query --law "民法" --from "184" --to "198" --json
twlaw regulation query --pcode "B0000001" --include-history --json
```

MOJ OpenAPI bulk datasets:

```bash
twlaw moj datasets --json
twlaw moj status --dataset all --json
twlaw moj sync --dataset ch-law --json
twlaw moj search --dataset ch-order --keyword "勞動" --include-articles --limit 20 --json
twlaw moj get --dataset ch-law --law "民法" --article "184" --json
twlaw moj search --dataset en-law --keyword "Personal Data" --limit 10 --json
twlaw moj updates --kind order --limit 10 --json
twlaw moj updates --kind draft --keyword "個資" --json
twlaw moj agreements --kind treaty --include-categories --json
twlaw moj agreements --kind treaty --keyword "CEDAW" --json
twlaw moj agreements --kind treaty --category-code "D1900500000000" --limit 20 --json
twlaw moj agreements --kind cross-strait --keyword "司法互助" --json
twlaw mojlaw search --kind admin-interpretation --keyword "個資" --limit 10 --json
twlaw mojlaw search --kind legal-consultation --keyword "個資" --json
twlaw mojlaw search --kind legal-seminar --keyword "個資" --json
```

Government open-data catalog:

```bash
twlaw open-data legal-catalog --limit 30 --json
twlaw open-data legal-catalog --keyword "判決" --limit 10 --json
twlaw open-data legal-catalog --agency "司法院" --json
```

Judgments:

```bash
twlaw judgment search --keyword "預售屋 遲延交屋" --case-type "民事" --max-results 10 --json
twlaw judgment special --kind simple --keyword "小額" --max-results 10 --json
twlaw judgment special --kind public-summons --keyword "本票" --max-results 10 --json
twlaw judgment search --case-word "台上" --case-number "3753" --year-from 114 --court "最高法院" --json
twlaw judgment get --jid "TPSV,114,台上,3753,20251112,1" --json
twlaw judgment get --url "https://judgment.judicial.gov.tw/FJUD/data.aspx?ty=JD&id=..." --json
```

Constitutional Court:

```bash
twlaw interpretation get "釋字748" --json
twlaw interpretation get "釋字748" --reasoning-keyword "婚姻" --json
twlaw interpretation get "111年憲判字第1號" --include-reasoning --json
twlaw interpretation current --limit 10 --json
twlaw interpretation terminal --kind procedure-ruling --limit 10 --json
twlaw interpretation terminal --kind non-acceptance --keyword "婚姻" --json
twlaw interpretation search --keyword "集會自由" --limit 10 --json
twlaw interpretation search --include-old=false --limit 10 --json
twlaw interpretation search --no-old --limit 10 --json
twlaw interpretation citations "釋字748" --include-context --json
```

## Exit Codes

- `0`: success
- `2`: invalid input
- `3`: not found
- `4`: upstream WAF or block page
- `5`: upstream page structure changed or parser failed
- `6`: network error
- `7`: bundled data error

On non-zero exits, stdout is still JSON:

```json
{
  "success": false,
  "error": {
    "code": "not_found",
    "message": "..."
  }
}
```

## Important Limits

Judicial Yuan pages may be protected by F5 WAF. This CLI detects block pages and returns `upstream_blocked`; it does not hide browser automation or MCP fallback behind the command. If the user needs browser-cookie refresh later, implement it as an explicit optional CLI feature.

Bundled Constitutional Court data is offline and fast. Regulations use bundled pcode metadata but fetch article text from `law.moj.gov.tw`. MOJ OpenAPI commands cache extracted official ZIP JSON under the local twlaw cache directory and can auto-sync a missing dataset. The open-data catalog command caches the official data.gov.tw CSV export locally. Judgment search and judgment text fetch from `judgment.judicial.gov.tw`. Live government HTTP calls use bounded retries/backoff, but high-volume agent workflows should still use local snapshot/sync data where possible.
