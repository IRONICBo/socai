---
title: socai documentation
description: Install socai, connect your signed-in Chrome, and run source-linked social media research.
sidebar:
  label: Overview
  order: 1
---

socai is a local agent that researches the social web through the Chrome session you already use. It opens real pages, reads posts and comment threads, and returns evidence with links back to the original sources.

It is designed for Xiaohongshu (RedNote), Douyin, TikTok, Instagram, and LinkedIn. The desktop app, terminal interface, and structured CLI share the same browser connection and research runtime.

## Start here

1. [Install socai](/docs/installation/).
2. [Connect Chrome](/docs/browser-connection/).
3. Follow the [five-minute quickstart](/docs/quickstart/).
4. Choose an [agent workflow](/docs/agent-workflows/) or a structured [CLI command](/docs/cli/).

## What socai returns

- Search results and opened posts from the live social platform.
- Comments, replies, author or company context, and media metadata when requested.
- Original post URLs and durable local run artifacts.
- A research answer that separates captured evidence from interpretation.

## Choose an interface

| Interface | Best for | Start with |
| --- | --- | --- |
| Desktop app | Natural-language tasks, run history, and artifact preview | Install the macOS or Windows app |
| Terminal interface | Consecutive research tasks in a terminal | Run `socai` |
| Structured CLI | Scripts, agent tools, and JSON pipelines | Run `socai <platform> --help` |

All supported social-platform research integrations are read-only. socai does not publish, follow, like, react, comment, or reply on those platforms. A Feishu export can create a document or send a message only after you explicitly choose and authorize that destination.

## Project links

- [Source code](https://github.com/socai-io/socai)
- [Latest release](https://github.com/socai-io/socai/releases/latest)
- [Discord community](https://discord.gg/CpQdA7bwt8)
- [Connect Chrome guide](/connect/)
