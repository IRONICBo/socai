---
title: Troubleshooting
description: Diagnose Chrome connection, login, empty result, and media-processing problems.
---

## Chrome does not connect

1. Run `socai config get` to inspect saved settings. If `chrome.profile` is absent, the effective default is `existing`.
2. Reopen the desktop connection flow or run `socai stop` before retrying.
3. Confirm remote debugging is enabled when Chrome requests it.
4. On macOS, check **Privacy & Security → Files & Folders** for the app, terminal, or editor that launched socai.

## A platform shows no results

- Open the platform manually in the same Chrome profile and confirm you are signed in.
- Check whether the query produces results in the website UI.
- Reduce the requested result count and retry.
- Use a preview/search command before requesting comments or media enrichment.

## Results stop partway through

Platforms can limit scrolling, invalidate sessions, or change their page markup. socai preserves valid partial evidence. Review the reported failure and retry only the missing scope instead of discarding completed work.

## OCR or transcription is unavailable

- Ensure the relevant command requested media download or transcription.
- Source media may require an authenticated request.
- Source builds need the local `socai-asr` helper for offline transcription routes.

## Get diagnostic output

Run the platform command with its diagnostic option when instructed by a maintainer:

```bash
socai xhs search "test query" --num-notes 3 --debug-snapshot --pretty
```

Diagnostic snapshots may contain visible page content. Inspect them before sharing and never publish cookies, credentials, or an entire Chrome profile.

If the issue persists, open a [GitHub issue](https://github.com/socai-io/socai/issues) with the socai version, operating system, profile mode, command, and sanitized error output.
