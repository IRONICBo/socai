import { spawn } from 'node:child_process'
import { homedir } from 'node:os'

export interface ProcessOptions {
  signal?: AbortSignal
  timeoutMs: number
  maxOutputBytes: number
  installUrl?: string
  /** Test/embedding override; production calls use the bounded default. */
  terminationGraceMs?: number
}

export interface ProcessResult {
  code: number
  stdout: string
  stderr: string
}

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue }

export interface SocaiResult {
  data: JsonValue
  runDir?: string
}

const INSTALL_URL = 'https://socai.io/?utm_source=deepseek-harness&utm_medium=plugin&utm_campaign=dsh-socai'
const TERMINATION_GRACE_MS = 1_000
const FORCE_SETTLE_GRACE_MS = 1_000
const TASKKILL_WAIT_MS = 1_000

function shortOutput(value: string, limit = 2_000): string {
  const trimmed = value.trim()
  return trimmed.length <= limit ? trimmed : `${trimmed.slice(0, limit)}…`
}

function redactLocalPaths(value: string): string {
  let redacted = value.replace(/^run_dir:\s*.+$/gm, 'run_dir: [redacted]')
  const home = homedir()
  if (home !== '') redacted = redacted.split(home).join('[home]')
  return redacted
}

function taskkillProcessTree(child: ReturnType<typeof spawn>, force: boolean): Promise<void> {
  const pid = child.pid
  if (pid === undefined) {
    child.kill(force ? 'SIGKILL' : 'SIGTERM')
    return Promise.resolve()
  }

  return new Promise((resolve) => {
    let settled = false
    const killer = spawn('taskkill', [
      '/pid', String(pid), '/T', ...(force ? ['/F'] : []),
    ], {
      stdio: 'ignore',
      windowsHide: true,
    })
    const finish = (fallback: boolean): void => {
      if (settled) return
      settled = true
      clearTimeout(wait)
      if (fallback && force) child.kill('SIGKILL')
      resolve()
    }
    const wait = setTimeout(() => {
      killer.kill('SIGKILL')
      finish(true)
    }, TASKKILL_WAIT_MS)
    killer.once('error', () => finish(true))
    killer.once('close', code => finish(code !== 0))
  })
}

function signalProcessTree(child: ReturnType<typeof spawn>, force: boolean): Promise<void> {
  const pid = child.pid
  if (!force && (child.exitCode !== null || child.signalCode !== null)) return Promise.resolve()
  if (process.platform === 'win32') {
    return taskkillProcessTree(child, force)
  }

  if (pid !== undefined) {
    try {
      process.kill(-pid, force ? 'SIGKILL' : 'SIGTERM')
      return Promise.resolve()
    } catch {
      // Fall back to signalling the direct child if its process group vanished.
    }
  }
  child.kill(force ? 'SIGKILL' : 'SIGTERM')
  return Promise.resolve()
}

/** Run an executable directly with an argv array; no shell is involved. */
export function runProcess(
  executable: string,
  args: readonly string[],
  options: ProcessOptions,
): Promise<ProcessResult> {
  if (options.signal?.aborted) {
    return Promise.reject(new Error('socai command was cancelled'))
  }

  return new Promise((resolve, reject) => {
    let settled = false
    const stdoutChunks: Buffer[] = []
    const stderrChunks: Buffer[] = []
    let stdoutBytes = 0
    let stderrBytes = 0
    let outputBytes = 0
    let failure: Error | undefined
    let forceTimer: ReturnType<typeof setTimeout> | undefined
    let forceSettleTimer: ReturnType<typeof setTimeout> | undefined

    const child = spawn(executable, [...args], {
      stdio: ['ignore', 'pipe', 'pipe'],
      windowsHide: true,
      detached: process.platform !== 'win32',
    })

    const finish = (callback: () => void): void => {
      if (settled) return
      settled = true
      clearTimeout(timeout)
      clearTimeout(forceTimer)
      clearTimeout(forceSettleTimer)
      options.signal?.removeEventListener('abort', onAbort)
      callback()
    }

    const failAndStop = (error: Error): void => {
      if (failure !== undefined) return
      failure = error
      void signalProcessTree(child, false)
      forceTimer = setTimeout(() => {
        void signalProcessTree(child, true).then(() => {
          forceSettleTimer = setTimeout(() => {
            child.stdout.destroy()
            child.stderr.destroy()
            finish(() => reject(failure ?? error))
          }, FORCE_SETTLE_GRACE_MS)
        })
      }, options.terminationGraceMs ?? TERMINATION_GRACE_MS)
    }

    const capture = (stream: 'stdout' | 'stderr', chunk: Buffer): void => {
      outputBytes += chunk.byteLength
      if (outputBytes > options.maxOutputBytes) {
        failAndStop(new Error(
          `socai output exceeded ${options.maxOutputBytes} bytes; retry with fewer notes/comments or preview=true`,
        ))
        return
      }
      if (stream === 'stdout') {
        stdoutChunks.push(chunk)
        stdoutBytes += chunk.byteLength
      } else {
        stderrChunks.push(chunk)
        stderrBytes += chunk.byteLength
      }
    }

    const onAbort = (): void => failAndStop(new Error('socai command was cancelled'))
    const timeout = setTimeout(() => {
      failAndStop(new Error(`socai command timed out after ${options.timeoutMs} ms`))
    }, options.timeoutMs)
    timeout.unref()

    child.stdout.on('data', (chunk: Buffer) => capture('stdout', chunk))
    child.stderr.on('data', (chunk: Buffer) => capture('stderr', chunk))
    child.on('error', (error: NodeJS.ErrnoException) => {
      const message = error.code === 'ENOENT'
        ? `socai CLI was not found. Configure binaryPath or install it from ${options.installUrl ?? INSTALL_URL}, then restart DSH.`
        : `failed to start socai CLI (${error.code ?? 'spawn error'})`
      finish(() => reject(new Error(message)))
    })
    child.on('close', (code) => {
      finish(() => {
        if (failure !== undefined) {
          reject(failure)
          return
        }
        resolve({
          code: code ?? -1,
          stdout: Buffer.concat(stdoutChunks, stdoutBytes).toString('utf8'),
          stderr: Buffer.concat(stderrChunks, stderrBytes).toString('utf8'),
        })
      })
    })

    options.signal?.addEventListener('abort', onAbort, { once: true })
  })
}

function extractRunDir(stderr: string): string | undefined {
  const matches = [...stderr.matchAll(/^run_dir:\s*(.+)$/gm)]
  return matches.at(-1)?.[1]?.trim() || undefined
}

/** Execute one socai CLI command and validate its machine-readable JSON output. */
export async function runSocai(
  executable: string,
  args: readonly string[],
  options: ProcessOptions,
): Promise<SocaiResult> {
  const result = await runProcess(executable, args, options)
  if (result.code !== 0) {
    const detail = shortOutput(redactLocalPaths(result.stderr))
      || shortOutput(redactLocalPaths(result.stdout))
      || `exit code ${result.code}`
    throw new Error(`socai command failed: ${detail}`)
  }

  try {
    return {
      data: JSON.parse(result.stdout) as JsonValue,
      runDir: extractRunDir(result.stderr),
    }
  } catch {
    throw new Error(
      `socai command returned invalid JSON: ${shortOutput(redactLocalPaths(result.stdout)) || '(empty stdout)'}`,
    )
  }
}
