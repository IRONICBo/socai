import {
  AgentHarness,
  MemorySessionRepo,
  TODO_CONTEXT,
} from "@earendil-works/pi-agent-core";
import { createModels } from "@earendil-works/pi-ai";
import {
  fauxAssistantMessage,
  fauxProvider,
  fauxToolCall,
} from "@earendil-works/pi-ai/providers/faux";

import { createSocaiTools } from "./tools.mjs";

function toHarnessTool(agentTool) {
  return {
    ...agentTool,
    execute(toolCallId, params, onUpdate, _toolContext, _invocation, context) {
      return agentTool.execute(toolCallId, params, context.abortSignal, onUpdate);
    },
  };
}

export async function runDurablePiHarness({
  toolName = "socai_xhs_search",
  toolArguments = { query: "社媒营销", numNotes: 3, preview: true },
  runnerOptions = {},
  prompt = "Use the socai tools to research the requested topic.",
  abortAfterMs,
} = {}) {
  const tools = createSocaiTools(runnerOptions).map(toHarnessTool);
  if (!tools.some((tool) => tool.name === toolName)) {
    throw new Error(`unknown socai tool for durable harness: ${toolName}`);
  }

  const faux = fauxProvider({ provider: "socai-harness", tokensPerSecond: 0 });
  faux.setResponses([
    fauxAssistantMessage(
      fauxToolCall(toolName, toolArguments, { id: "socai-durable-call-1" }),
      { stopReason: "toolUse" },
    ),
    fauxAssistantMessage("socai tool completed inside the Pi durable harness."),
  ]);
  const models = createModels();
  models.setProvider(faux.provider);

  const sessions = new MemorySessionRepo();
  const session = await sessions.create({}, TODO_CONTEXT);
  const { harness } = await AgentHarness.create({
    session,
    models,
    model: faux.getModel(),
    thinkingLevel: "off",
    tools,
    activeToolNames: tools.map((tool) => tool.name),
    systemPrompt: "Use only the provided socai tools for social research and continue from their structured results.",
    toolExecution: "sequential",
  }, TODO_CONTEXT);

  const eventTypes = [];
  let toolEnd;
  const eventNames = [
    "run_start",
    "run_end",
    "turn_start",
    "turn_end",
    "message_start",
    "message_update",
    "message_end",
    "tool_start",
    "tool_update",
    "tool_end",
    "entry_added",
  ];
  const unsubscribers = eventNames.map((type) => harness.events.on(type, (event) => {
    eventTypes.push(event.type);
    if (event.type === "tool_end") toolEnd = event;
  }));

  try {
    const lane = await harness.lane("main", TODO_CONTEXT);
    let abortRequest;
    const abortTimer = Number.isFinite(abortAfterMs)
      ? setTimeout(() => {
          abortRequest = lane.abort(TODO_CONTEXT);
        }, abortAfterMs)
      : undefined;
    const run = await lane.prompt(prompt, undefined, TODO_CONTEXT);
    if (abortTimer) clearTimeout(abortTimer);
    const abort = abortRequest ? await abortRequest : undefined;
    const watch = await lane.watch(TODO_CONTEXT);
    const snapshot = watch.snapshot;
    watch.unsubscribe();
    return { run, abort, eventTypes, toolEnd, snapshot };
  } finally {
    for (const unsubscribe of unsubscribers) unsubscribe();
    await harness.close(TODO_CONTEXT);
    await sessions.close(TODO_CONTEXT);
  }
}
