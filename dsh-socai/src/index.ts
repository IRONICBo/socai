import type { Context as CordisContext } from '@deepseek-ai/cordis'
import type SkillService from '@deepseek-ai/dsh-skill'
import type SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import type ToolRegistry from '@deepseek-ai/dsh-tools'
import { defineTool } from '@deepseek-ai/dsh-tools'
import z from '@deepseek-ai/schemastery'
import { createHash } from 'node:crypto'
import { runProcess, runSocai, type SocaiResult } from './runner.js'
import { socaiSkillProvider } from './skill.js'

type Context = CordisContext & {
  tools: ToolRegistry
  systemPrompt: SystemPrompt
  skills: SkillService
}

export const name = 'dsh-socai'
export const inject = ['tools', 'systemPrompt', 'skills']

const DEFAULT_PRODUCT_URL =
  'https://socai.io/?utm_source=deepseek-harness&utm_medium=plugin&utm_campaign=dsh-socai'
const MINIMUM_SOCAI_VERSION = '0.4.12'
const DEFAULT_OUTPUT_BYTES = 64_000

export interface Config {
  binaryPath?: string
  timeoutMs?: number
  maxOutputBytes?: number
  productUrl?: string
}

export const Config: z<Config> = z.object({
  binaryPath: z.string().default('socai')
    .description('socai CLI executable name or absolute path.'),
  timeoutMs: z.number().step(1).min(1_000).default(900_000)
    .description('Maximum runtime for one socai browser command in milliseconds.'),
  maxOutputBytes: z.number().step(1).min(10_000).max(256_000).default(DEFAULT_OUTPUT_BYTES)
    .description('Byte cap for child output and the final model-visible result. Lower note counts or use preview mode if exceeded.'),
  productUrl: z.string().default(DEFAULT_PRODUCT_URL)
    .description('Product/install link included in successful tool results and missing-CLI guidance.'),
})

const PROMPT_TEXT = `## Xiaohongshu research with socai
Use the socai_xhs_* tools when a user asks to research Xiaohongshu topics, posts, audiences, or authors. For broad discovery, start with socai_xhs_search and preview=true; deep-read only the selected notes with socai_xhs_get_notes, or set preview=false for a small focused scan. Use socai_xhs_author for creator/competitor research. These tools operate the user's local logged-in Chrome through the socai CLI, so do not run them in parallel. Load the socai-xhs-research skill for multi-step research.`

const OUTPUT_SCHEMA = {
  type: 'object' as const,
  additionalProperties: false,
  properties: {
    data: { type: 'json' as const, required: true },
    run_id: { type: 'string' as const },
    product_url: { type: 'string' as const, required: true },
  },
} as const

function output(result: SocaiResult, productUrl: string, maxOutputBytes: number) {
  const runId = result.runDir === undefined
    ? undefined
    : createHash('sha256').update(result.runDir).digest('hex').slice(0, 16)
  const value = {
    data: result.data,
    ...(runId === undefined || runId === '' ? {} : { run_id: runId }),
    product_url: productUrl,
  }
  const renderedBytes = Buffer.byteLength(JSON.stringify(value, null, 2))
  if (renderedBytes > maxOutputBytes) {
    throw new Error(
      `socai result would expose ${renderedBytes} bytes to the model, above the ${maxOutputBytes}-byte limit; retry with fewer notes/comments or preview=true`,
    )
  }
  return value
}

const OUTPUT = {
  schema: OUTPUT_SCHEMA,
  render: (_args: unknown, value: unknown) => [{
    type: 'text' as const,
    text: JSON.stringify(value, null, 2),
  }],
}

function count(value: number | undefined, name: string, minimum: number, maximum: number): void {
  if (value === undefined) return
  if (!Number.isInteger(value) || value < minimum || value > maximum) {
    throw new Error(`${name} must be an integer from ${minimum} to ${maximum}`)
  }
}

function nonEmpty(value: string, name: string): string {
  const trimmed = value.trim()
  if (trimmed === '') throw new Error(`${name} must not be empty`)
  return trimmed
}

function pushNumber(args: string[], flag: string, value: number | undefined): void {
  if (value !== undefined) args.push(flag, String(value))
}

function pushFlag(args: string[], flag: string, value: boolean | undefined): void {
  if (value === true) args.push(flag)
}

interface ParsedVersion {
  core: [number, number, number]
  prerelease?: string
}

function parseVersion(value: string): ParsedVersion | undefined {
  const match = value.match(/\bsocai\s+v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?\b/i)
  if (match === null) return undefined
  return {
    core: [Number(match[1]), Number(match[2]), Number(match[3])],
    ...(match[4] === undefined ? {} : { prerelease: match[4] }),
  }
}

function isVersionAtLeast(actual: ParsedVersion, minimum: ParsedVersion): boolean {
  for (let index = 0; index < actual.core.length; index += 1) {
    if (actual.core[index] !== minimum.core[index]) {
      return actual.core[index] > minimum.core[index]
    }
  }
  if (minimum.prerelease === undefined) return actual.prerelease === undefined
  if (actual.prerelease === undefined) return true
  return actual.prerelease.localeCompare(minimum.prerelease, 'en', { numeric: true }) >= 0
}

