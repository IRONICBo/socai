# Pi + socai harness results

Date: 2026-11-09

Branch: `feat/pi-socai-harness-dev-20261108`

Baseline: `dev` at `aacb279665095dfe5ae634162e6776fbe2f82967`

## What this verifies

This spike keeps one Agent loop: Pi chooses and executes tools, while the existing socai CLI continues to own Chrome, platform state, extraction, media processing and structured results. The adapter exposes seven existing agentless Xiaohongshu and Douyin commands; it does not call the Rust socai Agent from inside Pi.

## Results

| Layer | Command | Result |
| --- | --- | --- |
| Adapter tests | `npm test` | 11 passed, 0 failed. Covered argument isolation, CLI errors, invalid JSON, cancellation, deadline termination, all seven CLI mappings, Pi continuation and durable-harness completion/abort. |
| Durable contract | `npm run harness:contract` | Completed with `run_start` through `run_end`, one real Pi tool lifecycle, the expected argv and `faulted=false`. |
| Real CLI | `SOCAI_BIN=.../target/debug/socai npm run harness:live` | Xiaohongshu preview search completed with 3 cards. Douyin page-state completed with a readable, non-challenged page. |
| Pi + real socai | `SOCAI_BIN=.../target/debug/socai npm run harness:agent` | Durable run completed, `socai_xhs_search` returned 3 cards, `faulted=false`, and the operation closed. |
| Dependency audit | `npm audit --omit=dev --audit-level=high` | 0 vulnerabilities reported. |

The live Douyin state was `ok=true`, `login_required=false`, `challenge_required=false`, `blank_or_throttled=false`, and `signed_in=false`. This proves the public page is readable through the adapter; it does not claim an authenticated Douyin session.

All seven adapter mappings were also compared with the `--help` contract from the binary built on this branch. The automated Codex CLI review was attempted but returned HTTP 401 before producing a review, so no automated-review pass is claimed.

## Boundary not passed

The real-model harness was not run because no Pi provider/model or supported provider authentication is configured in this environment. `harness:model` fails closed in that condition, so this branch does not claim model-selection or answer-quality acceptance.

This is still an integration spike rather than production wiring. Production work needs versioned tool discovery and schemas, hosted transport, stable progress events, artifact lineage, login/approval resume, cost accounting and workflow evals before replacing the current Agent runtime.
