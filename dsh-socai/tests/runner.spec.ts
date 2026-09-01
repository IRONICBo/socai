import { describe, expect, it } from 'vitest'
import { join } from 'node:path'
import { homedir, tmpdir } from 'node:os'
import { access, mkdtemp, rm } from 'node:fs/promises'
import { runProcess, runSocai } from '../src/runner.ts'

const options = { timeoutMs: 5_000, maxOutputBytes: 10_000 }

describe('runProcess', () => {
  it('passes argv without invoking a shell', async () => {
    const payload = '$(printf injected)'
    const result = await runProcess(process.execPath, [
      '-e',
      'process.stdout.write(JSON.stringify(process.argv.slice(1)))',
      payload,
    ], options)
    expect(result.code).toBe(0)
    expect(JSON.parse(result.stdout)).toEqual([payload])
  })

  it('caps combined output', async () => {
    await expect(runProcess(
      process.execPath,
      ['-e', 'process.stdout.write("x".repeat(20000))'],
      options,
    )).rejects.toThrow(/output exceeded/)
  })

  it('preserves UTF-8 characters split across pipe chunks', async () => {
    const source = [
      'const body = Buffer.from(JSON.stringify({ text: "汉" }))',
      'const split = body.indexOf(Buffer.from("汉")) + 1',
      'process.stdout.write(body.subarray(0, split))',
      'setTimeout(() => process.stdout.write(body.subarray(split)), 20)',
    ].join(';')
    const result = await runSocai(process.execPath, ['-e', source], options)
    expect(result.data).toEqual({ text: '汉' })
  })

  it.runIf(process.platform !== 'win32')('force-kills a child that ignores graceful termination', async () => {
    const started = Date.now()
    await expect(runProcess(process.execPath, [
      '-e',
      'process.on("SIGTERM", () => {}); setInterval(() => {}, 1000)',
    ], {
      timeoutMs: 20,
      maxOutputBytes: 10_000,
      terminationGraceMs: 50,
    })).rejects.toThrow(/timed out/)
    expect(Date.now() - started).toBeLessThan(1_000)
  })

  it.runIf(process.platform === 'win32')('terminates a spawned Windows process tree', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'dsh-socai-tree-'))
    const marker = join(directory, 'orphan-ran')
    const descendant = `setTimeout(() => require('node:fs').writeFileSync(${JSON.stringify(marker)}, 'alive'), 500)`
    const parent = [
      "const { spawn } = require('node:child_process')",
      `spawn(process.execPath, ['-e', ${JSON.stringify(descendant)}], { stdio: 'ignore' })`,
      'setInterval(() => {}, 1000)',
    ].join(';')
    try {
      await expect(runProcess(process.execPath, ['-e', parent], {
        timeoutMs: 20,
        maxOutputBytes: 10_000,
        terminationGraceMs: 50,
      })).rejects.toThrow(/timed out/)
      await new Promise(resolve => setTimeout(resolve, 700))
      await expect(access(marker)).rejects.toThrow()
    } finally {
      await rm(directory, { recursive: true, force: true })
    }
  })

  it('does not spawn work for a pre-aborted call', async () => {
    const controller = new AbortController()
    controller.abort()
    await expect(runProcess(process.execPath, ['-e', 'process.exit(99)'], {
      ...options,
      signal: controller.signal,
    })).rejects.toThrow(/cancelled/)
  })

  it('uses the configured install URL when the executable is missing', async () => {
    const missing = join(tmpdir(), 'definitely-missing-socai')
    const message = await runProcess(missing, [], {
      ...options,
      installUrl: 'https://socai.example/install',
    }).then(() => '', error => String(error.message))
    expect(message).toMatch(/https:\/\/socai\.example\/install/)
    expect(message).not.toContain(missing)
  })
})

describe('runSocai', () => {
  it('parses stdout JSON and the run_dir marker from stderr', async () => {
    const result = await runSocai(process.execPath, [
      '-e',
      'process.stderr.write("run_dir: /tmp/socai-run\\n"); process.stdout.write(JSON.stringify({cards:[1]}))',
    ], options)
    expect(result).toEqual({ data: { cards: [1] }, runDir: '/tmp/socai-run' })
  })

  it('surfaces non-zero command failures', async () => {
    await expect(runSocai(process.execPath, [
      '-e',
      'process.stderr.write("login required"); process.exit(3)',
    ], options)).rejects.toThrow(/login required/)
  })

  it('redacts absolute run and home paths from command errors', async () => {
    const privatePath = join(homedir(), 'private-socai-data')
    const stderr = `run_dir: ${privatePath}\nfailed while reading ${privatePath}/trace.json`
    const message = await runSocai(process.execPath, [
      '-e',
      `process.stderr.write(${JSON.stringify(stderr)}); process.exit(3)`,
    ], options).then(() => '', error => String(error.message))
    expect(message).toContain('run_dir: [redacted]')
    expect(message).toContain('[home]/private-socai-data/trace.json')
    expect(message).not.toContain(homedir())
  })

  it('rejects non-JSON stdout', async () => {
    await expect(runSocai(process.execPath, [
      '-e',
      'process.stdout.write("not json")',
    ], options)).rejects.toThrow(/invalid JSON/)
  })
})
