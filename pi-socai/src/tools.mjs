import { Type } from "typebox";

import { runSocaiCli } from "./cli-runner.mjs";

const MAX_MODEL_RESULT_CHARS = 80_000;

const positiveCount = (description) => Type.Optional(Type.Integer({ minimum: 0, description }));
const optionalFlag = (description) => Type.Optional(Type.Boolean({ description }));
const stringList = (description) => Type.Array(Type.String(), { minItems: 1, description });

function addNumber(args, flag, value) {
  if (Number.isInteger(value)) args.push(flag, String(value));
}

function addFlag(args, flag, value) {
  if (value === true) args.push(flag);
}

function addRepeated(args, flag, values = []) {
  for (const value of values) args.push(flag, value);
}

function modelText(result) {
  const json = JSON.stringify(result, null, 2);
  if (json.length <= MAX_MODEL_RESULT_CHARS) return { text: json, truncated: false };
  return {
    text: `${json.slice(0, MAX_MODEL_RESULT_CHARS)}\n... [truncated for model context]`,
    truncated: true,
  };
}

function tool({ name, label, description, parameters, command, buildArgs, runnerOptions }) {
  return {
    name,
    label,
    description,
    parameters,
    executionMode: "sequential",
    replay: "never",
    async execute(_toolCallId, params, signal, onUpdate) {
      onUpdate?.({
        content: [{ type: "text", text: `Running socai ${command}...` }],
        details: { command, status: "running" },
      });
      const result = await runSocaiCli(buildArgs(params), { ...runnerOptions, signal });
      const content = modelText(result);
      return {
        content: [{ type: "text", text: content.text }],
        details: {
          command,
          status: "completed",
          truncatedForModel: content.truncated,
          result,
        },
      };
    },
  };
}

