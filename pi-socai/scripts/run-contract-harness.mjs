import { fileURLToPath } from "node:url";

import { runDurablePiHarness } from "../src/durable-harness.mjs";

const fixture = fileURLToPath(new URL("../test/fixtures/fake-socai.mjs", import.meta.url));

const result = await runDurablePiHarness({
  toolName: "socai_xhs_search",
  toolArguments: { query: "社媒营销", numNotes: 3, preview: true },
  runnerOptions: { bin: process.execPath, prefixArgs: [fixture] },
});

process.stdout.write(`${JSON.stringify({
  mode: "pi-durable-harness + fake-socai",
  status: result.run.ok ? result.run.value.status : "admission_failed",
  eventTypes: result.eventTypes,
  argv: result.toolEnd?.result?.details?.result?.argv,
  faulted: result.snapshot.faulted,
}, null, 2)}\n`);
