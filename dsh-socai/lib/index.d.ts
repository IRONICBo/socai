import ToolRegistry from "@deepseek-ai/dsh-tools";
import z from "@deepseek-ai/schemastery";
import SkillService from "@deepseek-ai/dsh-skill";
import { Context } from "@deepseek-ai/cordis";
import SystemPrompt from "@deepseek-ai/dsh-system-prompt";

//#region src/runner.d.ts
interface ProcessOptions {
  signal?: AbortSignal;
  timeoutMs: number;
  maxOutputBytes: number;
  installUrl?: string;
  /** Test/embedding override; production calls use the bounded default. */
  terminationGraceMs?: number;
}
interface ProcessResult {
  code: number;
  stdout: string;
  stderr: string;
}
type JsonValue = null | boolean | number | string | JsonValue[] | {
  [key: string]: JsonValue;
};
interface SocaiResult {
  data: JsonValue;
  runDir?: string;
}
/** Run an executable directly with an argv array; no shell is involved. */
declare function runProcess(executable: string, args: readonly string[], options: ProcessOptions): Promise<ProcessResult>;
/** Execute one socai CLI command and validate its machine-readable JSON output. */
declare function runSocai(executable: string, args: readonly string[], options: ProcessOptions): Promise<SocaiResult>;
//#endregion
//#region src/index.d.ts
type Context$1 = Context & {
  tools: ToolRegistry;
  systemPrompt: SystemPrompt;
  skills: SkillService;
};
declare const name = "dsh-socai";
declare const inject: string[];
interface Config {
  binaryPath?: string;
  timeoutMs?: number;
  maxOutputBytes?: number;
  productUrl?: string;
}
declare const Config: z<Config>;
declare function apply(ctx: Context$1, config: Config): void;
//#endregion
export { Config, apply, inject, name, runProcess, runSocai };