import { afterEach, describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import { access, chmod, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createHash } from 'node:crypto'
import { createUserMessage } from '@deepseek-ai/dsh-llm'
import SessionStore, { SessionId } from '@deepseek-ai/dsh-session'
import SkillService from '@deepseek-ai/dsh-skill'
import SystemPrompt from '@deepseek-ai/dsh-system-prompt'
import ToolRegistry, { type ToolExecutionResult } from '@deepseek-ai/dsh-tools'
import * as DshSocai from '../src/index.ts'

const activeContexts: Context[] = []
const temporaryDirectories: string[] = []
let calls = 0

afterEach(async () => {
  for (const ctx of activeContexts.splice(0)) await ctx.fiber.dispose()
  for (const path of temporaryDirectories.splice(0)) await rm(path, { recursive: true, force: true })
})

async function setup(config: DshSocai.Config = {}): Promise<Context> {
  const ctx = new Context()
  activeContexts.push(ctx)
  await ctx.plugin(SessionStore)
  await ctx.plugin(SystemPrompt)
  await ctx.plugin(SkillService)
  await ctx.plugin(ToolRegistry)
  await ctx.plugin(DshSocai, config)
  return ctx
}

async function callTool(ctx: Context, name: string, args: unknown): Promise<ToolExecutionResult> {
  const caller = ctx.sessions.create(SessionId(`caller-${++calls}`), { meta: { createdAt: 1, cwd: '/work' } })
  caller.append('turn/start', { turn: 1, trigger: { kind: 'message', source: { kind: 'user' } } })
  caller.append('user/message', createUserMessage({
    content: [{ type: 'text', text: 'go' }],
    source: { kind: 'user' },
  }), { surfaceOp: 'append' })
  return ctx.tools.execute({
    name,
    arguments: args,
    // The brand was renamed from CallId to ToolCallId after rc.2; both are
    // represented by a plain string at runtime.
    callId: `call-${++calls}` as never,
    signal: new AbortController().signal,
    agent: { id: caller.id, session: caller } as never,
  })
}

function text(result: ToolExecutionResult): string {
  return result.content.map(block => block.type === 'text' ? block.text : '').join('\n')
}

async function fakeSocai(options: {
  version?: string
  commandSource?: string
} = {}): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), 'dsh-socai-test-'))
  temporaryDirectories.push(directory)
  const executable = join(directory, 'socai')
  await writeFile(executable, [
    '#!/usr/bin/env node',
    `if (process.argv.includes('--version')) { process.stdout.write('socai ${options.version ?? '0.5.5'}\\n'); process.exit(0) }`,
    "process.stderr.write('run_dir: /tmp/fake-socai-run\\n')",
    options.commandSource ?? 'process.stdout.write(JSON.stringify({ argv: process.argv.slice(2) }))',
  ].join('\n'))
  await chmod(executable, 0o755)
  return executable
}

