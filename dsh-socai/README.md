# dsh-socai

Bring [socai](https://socai.io/?utm_source=deepseek-harness&utm_medium=plugin&utm_campaign=dsh-socai)
into DeepSeek Harness as typed, read-first Xiaohongshu research tools.

This proof of concept follows the same integration boundary as
`THU-MAIC/dsh-openmaic`: an installable DSH bundle registers tools, model
guidance, and a bundled Skill while the existing product remains the execution
backend. Here the backend is the local `socai` CLI rather than a public remote
generation API.

## What it contributes

- `socai_xhs_search`: broad preview or focused deep search.
- `socai_xhs_author`: creator/competitor profile and note scan.
- `socai_xhs_get_notes`: deep-read selected notes returned by discovery.
- `socai-xhs-research`: preview → select → deep-read research workflow.

The plugin launches the executable directly with an argument array; it does not
use a shell. A plugin-wide queue serializes calls across DSH sessions because
they share the SocAI browser daemon/profile; the DSH tools are also marked
non-concurrency-safe. Separate DSH processes converge on SocAI's own serialized
daemon request path. Successful results preserve the CLI JSON, expose only an
opaque run identifier (not the user's absolute path), and include a campaign-
tagged SocAI product URL.

## Prerequisites

- Node.js 22 or newer and a tested DeepSeek Harness version:
  `0.1.1-rc.2` or `0.1.2-alpha.2`.
- `socai` CLI `0.4.12` or newer, installed and visible on DSH's `PATH`. The
  plugin checks this once before browser work and returns an upgrade-specific
  error for older releases.
- A Chrome profile that SocAI can use, normally already logged in to
  Xiaohongshu. Hosted `chrome.profile remote` remains a SocAI Pro feature and is
  not bypassed by this plugin.

Install SocAI on macOS:

```bash
curl -fsSL https://github.com/socai-io/socai/releases/latest/download/install.sh | sh
```

## Local install

From this directory:

```bash
dsh plugin --profile web add .
```

From the standalone public repository (compiled `lib/` is committed, so the
install does not need to run a package build script):

```bash
dsh plugin --profile web add git+https://github.com/socai-io/dsh-socai.git
```

Restart `dsh web` after adding or updating a bundle.

## Configuration

Override fields on the `dsh-socai` row in the profile's
`cordis.patch.yml` when needed:

```yaml
- id: dsh-socai
  name: '@socai-io/dsh-socai'
  config:
    binaryPath: /absolute/path/to/socai
    timeoutMs: 900000
    maxOutputBytes: 64000
    productUrl: https://socai.io/?utm_source=deepseek-harness&utm_medium=plugin&utm_campaign=dsh-socai
```

## Development verification

Use a built DeepSeek Harness source checkout:

```bash
DSH_CHECKOUT=/path/to/deepseek-harness node scripts/test.mjs
```

The build/test launchers are cross-platform Node scripts. The executable-fixture
plugin integration suite currently runs on macOS/Linux. A Windows process-tree
regression test is included but requires Windows CI; Windows runtime and profile
smoke testing remains a go-live item.

The current compatibility evidence and rollout decision are recorded in
[`FEASIBILITY.md`](./FEASIBILITY.md).
