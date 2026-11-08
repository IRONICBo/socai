# @socai/pi-tools spike

This isolated spike exposes the current agentless socai CLI capabilities as Pi tools. Pi owns the model/tool loop; socai continues to own the browser session, platform navigation, login state, extraction, media processing, run directory, and structured result.

It intentionally does not call the existing socai Agent from inside Pi. That would create two nested Agent loops with ambiguous cancellation, retries, cost, and completion state.

## Exposed tools

- `socai_xhs_search`
- `socai_xhs_author`
- `socai_xhs_get_notes`
- `socai_dy_search`
- `socai_dy_author`
- `socai_dy_get_videos`
- `socai_dy_page_state`

The package is also a Pi extension through `src/extension.mjs`. All child processes use argument arrays with `shell: false`. Pi abort signals terminate the CLI client; the existing daemon drops the in-flight request when that client disconnects.

## Harness layers

```bash
npm test
npm run harness:contract

# Point at the binary built from the same dev branch.
SOCAI_BIN=/absolute/path/to/socai npm run harness:live
SOCAI_BIN=/absolute/path/to/socai npm run harness:agent

# Optional real-model routing check. This requires provider auth supported by Pi.
PI_SOCAI_PROVIDER=<provider> PI_SOCAI_MODEL=<model-id> \
SOCAI_BIN=/absolute/path/to/socai npm run harness:model
```

- `harness:contract` runs Pi's durable harness with a deterministic fake CLI and checks the complete tool lifecycle. It fails if Pi rejects or aborts the run, the socai tool fails, the durable session faults, or the operation remains open; Pi's outer `completed` status alone is not sufficient.
- `harness:live` calls the real Xiaohongshu preview search and Douyin page-state tools without a model.
- `harness:agent` runs Pi's durable harness against the real socai CLI with a scripted model. This isolates the Pi-to-socai integration from model quality.
- `harness:model` adds a real model to test tool selection and synthesis. It fails closed when the requested provider is not authenticated.

For the negative contract path, `FAKE_SOCAI_MODE=fail npm run harness:contract` must report the socai CLI error and exit non-zero even though the scripted Pi loop reaches its final message.

The latest local verification record is in [`HARNESS-RESULTS.md`](./HARNESS-RESULTS.md).

This remains a spike. Production work still needs versioned discovery/schema, hosted transport, stable progress events, artifact lineage, login/approval resume, cost accounting, and evals against the same real workflows used by the current Rust Agent.
