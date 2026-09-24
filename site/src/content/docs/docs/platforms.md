---
title: Supported platforms
description: Understand the current social platforms, research coverage, and login requirements.
---

## Capability matrix

| Platform | Research capabilities | Access |
| --- | --- | --- |
| Xiaohongshu (RedNote) | Search, authors, posts, comments and replies, media download, OCR, transcription | Agent and structured CLI |
| Douyin | Search, video details, authors, comments and replies, media artifacts | Agent and structured CLI |
| TikTok | Search, video details, profiles, comments and replies, video download | Agent and structured CLI |
| Instagram | Keyword search, profiles, posts, Reels, comments and replies, playable video download | Agent and structured CLI |
| LinkedIn | People, company and content search; profiles, experience, relationships, posts, comments | Agent and structured CLI |

## Platform logins

Each platform applies its own authentication, region, rate, and content-visibility rules. socai uses the account already signed in to the selected Chrome profile. A result visible in one account or region may not be available in another.

## Read-only behavior

The supported integrations are intentionally read-only. socai does not follow accounts, send connection requests, publish, like, react, comment, reply, or message.

## Partial results

Live sites change continuously. When an operation has already captured usable records, socai may preserve them with the failure context. Other failures can stop before partial records are returned. Treat partial-result recovery as best effort; socai never invents missing results.
