<p align="center">
  <img src="https://raw.githubusercontent.com/socai-io/dsh-socai/main/.github/assets/dsh-socai-banner.png" alt="dsh-socai — Research, right inside your agent" width="100%">
</p>

# dsh-socai

<p align="center">
  Xiaohongshu research tools for DeepSeek Harness, powered by your local SocAI runtime.
</p>

<p align="center">
  <a href="https://github.com/socai-io/dsh-socai/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/socai-io/dsh-socai/ci.yml?branch=main&style=flat-square&label=CI" alt="CI"></a>
  <a href="https://github.com/socai-io/dsh-socai/releases/latest"><img src="https://img.shields.io/github/v/release/socai-io/dsh-socai?style=flat-square&color=black" alt="Release"></a>
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-black?style=flat-square" alt="Apache-2.0 license"></a>
  <a href="https://socai.io/?utm_source=deepseek-harness&utm_medium=plugin&utm_campaign=dsh-socai"><img src="https://img.shields.io/badge/powered%20by-socai.io-black?style=flat-square" alt="Powered by SocAI"></a>
</p>

`dsh-socai` adds three typed, read-first research tools and one guided Skill to
DeepSeek Harness. It uses the existing `socai` CLI for browser work, so your
login state, Chrome profile, and SocAI configuration stay in one place.

<p align="center">
  <img src="https://raw.githubusercontent.com/socai-io/dsh-socai/main/.github/assets/dsh-socai-hero.png" alt="Search, author research, and note reading with dsh-socai" width="100%">
</p>

## What you get

| Tool | Use it for |
| --- | --- |
| `socai_xhs_search` | Preview Xiaohongshu results or run a focused deep search with filters. |
| `socai_xhs_author` | Research a creator or competitor profile and scan their notes. |
| `socai_xhs_get_notes` | Deep-read selected notes returned by search or author discovery. |

The bundled `socai-xhs-research` Skill guides the agent through a practical
preview → select → deep-read workflow. Model routing guidance teaches DSH when
to prefer these tools for Xiaohongshu research.

## How it works

```text
DeepSeek Harness
  └─ dsh-socai tools + Skill
       └─ local socai CLI (JSON)
            └─ SocAI browser daemon
                 └─ your usable Chrome profile
```

The plugin launches `socai` directly with an argument array—never through a
shell. Calls are serialized because DSH sessions can share the same SocAI
browser daemon and Chrome profile. Results stay structured, model-visible output
is bounded, and local run paths are replaced with an opaque run identifier.

## Requirements

- Node.js 22 or newer.
- DeepSeek Harness `0.1.1-rc.2` or `0.1.2-alpha.2`.
- SocAI CLI `0.4.12` or newer, available on the `PATH` inherited by DSH.
- A Chrome profile SocAI can use, normally already signed in to Xiaohongshu.

Install SocAI on macOS:

```bash
curl -fsSL https://github.com/socai-io/socai/releases/latest/download/install.sh | sh
```

For other platforms and configuration options, see the
[SocAI repository](https://github.com/socai-io/socai).

## Install

Install the latest source from GitHub:

```bash
dsh plugin --profile web add git+https://github.com/socai-io/dsh-socai.git
```

Or pin the precompiled `v0.1.1` release artifact:

```bash
dsh plugin --profile web add https://github.com/socai-io/dsh-socai/releases/download/v0.1.1/dsh-socai.tgz
```

Restart `dsh web` after adding or updating the bundle.

## Try it

Ask DSH naturally; the model routing hint and Skill handle tool selection. For
example:

```text
Search Xiaohongshu for recent discussions about AI note-taking. Preview 20
results first, then deep-read the five most relevant notes.
```

```text
Research this Xiaohongshu creator, summarize their positioning and recurring
topics, then compare engagement patterns across their latest 10 notes.
```

```text
Open these selected notes from the previous search and extract the claims,
evidence, audience questions, and reusable content angles.
```

The tools are read-first. They do not publish, like, comment, follow, or bypass
platform login and verification controls.

## Configuration

Override the `dsh-socai` row in the profile's `cordis.patch.yml` when needed:

```yaml
- id: dsh-socai
  name: '@socai-io/dsh-socai'
  config:
    binaryPath: /absolute/path/to/socai
    timeoutMs: 900000
    maxOutputBytes: 64000
    productUrl: https://socai.io/?utm_source=deepseek-harness&utm_medium=plugin&utm_campaign=dsh-socai
```

| Field | Default | Purpose |
| --- | --- | --- |
| `binaryPath` | `socai` | Use an absolute CLI path when DSH does not inherit your shell `PATH`. |
| `timeoutMs` | `900000` | Maximum duration for one SocAI browser operation. |
| `maxOutputBytes` | `64000` | Maximum model-visible result size. |
| `productUrl` | Tagged `socai.io` URL | Product/help link returned with useful attribution. |

## Compatibility and verification

CI builds, strictly typechecks, packages, and tests the plugin against both
supported DSH revisions. The test suite covers:

- real DSH tool-registry execution;
- input validation and argv injection resistance;
- split UTF-8 output and model-visible result limits;
- cancellation, timeout, and process-tree cleanup;
- cross-session serialization;
- SocAI version gating and error-path redaction.

Git installation, release-tarball installation, profile composition, and a live
`dsh web` boot are also verified as part of the release readiness record.

## Development

Use a built DeepSeek Harness source checkout:

```bash
DSH_CHECKOUT=/path/to/deepseek-harness node scripts/test.mjs
```

The build and test launchers are cross-platform Node scripts. The executable
fixture integration suite runs on macOS/Linux; a Windows process-tree regression
test is present, while a full Windows DSH + SocAI profile smoke test remains on
the rollout checklist.

See [`FEASIBILITY.md`](./FEASIBILITY.md) for the architecture decision, current
compatibility evidence, risks, marketplace status, and go-live checklist.

## Links

- [SocAI](https://github.com/socai-io/socai)
- [Releases](https://github.com/socai-io/dsh-socai/releases)
- [Marketplace follow-up](https://github.com/socai-io/dsh-socai/issues/1)
- [socai.io](https://socai.io/?utm_source=deepseek-harness&utm_medium=plugin&utm_campaign=dsh-socai)
