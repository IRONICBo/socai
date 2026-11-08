import { fileURLToPath } from "node:url";

import { runDurablePiHarness } from "../src/durable-harness.mjs";
import { durableHarnessVerdict } from "../src/harness-verdict.mjs";

const fixture = fileURLToPath(new URL("../test/fixtures/fake-socai.mjs", import.meta.url));

const result = await runDurablePiHarness({
  toolName: "socai_xhs_search",
  toolArguments: { query: "社媒营销", numNotes: 3, preview: true },
  runnerOptions: { bin: process.execPath, prefixArgs: [fixture] },
});
const verdict = durableHarnessVerdict(result);

process.stdout.write(`${JSON.stringify({
  mode: "pi-durable-harness + fake-socai",
  status: verdict.runStatus,
  eventTypes: result.eventTypes,
  argv: result.toolEnd?.result?.details?.result?.argv,
  faulted: result.snapshot.faulted,
  passed: verdict.ok,
  issues: verdict.issues,
}, null, 2)}\n`);

if (!verdict.ok) process.exitCode = 1;
