import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { createSocaiTools } from "../src/tools.mjs";

const fixture = fileURLToPath(new URL("./fixtures/fake-socai.mjs", import.meta.url));

function makeTools() {
  return createSocaiTools({
    bin: process.execPath,
    prefixArgs: [fixture],
  });
}

test("exposes current agentless Xiaohongshu and Douyin capabilities", () => {
  assert.deepEqual(
    makeTools().map((tool) => tool.name),
    [
      "socai_xhs_search",
      "socai_xhs_author",
      "socai_xhs_get_notes",
      "socai_dy_search",
      "socai_dy_author",
      "socai_dy_get_videos",
      "socai_dy_page_state",
    ],
  );
});

test("maps a Pi search tool call to the existing socai CLI contract", async () => {
  const tool = makeTools().find((candidate) => candidate.name === "socai_xhs_search");
  const result = await tool.execute(
    "call-1",
    {
      query: "轻食创业",
      numNotes: 5,
      numComments: 0,
      preview: true,
      filters: ["publish_time=一周内", "sort=最新"],
    },
    undefined,
    undefined,
  );

  const payload = result.details.result;
  assert.deepEqual(payload.argv, [
    "xhs",
    "search",
    "轻食创业",
    "--num-notes",
    "5",
    "--num-comments",
    "0",
    "--preview",
    "--filter",
    "publish_time=一周内",
    "--filter",
    "sort=最新",
  ]);
  assert.equal(result.details.command, "xhs search");
  assert.equal(result.content[0].type, "text");
});

test("maps every remaining Pi tool to the current agentless CLI contract", async () => {
  const cases = [
    {
      tool: "socai_xhs_author",
      params: {
        authorId: "author-1",
        numNotes: 4,
        numComments: 2,
        downloadMedia: true,
        ocr: true,
        transcribeAudio: true,
      },
      argv: [
        "xhs", "author", "author-1",
        "--num-notes", "4",
        "--num-comments", "2",
        "--download-media",
        "--ocr",
        "--transcribe-audio",
      ],
    },
    {
      tool: "socai_xhs_get_notes",
      params: {
        notes: ["note-1=token-1", "note-2=token-2"],
        numComments: 0,
        downloadMedia: true,
        ocr: true,
        transcribeAudio: true,
      },
      argv: [
        "xhs", "get-notes",
        "--note", "note-1=token-1",
        "--note", "note-2=token-2",
        "--num-comments", "0",
        "--download-media",
        "--ocr",
        "--transcribe-audio",
      ],
    },
    {
      tool: "socai_dy_search",
      params: { query: "无极机车", num: 7, waitSeconds: 90 },
      argv: ["dy", "search", "无极机车", "--num", "7", "--wait-seconds", "90"],
    },
    {
      tool: "socai_dy_author",
      params: { author: "https://www.douyin.com/user/example", num: 4, waitSeconds: 30 },
      argv: [
        "dy", "author", "https://www.douyin.com/user/example",
        "--num", "4",
        "--wait-seconds", "30",
      ],
    },
    {
      tool: "socai_dy_get_videos",
      params: {
        videos: ["video-1", "https://www.douyin.com/video/2"],
        numComments: 0,
        downloadMedia: true,
        transcribeAudio: true,
        waitSeconds: 45,
      },
      argv: [
        "dy", "get-videos",
        "--video", "video-1",
        "--video", "https://www.douyin.com/video/2",
        "--num-comments", "0",
        "--download-media",
        "--transcribe-audio",
        "--wait-seconds", "45",
      ],
    },
    {
      tool: "socai_dy_page_state",
      params: { waitSeconds: 60 },
      argv: ["dy", "page_state", "--wait-seconds", "60"],
    },
  ];

  const tools = makeTools();
  for (const entry of cases) {
    const tool = tools.find((candidate) => candidate.name === entry.tool);
    const result = await tool.execute(`call-${entry.tool}`, entry.params);
    assert.deepEqual(result.details.result.argv, entry.argv, entry.tool);
  }
});
