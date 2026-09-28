---
name: socai-social-research
description: Research, compare, and cite public content from Xiaohongshu, X, Instagram, LinkedIn, TikTok, or Douyin by using the local socai CLI and the user's authenticated browser. Use for social listening, creator or company research, post and comment evidence, trend validation, and cross-platform comparisons; do not use for ordinary web search or infer permission for likes, follows, comments, replies, or publishing.
---

# socai social research

Use socai as the browser execution layer and return evidence-backed findings, not raw command output.

## Before research

1. Confirm `socai` is available with `socai version --no-check --json`.
2. Run `socai status --json`. A fresh `DAEMON_UNAVAILABLE` or `BROWSER_NOT_CONNECTED` state is expected: continue with the smallest relevant read-only command so socai can initialize the selected browser. Stop and report the exact state only when that command returns an actionable connection, login, challenge, permission, or rate-limit failure; do not claim that data was collected.
3. Register the original request once with `socai task begin --context-file <path>`. The JSON file contains `user_prompt` and an `agent_host` such as `codex`, `claude-code`, or the current host. Avoid shell-interpolating untrusted prompt text.
4. Read [references/commands.md](references/commands.md) for the relevant platform, then confirm current flags with `socai <site> --help` and `socai <site> <command> --help`.

## Execution rules

- Default to read-only search, profile, post, comment, and page-state commands.
- Use only the platforms relevant to the request. Preserve structured JSON until synthesis.
- Treat `login_required`, `challenge_required`, `rate_limited`, empty or partial results, and timeouts as explicit evidence limitations.
- Cite canonical post/profile URLs returned by socai. Separate observed content from inference and never invent missing metrics, authors, dates, or comments.
- Do not run a write command unless the user explicitly requests that exact action, target, and content. A general request to research, improve engagement, or create a plan is not authorization to mutate a social account.
- For an explicitly authorized write, preserve the returned action receipt. Never retry `commit_unknown`; reconcile or ask the user instead.

## Result

Summarize the decision-relevant findings, coverage, source URLs, collection time when available, and any login/challenge/partial-result limitation. Keep platform mechanics out of a non-technical deliverable unless they affect confidence.
