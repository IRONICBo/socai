import { existsSync } from 'node:fs'
import { mkdir, rm, symlink } from 'node:fs/promises'
import { spawnSync } from 'node:child_process'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const checkout = process.env.DSH_CHECKOUT

if (checkout === undefined || !existsSync(join(checkout, 'packages')) || !existsSync(join(checkout, 'vendor'))) {
  console.error('build: set DSH_CHECKOUT to a built DeepSeek Harness source checkout')
  process.exit(1)
}

async function linkPackage(packageName, relativeTarget) {
  const target = join(checkout, relativeTarget)
  const link = join(root, 'node_modules', packageName)
  await mkdir(dirname(link), { recursive: true })
  await rm(link, { force: true, recursive: true })
  await symlink(target, link, process.platform === 'win32' ? 'junction' : 'dir')
}

await mkdir(join(root, 'node_modules', '@deepseek-ai'), { recursive: true })
await linkPackage('@types/node', 'node_modules/@types/node')
await linkPackage('@deepseek-ai/cordis', 'vendor/cordis')
await linkPackage('@deepseek-ai/cosmokit', 'vendor/cosmokit')
await linkPackage('@deepseek-ai/schemastery', 'vendor/schemastery')
await linkPackage('@deepseek-ai/dsh-llm', 'packages/llm/llm')
await linkPackage('@deepseek-ai/dsh-scope', 'packages/core/scope')
await linkPackage('@deepseek-ai/dsh-session', 'packages/core/session')
await linkPackage('@deepseek-ai/dsh-skill', 'packages/skill/skill')
await linkPackage('@deepseek-ai/dsh-system-prompt', 'packages/core/system-prompt')
await linkPackage('@deepseek-ai/dsh-tools', 'packages/core/tools')

const bin = command => join(checkout, 'node_modules', '.bin', `${command}${process.platform === 'win32' ? '.cmd' : ''}`)
for (const [command, args] of [['tsdown', []], ['tsc', ['-p', 'tsconfig.json']]]) {
  const result = spawnSync(bin(command), args, { cwd: root, stdio: 'inherit', shell: false })
  if (result.error !== undefined) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}
