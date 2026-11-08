import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { runScriptedPiHarness } from "../src/scripted-harness.mjs";

const fixture = fileURLToPath(new URL("./fixtures/fake-socai.mjs", import.meta.url));

test("Pi executes a socai tool and continues from its result", async () => {
  const result = await runScriptedPiHarness({
    toolName: "socai_xhs_search",
    toolArguments: { query: "社媒营销", numNotes: 3, preview: true },
    runnerOptions: { bin: process.execPath, prefixArgs: [fixture] },
  });

  assert.deepEqual(result.eventTypes, [
    "agent_start",
    "turn_start",
    "message_start",
    "message_end",
    "message_start",
    "message_end",
    "tool_execution_start",
    "tool_execution_update",
    "tool_execution_end",
    "message_start",
    "message_end",
    "turn_end",
    "turn_start",
    "message_start",
    "message_end",
    "turn_end",
    "agent_end",
  ]);
  assert.deepEqual(result.toolResult.details.result.argv, [
    "xhs",
    "search",
    "社媒营销",
    "--num-notes",
    "3",
    "--preview",
  ]);
  assert.match(result.finalText, /socai tool completed/);
});
