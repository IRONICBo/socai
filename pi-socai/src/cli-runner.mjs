import { spawn } from "node:child_process";

const DEFAULT_TIMEOUT_MS = 16 * 60 * 1_000;
const MAX_CAPTURE_BYTES = 20 * 1024 * 1024;

export class SocaiCliError extends Error {
  constructor(message, options = {}) {
    super(message, options.cause ? { cause: options.cause } : undefined);
    this.name = "SocaiCliError";
    this.exitCode = options.exitCode;
    this.signal = options.signal;
    this.stderr = options.stderr ?? "";
  }
}

function abortError(signal) {
  const error = new Error(signal?.reason?.message ?? "socai CLI execution was cancelled");
  error.name = "AbortError";
  error.cause = signal?.reason;
  return error;
}

function diagnostic(stderr, fallback) {
  const text = stderr.trim();
  return text || fallback;
}

export async function runSocaiCli(args, options = {}) {
  if (!Array.isArray(args) || args.some((arg) => typeof arg !== "string")) {
    throw new TypeError("socai CLI arguments must be an array of strings");
  }

  const bin = options.bin ?? process.env.SOCAI_BIN ?? "socai";
  const prefixArgs = options.prefixArgs ?? [];
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const signal = options.signal;
  if (signal?.aborted) throw abortError(signal);

  return await new Promise((resolve, reject) => {
    let stdout = Buffer.alloc(0);
    let stderr = Buffer.alloc(0);
    let outputExceeded = false;
    let timedOut = false;

    const child = spawn(bin, [...prefixArgs, ...args], {
      env: { ...process.env, ...options.env },
      shell: false,
      stdio: ["ignore", "pipe", "pipe"],
    });

    const terminate = () => {
      if (!child.killed) child.kill("SIGTERM");
    };
    const onAbort = () => terminate();
    signal?.addEventListener("abort", onAbort, { once: true });

    const timer = setTimeout(() => {
      timedOut = true;
      terminate();
    }, timeoutMs);
    timer.unref?.();

    const append = (current, chunk) => {
      const next = Buffer.concat([current, chunk]);
      if (next.byteLength <= MAX_CAPTURE_BYTES) return next;
      outputExceeded = true;
      terminate();
      return next.subarray(0, MAX_CAPTURE_BYTES);
    };

    child.stdout.on("data", (chunk) => {
      stdout = append(stdout, chunk);
    });
    child.stderr.on("data", (chunk) => {
      stderr = append(stderr, chunk);
    });

    child.once("error", (cause) => {
      clearTimeout(timer);
      signal?.removeEventListener("abort", onAbort);
      if (signal?.aborted) {
        reject(abortError(signal));
        return;
      }
      reject(new SocaiCliError(`failed to start socai CLI: ${cause.message}`, { cause }));
    });

    child.once("close", (exitCode, exitSignal) => {
      clearTimeout(timer);
      signal?.removeEventListener("abort", onAbort);
      if (signal?.aborted) {
        reject(abortError(signal));
        return;
      }

      const stdoutText = stdout.toString("utf8");
      const stderrText = stderr.toString("utf8");
      if (timedOut) {
        reject(new SocaiCliError(`socai CLI timed out after ${timeoutMs} ms`, {
          exitCode,
          signal: exitSignal,
          stderr: stderrText,
        }));
        return;
      }
      if (outputExceeded) {
        reject(new SocaiCliError(`socai CLI output exceeded ${MAX_CAPTURE_BYTES} bytes`, {
          exitCode,
          signal: exitSignal,
          stderr: stderrText,
        }));
        return;
      }
      if (exitCode !== 0) {
        reject(new SocaiCliError(
          `socai CLI exited with code ${exitCode}: ${diagnostic(stderrText, "no diagnostic output")}`,
          { exitCode, signal: exitSignal, stderr: stderrText },
        ));
        return;
      }

      try {
        resolve(JSON.parse(stdoutText));
      } catch (cause) {
        reject(new SocaiCliError(
          `socai CLI completed without valid JSON: ${stdoutText.trim().slice(0, 240) || "empty stdout"}`,
          { exitCode, signal: exitSignal, stderr: stderrText, cause },
        ));
      }
    });
  });
}
