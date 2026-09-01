import { existsSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const checkout = process.env.DSH_CHECKOUT
const executable = name => join(checkout ?? '', 'node_modules', '.bin', `${name}${process.platform === 'win32' ? '.cmd' : ''}`)

if (checkout === undefined || !existsSync(executable('vitest'))) {
  console.error('test: set DSH_CHECKOUT to an installed DeepSeek Harness source checkout')
  process.exit(1)
}

for (const [command, args, env] of [
  [process.execPath, [join(root, 'scripts', 'build.mjs')], process.env],
  [executable('vitest'), ['run'], { ...process.env, VITEST_MAX_WORKERS: '4' }],
]) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', env, shell: false })
  if (result.error !== undefined) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}