function displayVersion(version: ParsedVersion): string {
  return `${version.core.join('.')}${version.prerelease === undefined ? '' : `-${version.prerelease}`}`
}

class SerialQueue {
  private locked = false
  private readonly waiters: Array<{
    signal: AbortSignal
    onAbort: () => void
    resolve: (release: () => void) => void
    reject: (error: Error) => void
  }> = []

  async run<T>(signal: AbortSignal, task: () => Promise<T>): Promise<T> {
    const release = await this.acquire(signal)
    try {
      return await task()
    } finally {
      release()
    }
  }

  private acquire(signal: AbortSignal): Promise<() => void> {
    if (signal.aborted) return Promise.reject(new Error('socai command was cancelled'))
    if (!this.locked) {
      this.locked = true
      return Promise.resolve(this.makeRelease())
    }

    return new Promise((resolve, reject) => {
      const waiter = {
        signal,
        resolve,
        reject,
        onAbort: () => {
          const index = this.waiters.indexOf(waiter)
          if (index !== -1) this.waiters.splice(index, 1)
          reject(new Error('socai command was cancelled while waiting for the browser'))
        },
      }
      this.waiters.push(waiter)
      signal.addEventListener('abort', waiter.onAbort, { once: true })
      if (signal.aborted) waiter.onAbort()
    })
  }

  private makeRelease(): () => void {
    let released = false
    return () => {
      if (released) return
      released = true
      const next = this.waiters.shift()
      if (next === undefined) {
        this.locked = false
        return
      }
      next.signal.removeEventListener('abort', next.onAbort)
      next.resolve(this.makeRelease())
    }
  }
}

