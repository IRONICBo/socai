---
layout: ../../layouts/BlogPost.astro
lang: en
title: "How to get X (Twitter) data for your AI agent for free (2026/10)"
description: "Let your AI agent search X, read posts and check profiles without buying X API credits. A quick look at socai, Twikit and twscrape, with CLI examples."
date: 2026-10-02
dateLabel: October 2, 2026
readingTime: 3 min read
alternates:
  - hreflang: zh
    href: /blog/zh/free-x-data-for-ai-agents
faq:
  - q: "Can an AI agent read X without an API key?"
    a: "Yes. socai uses your signed-in Chrome session to search X and read accessible posts and profiles through CLI commands. You don't need an X developer API key."
  - q: "What does free mean here?"
    a: "The local socai CLI is open source and these browser operations don't incur X API fees. Your agent or model provider may still charge for its own usage."
---

You ask your agent to find what people are saying about a product on X. It comes back with a few search snippets, or asks for an API key. You open X yourself, find the posts, and paste them into the chat. So much for delegating the research.

If you just want to search posts, read a conversation or check a profile, there are ways to do that without buying X API credits.

## What are the options?

The [official X API](https://docs.x.com/x-api/getting-started/pricing) currently uses prepaid credits and charges by usage. It makes sense when your application needs an official integration. For a small research task, you might prefer to start with the account you already use.

Two open-source alternatives are worth knowing:

- [Twikit](https://github.com/d60/twikit) is a Python library that can search tweets without a developer API key, using an account login and saved cookies.
- [twscrape](https://github.com/vladkens/twscrape) provides a Python API and CLI for X search and other internal API operations, with account-session management.

Both are options if you're comfortable building around Python and handling account sessions. A general browser agent can also navigate X, though you may spend a while watching it locate buttons and scroll.

## Give the agent a few browser commands

[socai](https://github.com/socai-io/socai) provides browser commands for social research. It connects to your local Chrome, uses the X account signed in there, and gives your agent commands for search, posts and profiles. Results come back as JSON with source links.

The local CLI is free and open source. These operations don't use the paid X API. Your agent's model usage is still billed separately, if you use a paid model.

## Try a small search

[Install socai](/docs/installation/), [connect Chrome](/connect), and log in to X in Chrome. Then run:

```bash
socai task begin "Find discussions about customer research on X."
socai x search "customer research" --num 10 --pretty
```

Pick a relevant URL from the results and replace the placeholder below:

```bash
socai x get-posts --post '<post-url-from-search>' --num-comments 8
```

To let Codex discover these tools, install the integration once:

```bash
socai integrate install codex
```

Then ask something specific:

> Use socai to find recent X posts where marketers ask for help with customer research. Open the relevant posts, exclude tool promotions, and give me the original links with a short summary of each problem.

X can still restrict access, and replies may be incomplete. Start with a small sample and check the original posts. Having real text and links gives your agent something useful to work with—and gives you a way to check its conclusions.
