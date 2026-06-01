# Agent Instructions

Use `twlaw` as a CLI-only Taiwan legal research tool. This works for Codex, Claude Code, and any terminal agent. It is not an MCP server.

## Start

```bash
twlaw --version || scripts/install.sh
twlaw sources status --json
twlaw agent guide --json
```

## Rules

- Always call `twlaw ... --json`.
- stdout is JSON on success and error.
- Preserve `source_url`, `retrieved_at`, cache, pagination, and truncation fields in citations.
- Do not ask for government API credentials; default workflows are no-token.
- Do not provide legal advice.
- Prefer cached/bundled commands for high-volume or parallel work.
- Keep live government HTTP queries low-concurrency.

## Publish

```bash
scripts/release-check.sh
scripts/set-publish-urls.sh https://github.com/OWNER/REPO
scripts/release-check.sh --marketplace
```
