import { createSocaiTools } from "../src/tools.mjs";
import { summarizeSocaiResult } from "../src/result-summary.mjs";

const tools = createSocaiTools();
const cases = [
  {
    name: "xhs-preview-search",
    tool: "socai_xhs_search",
    params: {
      query: process.env.PI_SOCAI_QUERY ?? "社媒营销",
      numNotes: Number(process.env.PI_SOCAI_NUM_NOTES ?? 3),
      numComments: 0,
      preview: true,
    },
  },
  {
    name: "dy-page-state",
    tool: "socai_dy_page_state",
    params: { waitSeconds: Number(process.env.PI_SOCAI_DY_WAIT_SECONDS ?? 30) },
    skip: process.env.PI_SOCAI_SKIP_DY === "1",
  },
];

const results = [];
for (const entry of cases) {
  if (entry.skip) {
    results.push({ name: entry.name, status: "skipped" });
    continue;
  }
  const tool = tools.find((candidate) => candidate.name === entry.tool);
  try {
    const outcome = await tool.execute(`live-${entry.name}`, entry.params);
    results.push({
      name: entry.name,
      status: "completed",
      command: outcome.details.command,
      summary: summarizeSocaiResult(outcome.details.result),
    });
  } catch (error) {
    results.push({
      name: entry.name,
      status: "failed",
      error: error instanceof Error ? error.message : String(error),
    });
  }
}

process.stdout.write(`${JSON.stringify({
  mode: "direct-tools + live-socai",
  socaiBin: process.env.SOCAI_BIN ?? "socai",
  results,
}, null, 2)}\n`);

if (results.some((result) => result.status === "failed")) process.exitCode = 1;
