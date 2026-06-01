---
name: twlaw-setup-diagnostics
description: Use when the user asks to install, configure, diagnose, repair, validate, publish-check, test, inspect source coverage, list source gaps, check no-credential policy, or troubleshoot the `twlaw` CLI or agentic-tw-legal-db plugin. Do not use this for ordinary legal research questions.
---

# twlaw setup diagnostics

Use this skill for setup and diagnostics only. Ordinary legal research should use a source-specific twlaw skill.

## Setup Checks

Run from the plugin root when possible:

```bash
twlaw --version
twlaw sources status --json
twlaw agent guide --json
```

If `twlaw` is missing and the user asked to install or repair setup, run:

```bash
scripts/install.sh
```

Then re-run:

```bash
twlaw --version
twlaw sources status --json
```

## Source Discovery

```bash
twlaw sources status --json
twlaw sources list --no-credentials --json
twlaw sources gaps --json
twlaw agent guide --json
```

Use these commands when a user asks what the plugin can access, whether credentials are needed, what coverage gaps exist, or why a source is unavailable.

## Project Validation

From the plugin root:

```bash
scripts/doctor.sh
scripts/test.sh
scripts/release-check.sh
```

Use `scripts/release-check.sh --marketplace` only when validating marketplace metadata and publish readiness.

## Exit Codes

- `0`: success
- `2`: invalid input
- `3`: not found
- `4`: upstream WAF or block page
- `5`: upstream page structure changed or parser failed
- `6`: network error
- `7`: bundled data error

On non-zero `twlaw` exits, stdout is still JSON with `success: false` and an `error` object.

## Boundaries

- Do not require users to apply for external government API access.
- Token-required APIs may be mentioned as optional coverage gaps, but default workflows must use no-credential public sources or bundled snapshots.
- Do not use MCP for this plugin unless a future user explicitly asks to build a wrapper.
