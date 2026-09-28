---
title: Agent workflows
description: Turn a market, creator, customer, or trend question into a bounded evidence-backed research task.
---

Run `socai` without a subcommand when you want the agent to plan a multi-step investigation across platforms.

## Install Agent discovery

socai ships a portable social-research Skill whose description is visible to the agent before its full instructions are loaded. Install it once for the agent you use:

```bash
socai integrate install codex
socai integrate install claude-code
socai integrate install agents
```

`socai integrate install all` installs all three user-scoped copies. Add `--scope project` to write under the current repository instead, `--dry-run` to inspect destinations, or use `socai integrate status --json` to check whether every copy matches this CLI release.

After installation, requests about social listening, creator or company research, post and comment evidence, trend validation, and cross-platform comparison can select socai automatically. Discovery never grants permission to like, follow, comment, reply, or publish; those actions still require an explicit target and content from the user.

## CLI task workflow

For every new task using the structured CLI, first call `socai task begin` with
the user's original question. This is the common workflow for all platform
operations. Subsequent commands need no extra task parameters:

```bash
socai task begin "Find why consumers repurchase sugar-free tea."
socai xhs search "sugar-free tea repeat purchase" --num-notes 10
socai dy search "sugar-free tea reviews" --num 20
```

The daemon associates commands with its current task until the next `task begin`.
Register again after a daemon restart. Agents sharing a daemon share its latest
task boundary. Searches, profile reads, and retries within one task reuse that
boundary automatically.

For multiline or shell-sensitive input, write a UTF-8 JSON file containing
`{"user_prompt":"The user's original question"}` and use
`socai task begin --context-file context.json`. An optional `agent_host` identifies
the caller; `--context-file -` reads stdin. Preserve the original wording rather
than a summary. The [general CLI skill](https://github.com/socai-io/socai/blob/main/skills/socai-cli/SKILL.md)
provides the full external-agent workflow.

## Write a useful brief

A strong task names four things:

1. **Question** — what decision or uncertainty should the research address?
2. **Scope** — which platforms, markets, languages, and time window matter?
3. **Evidence depth** — posts only, or comments, replies, profiles, OCR, and transcription too?
4. **Output** — summary, comparison, spreadsheet, creator list, or source-linked report?

```text
Research how first-time campers describe regretted gear purchases on RedNote,
TikTok, and Instagram. Capture at least 20 relevant posts and read high-signal
comment threads. Group recurring mistakes, quote audience language briefly,
and link every finding to the original post.
```

## Common workflows

### Market research

Map recurring needs, objections, terminology, and purchase criteria across live posts and comments.

### Trend intelligence

Trace how a claim or format appears across platforms, who amplifies it, and how audience reaction changes.

### People discovery

Find creators, experts, candidates, or communities using relevant work and conversation evidence rather than follower count alone.

### Content analysis

Compare hooks, formats, comment language, and audience response, while preserving links to the original examples.

## Keep the task bounded

Specify target counts when coverage matters. Ask the agent to separate observed evidence from inference, and to report gaps when a platform blocks access or the sample is weak.

For deterministic operations inside another agent, use the [structured CLI](/docs/cli/).
