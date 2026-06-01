---
name: twlaw-moj-references
description: Use when the user asks to search, verify, cite, or research Taiwan Ministry of Justice legal-reference materials such as administrative interpretations, legal consultation opinions, legal issue seminars, objection decisions, treaties, international agreements, or cross-strait agreements through the `twlaw` CLI. This skill is CLI-only and must not use MCP.
---

# twlaw moj references

Use the local Rust CLI `twlaw` as the agent interface. Do not start or configure an MCP server.

## Contract

- Use `twlaw ... --json` for agent calls.
- If availability is uncertain, run `twlaw --version`; if missing, stop and report that `twlaw` is not installed. Do not install from this skill.
- Preserve `source_url`, `retrieved_at`, category metadata, pagination/truncation fields, cache fields, and coverage limits when citing results.
- Treat output as legal research material, not legal advice.
- Prefer no-credential public sources. Keep live government HTTP query concurrency low.

## Strategy

- Use `mojlaw search` for administrative interpretations, legal consultation opinions, legal issue seminars, and objection decisions. This is distinct from current law/order text.
- Use `moj agreements` for treaties and cross-strait agreements.
- Keep returned `first_page_only`, `truncated`, `total_pages`, and category metadata in mind before claiming exhaustive coverage.
- Start with category discovery or narrow keyword searches before broad fetches.

## Commands

MOJ legal-reference retrieval:

```bash
twlaw mojlaw search --kind admin-interpretation --keyword "個資" --limit 10 --json
twlaw mojlaw search --kind legal-consultation --keyword "個資" --json
twlaw mojlaw search --kind legal-seminar --keyword "個資" --json
twlaw mojlaw search --kind objection-decision --keyword "訴願" --json
```

Treaties and cross-strait agreements:

```bash
twlaw moj agreements --kind treaty --include-categories --json
twlaw moj agreements --kind treaty --keyword "CEDAW" --json
twlaw moj agreements --kind treaty --category-code "D1900500000000" --limit 20 --json
twlaw moj agreements --kind cross-strait --keyword "司法互助" --json
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

MOJ reference and agreement commands expose public retrieval-system coverage. Some listing commands may return partial pages or truncated result sets; surface those fields rather than presenting the output as complete.