export function createSocaiTools(runnerOptions = {}) {
  return [
    tool({
      name: "socai_xhs_search",
      label: "Search Xiaohongshu",
      description: "Search Xiaohongshu through the existing socai browser runtime. Use preview for result cards, or disable preview to read post bodies and comments.",
      command: "xhs search",
      runnerOptions,
      parameters: Type.Object({
        query: Type.String({ minLength: 1, description: "Search query." }),
        numNotes: positiveCount("Number of notes or cards to collect."),
        numComments: positiveCount("Comments and replies per opened note; 0 skips comments."),
        preview: optionalFlag("Return result cards without opening each note."),
        filters: Type.Optional(Type.Array(Type.String({ description: "A socai filter in group=option form." }))),
        downloadMedia: optionalFlag("Download note media into the run directory."),
        ocr: optionalFlag("Run OCR on images or video covers."),
        transcribeAudio: optionalFlag("Transcribe audio from opened video notes."),
      }, { additionalProperties: false }),
      buildArgs(params) {
        const args = ["xhs", "search", params.query];
        addNumber(args, "--num-notes", params.numNotes);
        addNumber(args, "--num-comments", params.numComments);
        addFlag(args, "--preview", params.preview);
        addRepeated(args, "--filter", params.filters);
        addFlag(args, "--download-media", params.downloadMedia);
        addFlag(args, "--ocr", params.ocr);
        addFlag(args, "--transcribe-audio", params.transcribeAudio);
        return args;
      },
    }),
    tool({
      name: "socai_xhs_author",
      label: "Read Xiaohongshu author",
      description: "Read a Xiaohongshu author profile and their notes through the existing socai browser runtime.",
      command: "xhs author",
      runnerOptions,
      parameters: Type.Object({
        authorId: Type.String({ minLength: 1, description: "Trailing author id from /user/profile/<id>." }),
        numNotes: positiveCount("Number of author notes to collect."),
        numComments: positiveCount("Comments and replies per opened note; 0 skips comments."),
        preview: optionalFlag("Return profile and note cards without opening each note."),
        downloadMedia: optionalFlag("Download media for opened notes."),
        ocr: optionalFlag("Run OCR on note images or covers."),
        transcribeAudio: optionalFlag("Transcribe audio from opened video notes."),
      }, { additionalProperties: false }),
      buildArgs(params) {
        const args = ["xhs", "author", params.authorId];
        addNumber(args, "--num-notes", params.numNotes);
        addNumber(args, "--num-comments", params.numComments);
        addFlag(args, "--preview", params.preview);
        addFlag(args, "--download-media", params.downloadMedia);
        addFlag(args, "--ocr", params.ocr);
        addFlag(args, "--transcribe-audio", params.transcribeAudio);
        return args;
      },
    }),
    tool({
      name: "socai_xhs_get_notes",
      label: "Read Xiaohongshu notes",
      description: "Read selected Xiaohongshu notes by note id and xsec token through the existing socai runtime.",
      command: "xhs get-notes",
      runnerOptions,
      parameters: Type.Object({
        notes: stringList("Entries in NOTE_ID=XSEC_TOKEN form."),
        numComments: positiveCount("Comments and replies per note; 0 skips comments."),
        downloadMedia: optionalFlag("Download note media into the run directory."),
        ocr: optionalFlag("Run OCR on images or video covers."),
        transcribeAudio: optionalFlag("Transcribe audio from video notes."),
      }, { additionalProperties: false }),
      buildArgs(params) {
        const args = ["xhs", "get-notes"];
        addRepeated(args, "--note", params.notes);
        addNumber(args, "--num-comments", params.numComments);
        addFlag(args, "--download-media", params.downloadMedia);
        addFlag(args, "--ocr", params.ocr);
        addFlag(args, "--transcribe-audio", params.transcribeAudio);
        return args;
      },
    }),
    tool({
      name: "socai_dy_search",
      label: "Search Douyin",
      description: "Search Douyin and collect video result cards through the existing socai browser runtime.",
      command: "dy search",
      runnerOptions,
      parameters: Type.Object({
        query: Type.String({ minLength: 1, description: "Search query." }),
        num: positiveCount("Number of video cards to collect."),
        waitSeconds: positiveCount("Maximum wait for page and search transitions."),
      }, { additionalProperties: false }),
      buildArgs(params) {
        const args = ["dy", "search", params.query];
        addNumber(args, "--num", params.num);
        addNumber(args, "--wait-seconds", params.waitSeconds);
        return args;
      },
    }),
    tool({
      name: "socai_dy_author",
      label: "Read Douyin author",
      description: "Read a Douyin author profile and their visible video cards through the existing socai browser runtime.",
      command: "dy author",
      runnerOptions,
      parameters: Type.Object({
        author: Type.String({ minLength: 1, description: "Douyin author id or profile URL." }),
        num: positiveCount("Number of visible video cards to collect."),
        waitSeconds: positiveCount("Maximum wait for the author page transition."),
      }, { additionalProperties: false }),
      buildArgs(params) {
        const args = ["dy", "author", params.author];
        addNumber(args, "--num", params.num);
        addNumber(args, "--wait-seconds", params.waitSeconds);
        return args;
      },
    }),
    tool({
      name: "socai_dy_get_videos",
      label: "Read Douyin videos",
      description: "Read selected Douyin video details and top comments through the existing socai browser runtime.",
      command: "dy get-videos",
      runnerOptions,
      parameters: Type.Object({
        videos: stringList("Douyin video ids or URLs."),
        numComments: positiveCount("Top comments per video; 0 skips comments."),
        downloadMedia: optionalFlag("Download playable video and cover into the run directory."),
        transcribeAudio: optionalFlag("Download and transcribe video audio."),
        waitSeconds: positiveCount("Maximum wait for each video page transition."),
      }, { additionalProperties: false }),
      buildArgs(params) {
        const args = ["dy", "get-videos"];
        addRepeated(args, "--video", params.videos);
        addNumber(args, "--num-comments", params.numComments);
        addFlag(args, "--download-media", params.downloadMedia);
        addFlag(args, "--transcribe-audio", params.transcribeAudio);
        addNumber(args, "--wait-seconds", params.waitSeconds);
        return args;
      },
    }),
    tool({
      name: "socai_dy_page_state",
      label: "Inspect Douyin page",
      description: "Open or reuse Douyin and return the current page, login, challenge, and hydration state.",
      command: "dy page_state",
      runnerOptions,
      parameters: Type.Object({
        waitSeconds: positiveCount("Maximum wait for a non-blank Douyin page."),
      }, { additionalProperties: false }),
      buildArgs(params) {
        const args = ["dy", "page_state"];
        addNumber(args, "--wait-seconds", params.waitSeconds);
        return args;
      },
    }),
  ];
}

export { SocaiCliError, runSocaiCli } from "./cli-runner.mjs";