export function apply(ctx: Context, config: Config): void {
  const resolved = {
    binaryPath: config.binaryPath ?? 'socai',
    timeoutMs: config.timeoutMs ?? 900_000,
    maxOutputBytes: config.maxOutputBytes ?? DEFAULT_OUTPUT_BYTES,
    productUrl: config.productUrl ?? DEFAULT_PRODUCT_URL,
  }
  const queue = new SerialQueue()
  let compatibleBinary: string | undefined

  const ensureCompatible = async (signal: AbortSignal): Promise<void> => {
    if (compatibleBinary === resolved.binaryPath) return
    const version = await runProcess(resolved.binaryPath, ['--version'], {
      signal,
      timeoutMs: Math.min(resolved.timeoutMs, 10_000),
      maxOutputBytes: 10_000,
      installUrl: resolved.productUrl,
    })
    const parsed = parseVersion(`${version.stdout}\n${version.stderr}`)
    const minimum = parseVersion(`socai ${MINIMUM_SOCAI_VERSION}`)
    if (version.code !== 0 || parsed === undefined || minimum === undefined) {
      throw new Error(
        `could not verify the socai CLI version; dsh-socai requires socai >= ${MINIMUM_SOCAI_VERSION}. Upgrade from ${resolved.productUrl}`,
      )
    }
    if (!isVersionAtLeast(parsed, minimum)) {
      throw new Error(
        `socai ${displayVersion(parsed)} is too old; dsh-socai requires socai >= ${MINIMUM_SOCAI_VERSION}. Upgrade from ${resolved.productUrl}`,
      )
    }
    compatibleBinary = resolved.binaryPath
  }

  const execute = (args: string[], signal: AbortSignal) => queue.run(signal, async () => {
    await ensureCompatible(signal)
    return output(
      await runSocai(resolved.binaryPath, args, {
        signal,
        timeoutMs: resolved.timeoutMs,
        maxOutputBytes: resolved.maxOutputBytes,
        installUrl: resolved.productUrl,
      }),
      resolved.productUrl,
      resolved.maxOutputBytes,
    )
  })

  ctx.effect(() => ctx.tools.register(defineTool({
    name: 'socai_xhs_search',
    description:
      'Search Xiaohongshu with the local socai browser agent. Preview result cards for broad discovery, or open a focused set to return bodies and top comments. Requires the socai CLI and a usable Chrome profile.',
    parameters: {
      query: { type: 'string', required: true, description: 'Xiaohongshu search query.' },
      filters: {
        type: 'array',
        items: { type: 'string' },
        description: 'Optional group=option filters, e.g. publish_time=一周内, note_type=图文, sort=最新.',
      },
      num_notes: { type: 'integer', description: 'Notes/cards to collect. Maximum 50; default 10.' },
      num_comments: { type: 'integer', description: 'Comments per opened note. Maximum 100; default 8.' },
      preview: { type: 'boolean', description: 'Return cards only without opening every note.' },
      download_media: { type: 'boolean', description: 'Download note media into the socai run directory.' },
      ocr: { type: 'boolean', description: 'Run local OCR on note images or video covers.' },
      transcribe_audio: { type: 'boolean', description: 'Transcribe video audio when the configured socai model supports it.' },
    },
    output: OUTPUT,
    timeoutMs: resolved.timeoutMs,
    isConcurrencySafe: () => false,
    async execute(args, exec) {
      const query = nonEmpty(args.query, 'query')
      count(args.num_notes, 'num_notes', 1, 50)
      count(args.num_comments, 'num_comments', 0, 100)
      const cliArgs = ['xhs', 'search', query]
      for (const filter of args.filters ?? []) cliArgs.push('--filter', nonEmpty(filter, 'filter'))
      pushNumber(cliArgs, '--num-notes', args.num_notes)
      pushNumber(cliArgs, '--num-comments', args.num_comments)
      pushFlag(cliArgs, '--preview', args.preview)
      pushFlag(cliArgs, '--download-media', args.download_media)
      pushFlag(cliArgs, '--ocr', args.ocr)
      pushFlag(cliArgs, '--transcribe-audio', args.transcribe_audio)
      return execute(cliArgs, exec.signal)
    },
  })), 'dsh-socai.search')

  ctx.effect(() => ctx.tools.register(defineTool({
    name: 'socai_xhs_author',
    description:
      'Scan one Xiaohongshu author with the local socai browser agent: profile metadata plus note cards, bodies, and comments. Requires the socai CLI and a usable Chrome profile.',
    parameters: {
      author_id: { type: 'string', required: true, description: 'Trailing id from /user/profile/<author_id>.' },
      num_notes: { type: 'integer', description: 'Notes/cards to collect. Maximum 50; default 10.' },
      num_comments: { type: 'integer', description: 'Comments per opened note. Maximum 100; default 8.' },
      preview: { type: 'boolean', description: 'Return author metadata and note cards without opening each note.' },
      download_media: { type: 'boolean', description: 'Download opened-note media into the socai run directory.' },
      ocr: { type: 'boolean', description: 'Run local OCR on note images or video covers.' },
      transcribe_audio: { type: 'boolean', description: 'Transcribe video audio when the configured socai model supports it.' },
    },
    output: OUTPUT,
    timeoutMs: resolved.timeoutMs,
    isConcurrencySafe: () => false,
    async execute(args, exec) {
      const authorId = nonEmpty(args.author_id, 'author_id')
      count(args.num_notes, 'num_notes', 1, 50)
      count(args.num_comments, 'num_comments', 0, 100)
      const cliArgs = ['xhs', 'author', authorId]
      pushNumber(cliArgs, '--num-notes', args.num_notes)
      pushNumber(cliArgs, '--num-comments', args.num_comments)
      pushFlag(cliArgs, '--preview', args.preview)
      pushFlag(cliArgs, '--download-media', args.download_media)
      pushFlag(cliArgs, '--ocr', args.ocr)
      pushFlag(cliArgs, '--transcribe-audio', args.transcribe_audio)
      return execute(cliArgs, exec.signal)
    },
  })), 'dsh-socai.author')

  ctx.effect(() => ctx.tools.register(defineTool({
    name: 'socai_xhs_get_notes',
    description:
      'Deep-read selected Xiaohongshu notes by note_id=xsec_token pairs returned from socai search/author. Returns bodies and comments without repeating broad discovery.',
    parameters: {
      notes: {
        type: 'array',
        required: true,
        items: { type: 'string' },
        description: 'One or more NOTE_ID=XSEC_TOKEN pairs returned by search or author.',
      },
      num_comments: { type: 'integer', description: 'Comments per note. Maximum 100; default 8.' },
      download_media: { type: 'boolean', description: 'Download note media into the socai run directory.' },
      ocr: { type: 'boolean', description: 'Run local OCR on note images or video covers.' },
      transcribe_audio: { type: 'boolean', description: 'Transcribe video audio when the configured socai model supports it.' },
    },
    output: OUTPUT,
    timeoutMs: resolved.timeoutMs,
    isConcurrencySafe: () => false,
    async execute(args, exec) {
      if (args.notes.length === 0 || args.notes.length > 20) {
        throw new Error('notes must contain 1 to 20 NOTE_ID=XSEC_TOKEN values')
      }
      count(args.num_comments, 'num_comments', 0, 100)
      const cliArgs = ['xhs', 'get-notes']
      for (const note of args.notes) {
        const value = nonEmpty(note, 'note')
        if (!value.includes('=')) throw new Error('each note must use NOTE_ID=XSEC_TOKEN format')
        cliArgs.push('--note', value)
      }
      pushNumber(cliArgs, '--num-comments', args.num_comments)
      pushFlag(cliArgs, '--download-media', args.download_media)
      pushFlag(cliArgs, '--ocr', args.ocr)
      pushFlag(cliArgs, '--transcribe-audio', args.transcribe_audio)
      return execute(cliArgs, exec.signal)
    },
  })), 'dsh-socai.get-notes')

  ctx.effect(() => ctx.skills.registerProvider(() => socaiSkillProvider), 'dsh-socai.skill')
  ctx.effect(() => ctx.systemPrompt.section({
    name: 'tool:dsh-socai',
    order: 2150,
    text: PROMPT_TEXT,
  }), 'dsh-socai.prompt')
}

export { runProcess, runSocai } from './runner.js'
