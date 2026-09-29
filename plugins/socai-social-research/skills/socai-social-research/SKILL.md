---
name: socai-social-research
description: Research, compare, and cite public content from Xiaohongshu (XHS/RedNote), X, Instagram, LinkedIn, TikTok, or Douyin by using the local socai CLI and the user's authenticated Chrome session. Use for social listening, consumer insights, competitor research, creator or company research, post and comment evidence, trend validation, and cross-platform comparisons; do not use for ordinary web search, content creation, or publishing.
---

# socai social research

Use socai as the browser execution layer and return evidence-backed findings, not raw command output.

## Before research

1. Confirm `socai` is available with `socai version --no-check --json`. This workflow requires a local shell, an installed Socai CLI, and a supported Chrome session. If the command is missing, do not download or run an installer without explicit user approval. Explain the prerequisite and link to `https://socai.io/docs/installation/`. If the current host cannot access a local shell or browser, report that this workflow is unavailable on that host instead of claiming that research ran.
2. Run `socai status --json`. A fresh `DAEMON_UNAVAILABLE` or `BROWSER_NOT_CONNECTED` state is expected: continue with the smallest relevant read-only command so socai can initialize the selected browser. Stop and report the exact state only when that command returns an actionable connection, login, challenge, permission, or rate-limit failure; do not claim that data was collected.
3. Register the original request once with `socai task begin --context-file <path>`. The JSON file contains `user_prompt` and an `agent_host` such as `codex`, `claude-code`, or the current host. Avoid shell-interpolating untrusted prompt text.
4. Read [references/commands.md](references/commands.md) for the relevant platform, then confirm current flags with `socai <site> --help` and `socai <site> <command> --help`.

## Execution rules

- Default to read-only search, profile, post, comment, and page-state commands.
- Use only the platforms relevant to the request. Preserve structured JSON until synthesis.
- Treat `login_required`, `challenge_required`, `rate_limited`, empty or partial results, and timeouts as explicit evidence limitations.
- Cite canonical post/profile URLs returned by socai. Separate observed content from inference and never invent missing metrics, authors, dates, or comments.
- This plugin is strictly read-only. Never run commands that publish, like, follow, react, comment, reply, message, connect, upload, delete, or otherwise mutate a social account, even when the user asks. Explain that the installed research plugin does not provide account mutations.

## Result

Summarize the decision-relevant findings, coverage, source URLs, collection time when available, and any login/challenge/partial-result limitation. Keep platform mechanics out of a non-technical deliverable unless they affect confidence.
