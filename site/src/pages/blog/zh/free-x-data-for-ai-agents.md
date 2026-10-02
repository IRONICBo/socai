---
layout: ../../../layouts/BlogPost.astro
lang: zh
title: "如何免费让 AI agent 获取 X（Twitter）数据 (2026/10)"
description: "不用购买 X API，让 AI agent 搜帖子、读正文、看用户主页。简单聊聊 socai、Twikit 和 twscrape，再用几条 CLI 命令跑一次搜索。"
date: 2026-10-02
dateLabel: 2026 年 10 月 2 日
readingTime: 3 分钟阅读
alternates:
  - hreflang: en
    href: /blog/free-x-data-for-ai-agents
faq:
  - q: "没有 X API Key，agent 也能读取帖子吗？"
    a: "可以。socai 通过已登录的 Chrome 搜索 X、读取账号能访问的帖子和主页，并提供 CLI 命令供 agent 调用，无需 X 开发者 API Key。"
  - q: "这里的免费具体指什么？"
    a: "本地 socai CLI 免费开源，这些浏览器操作不产生 X API 费用。如果你使用付费 agent 或模型服务，它们的费用仍然单独计算。"
---

让 agent 去 X 上看看大家怎么评价一个产品，它可能给你几条网页摘要，也可能开始问你要 API Key。最后，你还是自己打开 X，找到帖子，再复制进聊天框。说好的让它帮忙呢。

如果只是想搜一些帖子、读讨论、看看用户主页，其实可以不用先买 X API。

## 现在有哪些办法？

[X 官方 API](https://docs.x.com/x-api/getting-started/pricing) 目前采用预付额度、按使用量计费的方式。应用需要正式的 API 集成时，这条路很直接。只是做一次小调研的话，也可以先用你平时登录的账号。

开源工具里，有两个值得了解：

- [Twikit](https://github.com/d60/twikit)：Python 库，用账号登录和保存的 cookies 搜索推文，不需要开发者 API Key。
- [twscrape](https://github.com/vladkens/twscrape)：提供 Python API 和 CLI，支持 X 搜索等内部接口操作，也有账号会话管理。

如果熟悉 Python，也愿意自己处理账号会话，可以看看它们。通用浏览器 agent 也能操作 X，只是找按钮、滚动、展开内容这些步骤，有时看着挺着急。

## 给 agent 几个现成的浏览器操作

[socai](https://github.com/socai-io/socai) 提供了一组用于社交媒体研究的浏览器操作。它连接本地 Chrome，使用里面已登录的 X 账号，让 agent 通过命令搜索、打开帖子、读取主页。结果以 JSON 返回，带着原始链接。

本地 CLI 免费开源，这些操作不调用付费 X API。你用的 agent 或模型如果收费，那部分仍然单独算。

## 先搜十条试试

[安装 socai](/docs/installation/)，[连接 Chrome](/connect)，在 Chrome 里登录 X，然后运行：

```bash
socai task begin "在 X 上找关于客户研究的讨论。"
socai x search "customer research" --num 10 --pretty
```

从结果里挑一条相关帖子，把下面的占位符换成它的 URL：

```bash
socai x get-posts --post '<post-url-from-search>' --num-comments 8
```

如果想让 Codex 自己发现这些工具，装一次集成：

```bash
socai integrate install codex
```

接着直接告诉它要找什么：

> 用 socai 在 X 找最近营销人员请教如何做客户研究的帖子。打开相关正文，排除工具推广，给我原帖链接和每个人遇到的问题。

X 仍然可能限制访问，回复也不一定能读全。先跑一小批，再点回原帖核对。有了真实正文和链接，agent 才有材料可分析，你也能看出它的结论有没有依据。
