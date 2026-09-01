import { defineTool } from "@deepseek-ai/dsh-tools";
import z from "@deepseek-ai/schemastery";
import { createHash } from "node:crypto";
import { spawn } from "node:child_process";
import { homedir } from "node:os";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { BUNDLED_SKILL_RANK } from "@deepseek-ai/dsh-skill";
//#region src/runner.ts
const INSTALL_URL = "https://socai.io/?utm_source=deepseek-harness&utm_medium=plugin&utm_campaign=dsh-socai";
const TERMINATION_GRACE_MS = 1e3;
const FORCE_SETTLE_GRACE_MS = 1e3;
const TASKKILL_WAIT_MS = 1e3;
function shortOutput(value, limit = 2e3) {
	const trimmed = value.trim();
	return trimmed.length <= limit ? trimmed : `${trimmed.slice(0, limit)}…`;
}
function redactLocalPaths(value) {
	let redacted = value.replace(/^run_dir:\s*.+$/gm, "run_dir: [redacted]");
	const home = homedir();
	if (home !== "") redacted = redacted.split(home).join("[home]");
	return redacted;
}
function taskkillProcessTree(child, force) {
	const pid = child.pid;
	if (pid === void 0) {
		child.kill(force ? "SIGKILL" : "SIGTERM");
		return Promise.resolve();
	}
	return new Promise((resolve) => {
		let settled = false;
		const killer = spawn("taskkill", [
			"/pid",
			String(pid),
			"/T",
			...force ? ["/F"] : []
		], {
			stdio: "ignore",
			windowsHide: true
		});
		const finish = (fallback) => {
			if (settled) return;
			settled = true;
			clearTimeout(wait);
			if (fallback && force) child.kill("SIGKILL");
			resolve();
		};
		const wait = setTimeout(() => {
			killer.kill("SIGKILL");
			finish(true);
		}, TASKKILL_WAIT_MS);
		killer.once("error", () => finish(true));
		killer.once("close", (code) => finish(code !== 0));
	});
}
function signalProcessTree(child, force) {
	const pid = child.pid;
	if (!force && (child.exitCode !== null || child.signalCode !== null)) return Promise.resolve();
	if (process.platform === "win32") return taskkillProcessTree(child, force);
	if (pid !== void 0) try {
		process.kill(-pid, force ? "SIGKILL" : "SIGTERM");
		return Promise.resolve();
	} catch {}
	child.kill(force ? "SIGKILL" : "SIGTERM");
	return Promise.resolve();
}
/** Run an executable directly with an argv array; no shell is involved. */
function runProcess(executable, args, options) {
	if (options.signal?.aborted) return Promise.reject(/* @__PURE__ */ new Error("socai command was cancelled"));
	return new Promise((resolve, reject) => {
		let settled = false;
		const stdoutChunks = [];
		const stderrChunks = [];
		let stdoutBytes = 0;
		let stderrBytes = 0;
		let outputBytes = 0;
		let failure;
		let forceTimer;
		let forceSettleTimer;
		const child = spawn(executable, [...args], {
			stdio: [
				"ignore",
				"pipe",
				"pipe"
			],
			windowsHide: true,
			detached: process.platform !== "win32"
		});
		const finish = (callback) => {
			if (settled) return;
			settled = true;
			clearTimeout(timeout);
			clearTimeout(forceTimer);
			clearTimeout(forceSettleTimer);
			options.signal?.removeEventListener("abort", onAbort);
			callback();
		};
		const failAndStop = (error) => {
			if (failure !== void 0) return;
			failure = error;
			signalProcessTree(child, false);
			forceTimer = setTimeout(() => {
				signalProcessTree(child, true).then(() => {
					forceSettleTimer = setTimeout(() => {
						child.stdout.destroy();
						child.stderr.destroy();
						finish(() => reject(failure ?? error));
					}, FORCE_SETTLE_GRACE_MS);
				});
			}, options.terminationGraceMs ?? TERMINATION_GRACE_MS);
		};
		const capture = (stream, chunk) => {
			outputBytes += chunk.byteLength;
			if (outputBytes > options.maxOutputBytes) {
				failAndStop(/* @__PURE__ */ new Error(`socai output exceeded ${options.maxOutputBytes} bytes; retry with fewer notes/comments or preview=true`));
				return;
			}
			if (stream === "stdout") {
				stdoutChunks.push(chunk);
				stdoutBytes += chunk.byteLength;
			} else {
				stderrChunks.push(chunk);
				stderrBytes += chunk.byteLength;
			}
		};
		const onAbort = () => failAndStop(/* @__PURE__ */ new Error("socai command was cancelled"));
		const timeout = setTimeout(() => {
			failAndStop(/* @__PURE__ */ new Error(`socai command timed out after ${options.timeoutMs} ms`));
		}, options.timeoutMs);
		timeout.unref();
		child.stdout.on("data", (chunk) => capture("stdout", chunk));
		child.stderr.on("data", (chunk) => capture("stderr", chunk));
		child.on("error", (error) => {
			const message = error.code === "ENOENT" ? `socai CLI was not found. Configure binaryPath or install it from ${options.installUrl ?? INSTALL_URL}, then restart DSH.` : `failed to start socai CLI (${error.code ?? "spawn error"})`;
			finish(() => reject(new Error(message)));
		});
		child.on("close", (code) => {
			finish(() => {
				if (failure !== void 0) {
					reject(failure);
					return;
				}
				resolve({
					code: code ?? -1,
					stdout: Buffer.concat(stdoutChunks, stdoutBytes).toString("utf8"),
					stderr: Buffer.concat(stderrChunks, stderrBytes).toString("utf8")
				});
			});
		});
		options.signal?.addEventListener("abort", onAbort, { once: true });
	});
}
function extractRunDir(stderr) {
	return [...stderr.matchAll(/^run_dir:\s*(.+)$/gm)].at(-1)?.[1]?.trim() || void 0;
}
/** Execute one socai CLI command and validate its machine-readable JSON output. */
async function runSocai(executable, args, options) {
	const result = await runProcess(executable, args, options);
	if (result.code !== 0) {
		const detail = shortOutput(redactLocalPaths(result.stderr)) || shortOutput(redactLocalPaths(result.stdout)) || `exit code ${result.code}`;
		throw new Error(`socai command failed: ${detail}`);
	}
	try {
		return {
			data: JSON.parse(result.stdout),
			runDir: extractRunDir(result.stderr)
		};
	} catch {
		throw new Error(`socai command returned invalid JSON: ${shortOutput(redactLocalPaths(result.stdout)) || "(empty stdout)"}`);
	}
}
//#endregion
//#region src/skill.ts
const PROVIDER_NAME = "dsh-socai";
const RESOURCE_BASE = {
	kind: "directory",
	path: fileURLToPath(new URL("../assets/", import.meta.url))
};
const CANDIDATE = {
	name: "socai-xhs-research",
	description: "Research Xiaohongshu with socai: start with a broad preview, select useful notes, then deep-read bodies/comments or scan an author. Load when the user asks for Xiaohongshu topic, audience, content, or competitor research.",
	invocation: {
		modelInvocable: true,
		userInvocable: true
	},
	provider: PROVIDER_NAME,
	source: "bundled",
	resourceBase: RESOURCE_BASE,
	rank: BUNDLED_SKILL_RANK,
	locator: new URL("../assets/socai-xhs-research.md", import.meta.url)
};
const socaiSkillProvider = {
	name: PROVIDER_NAME,
	list: () => Promise.resolve([CANDIDATE]),
	async get(candidate) {
		if (candidate.name !== CANDIDATE.name) throw new Error(`unknown dsh-socai skill: ${candidate.name}`);
		return {
			name: CANDIDATE.name,
			description: CANDIDATE.description,
			invocation: CANDIDATE.invocation,
			provider: CANDIDATE.provider,
			source: CANDIDATE.source,
			resourceBase: RESOURCE_BASE,
			content: await readFile(CANDIDATE.locator, "utf8")
		};
	}
};
//#endregion
//#region src/index.ts
const name = "dsh-socai";
const inject = [
	"tools",
	"systemPrompt",
	"skills"
];
const DEFAULT_PRODUCT_URL = "https://socai.io/?utm_source=deepseek-harness&utm_medium=plugin&utm_campaign=dsh-socai";
const MINIMUM_SOCAI_VERSION = "0.4.12";
const DEFAULT_OUTPUT_BYTES = 64e3;
const Config = z.object({
	binaryPath: z.string().default("socai").description("socai CLI executable name or absolute path."),
	timeoutMs: z.number().step(1).min(1e3).default(9e5).description("Maximum runtime for one socai browser command in milliseconds."),
	maxOutputBytes: z.number().step(1).min(1e4).max(256e3).default(DEFAULT_OUTPUT_BYTES).description("Byte cap for child output and the final model-visible result. Lower note counts or use preview mode if exceeded."),
	productUrl: z.string().default(DEFAULT_PRODUCT_URL).description("Product/install link included in successful tool results and missing-CLI guidance.")
});
const PROMPT_TEXT = `## Xiaohongshu research with socai
Use the socai_xhs_* tools when a user asks to research Xiaohongshu topics, posts, audiences, or authors. For broad discovery, start with socai_xhs_search and preview=true; deep-read only the selected notes with socai_xhs_get_notes, or set preview=false for a small focused scan. Use socai_xhs_author for creator/competitor research. These tools operate the user's local logged-in Chrome through the socai CLI, so do not run them in parallel. Load the socai-xhs-research skill for multi-step research.`;
const OUTPUT_SCHEMA = {
	type: "object",
	additionalProperties: false,
	properties: {
		data: {
			type: "json",
			required: true
		},
		run_id: { type: "string" },
		product_url: {
			type: "string",
			required: true
		}
	}
};
function output(result, productUrl, maxOutputBytes) {
	const runId = result.runDir === void 0 ? void 0 : createHash("sha256").update(result.runDir).digest("hex").slice(0, 16);
	const value = {
		data: result.data,
		...runId === void 0 || runId === "" ? {} : { run_id: runId },
		product_url: productUrl
	};
	const renderedBytes = Buffer.byteLength(JSON.stringify(value, null, 2));
	if (renderedBytes > maxOutputBytes) throw new Error(`socai result would expose ${renderedBytes} bytes to the model, above the ${maxOutputBytes}-byte limit; retry with fewer notes/comments or preview=true`);
	return value;
}
const OUTPUT = {
	schema: OUTPUT_SCHEMA,
	render: (_args, value) => [{
		type: "text",
		text: JSON.stringify(value, null, 2)
	}]
};
function count(value, name, minimum, maximum) {
	if (value === void 0) return;
	if (!Number.isInteger(value) || value < minimum || value > maximum) throw new Error(`${name} must be an integer from ${minimum} to ${maximum}`);
}
function nonEmpty(value, name) {
	const trimmed = value.trim();
	if (trimmed === "") throw new Error(`${name} must not be empty`);
	return trimmed;
}
function pushNumber(args, flag, value) {
	if (value !== void 0) args.push(flag, String(value));
}
function pushFlag(args, flag, value) {
	if (value === true) args.push(flag);
}
function parseVersion(value) {
	const match = value.match(/\bsocai\s+v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?\b/i);
	if (match === null) return void 0;
	return {
		core: [
			Number(match[1]),
			Number(match[2]),
			Number(match[3])
		],
		...match[4] === void 0 ? {} : { prerelease: match[4] }
	};
}
function isVersionAtLeast(actual, minimum) {
	for (let index = 0; index < actual.core.length; index += 1) if (actual.core[index] !== minimum.core[index]) return actual.core[index] > minimum.core[index];
	if (minimum.prerelease === void 0) return actual.prerelease === void 0;
	if (actual.prerelease === void 0) return true;
	return actual.prerelease.localeCompare(minimum.prerelease, "en", { numeric: true }) >= 0;
}
function displayVersion(version) {
	return `${version.core.join(".")}${version.prerelease === void 0 ? "" : `-${version.prerelease}`}`;
}
var SerialQueue = class {
	locked = false;
	waiters = [];
	async run(signal, task) {
		const release = await this.acquire(signal);
		try {
			return await task();
		} finally {
			release();
		}
	}
	acquire(signal) {
		if (signal.aborted) return Promise.reject(/* @__PURE__ */ new Error("socai command was cancelled"));
		if (!this.locked) {
			this.locked = true;
			return Promise.resolve(this.makeRelease());
		}
		return new Promise((resolve, reject) => {
			const waiter = {
				signal,
				resolve,
				reject,
				onAbort: () => {
					const index = this.waiters.indexOf(waiter);
					if (index !== -1) this.waiters.splice(index, 1);
					reject(/* @__PURE__ */ new Error("socai command was cancelled while waiting for the browser"));
				}
			};
			this.waiters.push(waiter);
			signal.addEventListener("abort", waiter.onAbort, { once: true });
			if (signal.aborted) waiter.onAbort();
		});
	}
	makeRelease() {
		let released = false;
		return () => {
			if (released) return;
			released = true;
			const next = this.waiters.shift();
			if (next === void 0) {
				this.locked = false;
				return;
			}
			next.signal.removeEventListener("abort", next.onAbort);
			next.resolve(this.makeRelease());
		};
	}
};
function apply(ctx, config) {
	const resolved = {
		binaryPath: config.binaryPath ?? "socai",
		timeoutMs: config.timeoutMs ?? 9e5,
		maxOutputBytes: config.maxOutputBytes ?? DEFAULT_OUTPUT_BYTES,
		productUrl: config.productUrl ?? DEFAULT_PRODUCT_URL
	};
	const queue = new SerialQueue();
	let compatibleBinary;
	const ensureCompatible = async (signal) => {
		if (compatibleBinary === resolved.binaryPath) return;
		const version = await runProcess(resolved.binaryPath, ["--version"], {
			signal,
			timeoutMs: Math.min(resolved.timeoutMs, 1e4),
			maxOutputBytes: 1e4,
			installUrl: resolved.productUrl
		});
		const parsed = parseVersion(`${version.stdout}\n${version.stderr}`);
		const minimum = parseVersion(`socai ${MINIMUM_SOCAI_VERSION}`);
		if (version.code !== 0 || parsed === void 0 || minimum === void 0) throw new Error(`could not verify the socai CLI version; dsh-socai requires socai >= ${MINIMUM_SOCAI_VERSION}. Upgrade from ${resolved.productUrl}`);
		if (!isVersionAtLeast(parsed, minimum)) throw new Error(`socai ${displayVersion(parsed)} is too old; dsh-socai requires socai >= ${MINIMUM_SOCAI_VERSION}. Upgrade from ${resolved.productUrl}`);
		compatibleBinary = resolved.binaryPath;
	};
	const execute = (args, signal) => queue.run(signal, async () => {
		await ensureCompatible(signal);
		return output(await runSocai(resolved.binaryPath, args, {
			signal,
			timeoutMs: resolved.timeoutMs,
			maxOutputBytes: resolved.maxOutputBytes,
			installUrl: resolved.productUrl
		}), resolved.productUrl, resolved.maxOutputBytes);
	});
	ctx.effect(() => ctx.tools.register(defineTool({
		name: "socai_xhs_search",
		description: "Search Xiaohongshu with the local socai browser agent. Preview result cards for broad discovery, or open a focused set to return bodies and top comments. Requires the socai CLI and a usable Chrome profile.",
		parameters: {
			query: {
				type: "string",
				required: true,
				description: "Xiaohongshu search query."
			},
			filters: {
				type: "array",
				items: { type: "string" },
				description: "Optional group=option filters, e.g. publish_time=一周内, note_type=图文, sort=最新."
			},
			num_notes: {
				type: "integer",
				description: "Notes/cards to collect. Maximum 50; default 10."
			},
			num_comments: {
				type: "integer",
				description: "Comments per opened note. Maximum 100; default 8."
			},
			preview: {
				type: "boolean",
				description: "Return cards only without opening every note."
			},
			download_media: {
				type: "boolean",
				description: "Download note media into the socai run directory."
			},
			ocr: {
				type: "boolean",
				description: "Run local OCR on note images or video covers."
			},
			transcribe_audio: {
				type: "boolean",
				description: "Transcribe video audio when the configured socai model supports it."
			}
		},
		output: OUTPUT,
		timeoutMs: resolved.timeoutMs,
		isConcurrencySafe: () => false,
		async execute(args, exec) {
			const query = nonEmpty(args.query, "query");
			count(args.num_notes, "num_notes", 1, 50);
			count(args.num_comments, "num_comments", 0, 100);
			const cliArgs = [
				"xhs",
				"search",
				query
			];
			for (const filter of args.filters ?? []) cliArgs.push("--filter", nonEmpty(filter, "filter"));
			pushNumber(cliArgs, "--num-notes", args.num_notes);
			pushNumber(cliArgs, "--num-comments", args.num_comments);
			pushFlag(cliArgs, "--preview", args.preview);
			pushFlag(cliArgs, "--download-media", args.download_media);
			pushFlag(cliArgs, "--ocr", args.ocr);
			pushFlag(cliArgs, "--transcribe-audio", args.transcribe_audio);
			return execute(cliArgs, exec.signal);
		}
	})), "dsh-socai.search");
	ctx.effect(() => ctx.tools.register(defineTool({
		name: "socai_xhs_author",
		description: "Scan one Xiaohongshu author with the local socai browser agent: profile metadata plus note cards, bodies, and comments. Requires the socai CLI and a usable Chrome profile.",
		parameters: {
			author_id: {
				type: "string",
				required: true,
				description: "Trailing id from /user/profile/<author_id>."
			},
			num_notes: {
				type: "integer",
				description: "Notes/cards to collect. Maximum 50; default 10."
			},
			num_comments: {
				type: "integer",
				description: "Comments per opened note. Maximum 100; default 8."
			},
			preview: {
				type: "boolean",
				description: "Return author metadata and note cards without opening each note."
			},
			download_media: {
				type: "boolean",
				description: "Download opened-note media into the socai run directory."
			},
			ocr: {
				type: "boolean",
				description: "Run local OCR on note images or video covers."
			},
			transcribe_audio: {
				type: "boolean",
				description: "Transcribe video audio when the configured socai model supports it."
			}
		},
		output: OUTPUT,
		timeoutMs: resolved.timeoutMs,
		isConcurrencySafe: () => false,
		async execute(args, exec) {
			const authorId = nonEmpty(args.author_id, "author_id");
			count(args.num_notes, "num_notes", 1, 50);
			count(args.num_comments, "num_comments", 0, 100);
			const cliArgs = [
				"xhs",
				"author",
				authorId
			];
			pushNumber(cliArgs, "--num-notes", args.num_notes);
			pushNumber(cliArgs, "--num-comments", args.num_comments);
			pushFlag(cliArgs, "--preview", args.preview);
			pushFlag(cliArgs, "--download-media", args.download_media);
			pushFlag(cliArgs, "--ocr", args.ocr);
			pushFlag(cliArgs, "--transcribe-audio", args.transcribe_audio);
			return execute(cliArgs, exec.signal);
		}
	})), "dsh-socai.author");
	ctx.effect(() => ctx.tools.register(defineTool({
		name: "socai_xhs_get_notes",
		description: "Deep-read selected Xiaohongshu notes by note_id=xsec_token pairs returned from socai search/author. Returns bodies and comments without repeating broad discovery.",
		parameters: {
			notes: {
				type: "array",
				required: true,
				items: { type: "string" },
				description: "One or more NOTE_ID=XSEC_TOKEN pairs returned by search or author."
			},
			num_comments: {
				type: "integer",
				description: "Comments per note. Maximum 100; default 8."
			},
			download_media: {
				type: "boolean",
				description: "Download note media into the socai run directory."
			},
			ocr: {
				type: "boolean",
				description: "Run local OCR on note images or video covers."
			},
			transcribe_audio: {
				type: "boolean",
				description: "Transcribe video audio when the configured socai model supports it."
			}
		},
		output: OUTPUT,
		timeoutMs: resolved.timeoutMs,
		isConcurrencySafe: () => false,
		async execute(args, exec) {
			if (args.notes.length === 0 || args.notes.length > 20) throw new Error("notes must contain 1 to 20 NOTE_ID=XSEC_TOKEN values");
			count(args.num_comments, "num_comments", 0, 100);
			const cliArgs = ["xhs", "get-notes"];
			for (const note of args.notes) {
				const value = nonEmpty(note, "note");
				if (!value.includes("=")) throw new Error("each note must use NOTE_ID=XSEC_TOKEN format");
				cliArgs.push("--note", value);
			}
			pushNumber(cliArgs, "--num-comments", args.num_comments);
			pushFlag(cliArgs, "--download-media", args.download_media);
			pushFlag(cliArgs, "--ocr", args.ocr);
			pushFlag(cliArgs, "--transcribe-audio", args.transcribe_audio);
			return execute(cliArgs, exec.signal);
		}
	})), "dsh-socai.get-notes");
	ctx.effect(() => ctx.skills.registerProvider(() => socaiSkillProvider), "dsh-socai.skill");
	ctx.effect(() => ctx.systemPrompt.section({
		name: "tool:dsh-socai",
		order: 2150,
		text: PROMPT_TEXT
	}), "dsh-socai.prompt");
}
//#endregion
export { Config, apply, inject, name, runProcess, runSocai };
