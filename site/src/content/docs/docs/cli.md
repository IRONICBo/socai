---
title: CLI reference
description: Use socai's structured platform commands from scripts and coding agents.
---

The CLI writes the final machine-readable command result to standard output. Interactive progress is rendered separately, so scripts and agents can parse the result without stripping progress lines.

## Search commands

```bash
socai xhs search "content marketing ideas" --num-notes 30 --num-comments 20 --pretty
socai dy search "coffee" --num 30
socai tiktok search "coffee" --num 30 --pretty
socai instagram search "coffee" --num 20 --pretty
socai linkedin search "product designer" --type people --num 20 --pretty
```

## Platform entry points

| Command | Typical operations |
| --- | --- |
| `socai xhs` | Search, authors, selected notes, comments, media, OCR, transcription |
| `socai dy` | Search, videos, authors, comments, media |
| `socai tiktok` | Search, videos, profiles, comments, media |
| `socai instagram` | Search, profiles, posts, Reels, comments, media |
| `socai linkedin` | People, company and content search; profiles, history, posts, comments |

The command surface evolves with platform changes. Treat built-in help as authoritative:

```bash
socai xhs --help
socai instagram --help
socai linkedin --help
```

## Xiaohongshu example

```bash
socai xhs search "Shanghai weekend activities" \
  --num-notes 20 \
  --num-comments 12 \
  --filter publish_time=一周内 \
  --filter note_type=图文 \
  --filter sort=最新 \
  --download-media \
  --ocr \
  --pretty
```

Use `--preview` when result-card metadata is enough and you do not want to open every post. Use `--debug-snapshot` only for development diagnostics because it writes page snapshots and screenshots.

## Preserve runs

Set a durable run directory when another process needs to inspect exact tool evidence:

```bash
socai config set runs.dir /path/to/socai-runs
```

See [Evidence and artifacts](/docs/evidence/) for the stored layout.