describe.skipIf(process.platform === 'win32')('dsh-socai plugin', () => {
  it('registers the three typed tools', async () => {
    const ctx = await setup()
    expect(ctx.tools.get('socai_xhs_search')).toBeDefined()
    expect(ctx.tools.get('socai_xhs_author')).toBeDefined()
    expect(ctx.tools.get('socai_xhs_get_notes')).toBeDefined()
  })

  it('contributes model guidance and the bundled skill', async () => {
    const ctx = await setup()
    const assembly = await ctx.systemPrompt.assemble({ cwd: '/work' } as never)
    expect(assembly.sections.find(section => section.name === 'tool:dsh-socai')?.text)
      .toMatch(/socai_xhs_search/)
    const skills = await ctx.skills.list()
    expect(skills.map(skill => skill.name)).toContain('socai-xhs-research')
  })

  it('executes search through an argv array and preserves structured attribution', async () => {
    const binaryPath = await fakeSocai()
    const ctx = await setup({ binaryPath, productUrl: 'https://socai.example/dsh' })
    const result = await callTool(ctx, 'socai_xhs_search', {
      query: '咖啡 $(touch /tmp/must-not-run)',
      filters: ['publish_time=一周内'],
      num_notes: 12,
      preview: true,
    })
    expect(result.isError).toBeFalsy()
    expect(JSON.parse(text(result))).toEqual({
      data: {
        argv: [
          'xhs', 'search', '咖啡 $(touch /tmp/must-not-run)',
          '--filter', 'publish_time=一周内', '--num-notes', '12', '--preview',
        ],
      },
      run_id: createHash('sha256').update('/tmp/fake-socai-run').digest('hex').slice(0, 16),
      product_url: 'https://socai.example/dsh',
    })
  })

  it('rejects a SocAI release below the first complete CLI contract', async () => {
    const binaryPath = await fakeSocai({ version: '0.4.11' })
    const ctx = await setup({ binaryPath, productUrl: 'https://socai.example/upgrade' })
    const result = await callTool(ctx, 'socai_xhs_search', { query: '咖啡', preview: true })
    expect(result.isError).toBeTruthy()
    expect(text(result)).toMatch(/requires socai >= 0\.4\.12/)
    expect(text(result)).toMatch(/socai\.example\/upgrade/)
  })

  it('does not treat a minimum-version prerelease as the stable release', async () => {
    const binaryPath = await fakeSocai({ version: '0.4.12-alpha.1' })
    const ctx = await setup({ binaryPath })
    const result = await callTool(ctx, 'socai_xhs_search', { query: '咖啡', preview: true })
    expect(result.isError).toBeTruthy()
    expect(text(result)).toMatch(/socai 0\.4\.12-alpha\.1 is too old/)
  })

  it('rejects zero notes while retaining zero as a valid comment count', async () => {
    const binaryPath = await fakeSocai()
    const ctx = await setup({ binaryPath })
    const invalid = await callTool(ctx, 'socai_xhs_search', { query: '咖啡', num_notes: 0 })
    expect(invalid.isError).toBeTruthy()
    expect(text(invalid)).toMatch(/num_notes must be an integer from 1 to 50/)

    const valid = await callTool(ctx, 'socai_xhs_author', {
      author_id: 'author-1',
      num_notes: 1,
      num_comments: 0,
      preview: true,
    })
    expect(valid.isError).toBeFalsy()
  })

  it('serializes SocAI work across independent DSH sessions', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'dsh-socai-lock-test-'))
    temporaryDirectories.push(directory)
    const lockPath = join(directory, 'active')
    const overlapPath = join(directory, 'overlap')
    const binaryPath = await fakeSocai({
      commandSource: [
        "const fs = require('node:fs')",
        `const lock = ${JSON.stringify(lockPath)}`,
        `const overlap = ${JSON.stringify(overlapPath)}`,
        "if (fs.existsSync(lock)) fs.writeFileSync(overlap, 'overlap')",
        "fs.writeFileSync(lock, 'active')",
        "setTimeout(() => { fs.rmSync(lock, { force: true }); process.stdout.write(JSON.stringify({ ok: true })) }, 80)",
      ].join(';'),
    })
    const ctx = await setup({ binaryPath })
    const [first, second] = await Promise.all([
      callTool(ctx, 'socai_xhs_search', { query: 'first', preview: true }),
      callTool(ctx, 'socai_xhs_search', { query: 'second', preview: true }),
    ])
    expect(first.isError).toBeFalsy()
    expect(second.isError).toBeFalsy()
    await expect(access(overlapPath)).rejects.toThrow()
  })

  it('guards the final pretty-printed model result, not only raw stdout', async () => {
    const binaryPath = await fakeSocai({
      commandSource: "process.stdout.write(JSON.stringify(Array.from({ length: 4000 }, () => 1)))",
    })
    const ctx = await setup({ binaryPath, maxOutputBytes: 10_000 })
    const result = await callTool(ctx, 'socai_xhs_search', { query: 'large', preview: true })
    expect(result.isError).toBeTruthy()
    expect(text(result)).toMatch(/above the 10000-byte limit/)
  })
})
