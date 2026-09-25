---
title: Connect Chrome
description: Choose a Chrome profile mode and give socai access to the signed-in social web.
sidebar:
  order: 4
---

socai works through Chrome DevTools Protocol instead of a private platform API. It needs a running Chrome instance with remote debugging enabled and a valid login for each platform you want to research.

## Profile modes

| Mode | Best for | Login behavior |
| --- | --- | --- |
| `existing` | Everyday use; the default | Reuses your current Chrome session and logins |
| `managed` | Isolating research from everyday browsing | Uses `~/.socai/chrome-profile`; sign in once |
| `auto` | Let socai choose | Tries managed first, then existing Chrome |
| `remote` | Hosted browser testing | Beta socai pro capability with session limits |

Set a mode with:

```bash
socai config set chrome.profile managed
socai stop
```

The next run starts or connects using the selected profile.

## First connection

1. Follow the in-app prompt or the [visual connection guide](/connect/).
2. Enable remote debugging in Chrome if requested.
3. Confirm Chrome's connection permission prompt.
4. Sign in to each social platform in the selected profile.
5. Run a small search and verify that results open successfully.

## macOS permissions

When an app launched from a terminal or editor attaches to your existing Chrome, macOS may require access under **System Settings → Privacy & Security → Files & Folders**. Grant access to the process that actually launched socai. A development build and the installed desktop app can have different permission records.

## Security boundary

socai drives only the browser profile you select. Platform cookies stay in Chrome; do not export or commit Chrome profile data. Use the managed profile when you want a separate account or a clean research boundary.
