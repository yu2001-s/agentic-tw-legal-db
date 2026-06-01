# Publishing

```bash
scripts/release-check.sh
scripts/set-publish-urls.sh https://github.com/OWNER/REPO
scripts/release-check.sh --marketplace
```

The URL script fills Codex manifest fields for website, privacy policy, and terms. Codex uses `.codex-plugin/plugin.json` and `skills/`. Claude Code uses `CLAUDE.md` and `.claude/commands/`. Generic agents use `AGENTS.md`.

Known gaps: no MCP wrapper; token-required Judicial Yuan API is optional; local-government law text remains tracked but not first-class.
