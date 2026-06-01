# Claude Code Instructions

Use Bash to run `twlaw ... --json`. Follow `AGENTS.md` for the shared CLI contract.

First checks:

```bash
twlaw --version || scripts/install.sh
twlaw sources status --json
twlaw agent guide --json
```

Do not configure MCP unless the user explicitly asks for a new wrapper. Preserve official source URLs and retrieval timestamps. Do not ask for external government API tokens. Do not provide legal advice.

Project commands:

- `/twlaw-status`
- `/twlaw-research`
- `/twlaw-publish-check`
