import { Agent } from "@earendil-works/pi-agent-core";
import { builtinModels } from "@earendil-works/pi-ai/providers/all";

import { createSocaiTools } from "../src/tools.mjs";
import { summarizeSocaiResult } from "../src/result-summary.mjs";

const provider = process.env.PI_SOCAI_PROVIDER;
const modelId = process.env.PI_SOCAI_MODEL;
if (!provider || !modelId) {
  throw new Error("set PI_SOCAI_PROVIDER and PI_SOCAI_MODEL before running the real-model harness");
}

const models = builtinModels();
const model = models.getModel(provider, modelId);
if (!model) throw new Error(`Pi model is not present in the installed catalog: ${provider}/${modelId}`);
const auth = await models.checkAuth(provider);
if (!auth) throw new Error(`Pi provider is not authenticated: ${provider}`);

const tools = createSocaiTools();
const events = [];
let toolCalls = 0;
const agent = new Agent({
  initialState: {
    systemPrompt: "You are testing the socai Pi adapter. Use socai_xhs_search exactly once in preview mode, then summarize only what the tool returned.",
    model,
    tools,
  },
  streamFn: models.streamSimple.bind(models),
  toolExecution: "sequential",
  beforeToolCall: async () => {
    toolCalls += 1;
    if (toolCalls > 1) return { block: true, reason: "model harness allows one socai call" };
  },
});
agent.subscribe((event) => events.push(event));

await agent.prompt(`Search Xiaohongshu for ${process.env.PI_SOCAI_QUERY ?? "社媒营销"}. Request 3 preview cards and do not open post details.`);
const toolResult = agent.state.messages.find((message) => message.role === "toolResult");
const final = [...agent.state.messages].reverse().find((message) => message.role === "assistant");
process.stdout.write(`${JSON.stringify({
  mode: "pi-agent + live-model + live-socai",
  model: `${provider}/${modelId}`,
  toolCalls,
  toolName: toolResult?.toolName,
  toolError: toolResult?.isError,
  toolResult: toolResult?.details?.result ? summarizeSocaiResult(toolResult.details.result) : undefined,
  finalText: final?.content?.filter((item) => item.type === "text").map((item) => item.text).join("") ?? "",
  eventCounts: Object.fromEntries(events.map((event) => event.type).map((type, _, all) => [type, all.filter((item) => item === type).length])),
}, null, 2)}\n`);
