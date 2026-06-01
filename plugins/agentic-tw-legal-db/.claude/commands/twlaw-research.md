---
allowed-tools: Bash(twlaw:*)
argument-hint: [research question]
description: Run Taiwan legal research through twlaw with JSON output and official citations.
---

Research request: $ARGUMENTS

Use `twlaw` only through JSON commands. Start with:

```bash
twlaw sources status --json
twlaw agent guide --json
```

Use `twlaw agent guide --json` to choose focused commands. Return a concise summary with official `source_url` citations, retrieval timestamps, and relevant coverage limits. Do not provide legal advice.
