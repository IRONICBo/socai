import { Agent } from "@earendil-works/pi-agent-core";
import { createAssistantMessageEventStream } from "@earendil-works/pi-ai";

import { createSocaiTools } from "./tools.mjs";

const SCRIPTED_MODEL = {
  id: "socai-scripted-harness",
  name: "socai scripted harness",
  api: "openai-completions",
  provider: "socai-harness",
  baseUrl: "http://127.0.0.1.invalid",
  reasoning: false,
  input: ["text"],
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
  contextWindow: 32_000,
  maxTokens: 2_000,
};

function emptyUsage() {
  return {
    input: 0,
    output: 0,
    cacheRead: 0,
    cacheWrite: 0,
    totalTokens: 0,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total: 0 },
  };
}

function assistantMessage(content, stopReason) {
  return {
    role: "assistant",
    content,
    api: SCRIPTED_MODEL.api,
    provider: SCRIPTED_MODEL.provider,
    model: SCRIPTED_MODEL.id,
    usage: emptyUsage(),
    stopReason,
    timestamp: Date.now(),
  };
}

function scriptedStream(toolName, toolArguments) {
  return async (_model, context) => {
    const hasToolResult = context.messages.some((message) => message.role === "toolResult");
    const message = hasToolResult
      ? assistantMessage(
          [{ type: "text", text: "socai tool completed and Pi continued from the structured result." }],
          "stop",
        )
      : assistantMessage(
          [{ type: "toolCall", id: "socai-call-1", name: toolName, arguments: toolArguments }],
          "toolUse",
        );
    const stream = createAssistantMessageEventStream();
    stream.push({ type: "start", partial: message });
    stream.push({
      type: "done",
      reason: message.stopReason,
      message,
    });
    return stream;
  };
}

export async function runScriptedPiHarness({
  toolName = "socai_xhs_search",
  toolArguments = { query: "社媒营销", numNotes: 3, preview: true },
  runnerOptions = {},
  prompt = "Use the socai tools to research the requested topic.",
} = {}) {
  const tools = createSocaiTools(runnerOptions);
  if (!tools.some((tool) => tool.name === toolName)) {
    throw new Error(`unknown socai tool for harness: ${toolName}`);
  }

  const agent = new Agent({
    initialState: {
      systemPrompt: "Use only the provided socai tools for social research. Continue from structured tool results.",
      model: SCRIPTED_MODEL,
      tools,
    },
    streamFn: scriptedStream(toolName, toolArguments),
    toolExecution: "sequential",
  });
  const eventTypes = [];
  agent.subscribe((event) => {
    eventTypes.push(event.type);
  });

  await agent.prompt(prompt);
  const toolResult = agent.state.messages.find((message) => message.role === "toolResult");
  const finalAssistant = [...agent.state.messages]
    .reverse()
    .find((message) => message.role === "assistant");
  const finalText = finalAssistant?.content
    ?.filter((item) => item.type === "text")
    .map((item) => item.text)
    .join("") ?? "";

  return {
    eventTypes,
    finalText,
    messages: agent.state.messages,
    toolResult,
  };
}
