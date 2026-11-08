import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { runDurablePiHarness } from "../src/durable-harness.mjs";
import { durableHarnessVerdict } from "../src/harness-verdict.mjs";

const fixture = fileURLToPath(new URL("./fixtures/fake-socai.mjs", import.meta.url));

test("Pi durable harness records the socai tool outcome and completes the run", async () => {
  const result = await runDurablePiHarness({
    toolName: "socai_xhs_search",
    toolArguments: { query: "社媒营销", numNotes: 3, preview: true },
    runnerOptions: { bin: process.execPath, prefixArgs: [fixture] },
  });

  assert.equal(result.run.ok, true);
  assert.equal(result.run.value.status, "completed");
  assert.deepEqual(result.toolEnd.result.details.result.argv, [
    "xhs",
    "search",
    "社媒营销",
    "--num-notes",
    "3",
    "--preview",
  ]);
  assert.ok(result.eventTypes.includes("tool_update"));
  assert.ok(result.eventTypes.includes("run_end"));
  assert.equal(result.snapshot.faulted, false);
  assert.equal(result.snapshot.operation, null);
  assert.deepEqual(durableHarnessVerdict(result), {
    ok: true,
    runStatus: "completed",
    issues: [],
  });
});

test("Pi durable harness abort reaches the running socai CLI process", async () => {
  const result = await runDurablePiHarness({
    toolName: "socai_xhs_search",
    toolArguments: { query: "社媒营销", numNotes: 3, preview: true },
    runnerOptions: {
      bin: process.execPath,
      prefixArgs: [fixture],
      env: { FAKE_SOCAI_MODE: "hang" },
    },
    abortAfterMs: 80,
  });

  assert.equal(result.run.ok, true);
  assert.equal(result.run.value.status, "aborted");
  assert.ok(result.eventTypes.includes("tool_start"));
  assert.ok(result.eventTypes.includes("run_end"));
  assert.equal(result.snapshot.operation, null);
});

test("durable harness verdict rejects a completed Pi run when the socai tool failed", async () => {
  const result = await runDurablePiHarness({
    toolName: "socai_xhs_search",
    toolArguments: { query: "社媒营销", numNotes: 3, preview: true },
    runnerOptions: {
      bin: process.execPath,
      prefixArgs: [fixture],
      env: { FAKE_SOCAI_MODE: "fail" },
    },
  });

  assert.equal(result.run.ok, true);
  assert.equal(result.run.value.status, "completed");
  assert.equal(result.toolEnd.isError, true);
  assert.deepEqual(durableHarnessVerdict(result), {
    ok: false,
    runStatus: "completed",
    issues: ["socai tool failed: socai CLI exited with code 23: browser login is required"],
  });
});
