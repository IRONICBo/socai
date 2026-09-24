---
title: Evidence and artifacts
description: Understand source links, run records, media files, and the boundary between evidence and inference.
---

socai keeps enough context to review how a result was produced. A high-quality report should link important findings to captured posts, comments, profiles, or media.

## Evidence layers

1. **Source** — the original social URL and platform identity.
2. **Captured record** — structured post, comment, author, or media data returned by a tool.
3. **Agent interpretation** — themes, comparisons, and conclusions derived from those records.

Do not treat the interpretation as a replacement for the source. Re-open critical links before making a consequential decision.

## Run storage

When a durable runs directory is configured, execution data uses three ownership layers:

```text
~/.socai/sessions/<session-id>/conversation.json
<run-dir>/run.json
<run-dir>/llm/
<run-dir>/tools/<tool-call>/
```

Standalone structured CLI commands use `<run-dir>/tool.json` as the command record.

## Media enrichment

Some platform operations can download images or videos, run OCR, or transcribe speech. Generated files are recorded as artifacts alongside the tool result. Availability depends on the source page, login state, and selected command options.

## Export

The desktop app can preview or download report and media artifacts. Supported builds can also export results to a Feishu document or send them to a group chat after explicit user selection and authorization. This export is a user-initiated side effect; the social-platform research integrations remain read-only.
