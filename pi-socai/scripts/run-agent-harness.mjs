import { runDurablePiHarness } from "../src/durable-harness.mjs";
import { summarizeSocaiResult } from "../src/result-summary.mjs";

const result = await runDurablePiHarness({
  toolName: "socai_xhs_search",
  toolArguments: {
    query: process.env.PI_SOCAI_QUERY ?? "社媒营销",
    numNotes: Number(process.env.PI_SOCAI_NUM_NOTES ?? 3),
    numComments: 0,
    preview: true,
  },
});

const toolDetails = result.toolEnd?.result?.details;
process.stdout.write(`${JSON.stringify({
  mode: "pi-durable-harness + live-socai + scripted-model",
  status: result.run.ok ? result.run.value.status : "admission_failed",
  operationId: result.run.ok ? result.run.value.operationId : undefined,
  eventTypes: result.eventTypes,
  tool: result.toolEnd?.toolName,
  toolStatus: toolDetails?.status,
  toolResult: toolDetails?.result ? summarizeSocaiResult(toolDetails.result) : undefined,
  faulted: result.snapshot.faulted,
  operationOpen: result.snapshot.operation !== null,
}, null, 2)}\n`);

if (!result.run.ok || result.run.value.status !== "completed") process.exitCode = 1;
