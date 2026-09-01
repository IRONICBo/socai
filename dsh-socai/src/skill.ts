import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import {
  BUNDLED_SKILL_RANK,
  type SkillCandidate,
  type SkillDefinition,
  type SkillProvider,
} from '@deepseek-ai/dsh-skill'

const PROVIDER_NAME = 'dsh-socai'
const RESOURCE_BASE = {
  kind: 'directory',
  path: fileURLToPath(new URL('../assets/', import.meta.url)),
} as const
const INVOCATION = { modelInvocable: true, userInvocable: true } as const

const CANDIDATE: SkillCandidate = {
  name: 'socai-xhs-research',
  description:
    'Research Xiaohongshu with socai: start with a broad preview, select useful notes, ' +
    'then deep-read bodies/comments or scan an author. Load when the user asks for ' +
    'Xiaohongshu topic, audience, content, or competitor research.',
  invocation: INVOCATION,
  provider: PROVIDER_NAME,
  source: 'bundled',
  resourceBase: RESOURCE_BASE,
  rank: BUNDLED_SKILL_RANK,
  locator: new URL('../assets/socai-xhs-research.md', import.meta.url),
}

export const socaiSkillProvider: SkillProvider = {
  name: PROVIDER_NAME,
  list: () => Promise.resolve([CANDIDATE]),
  async get(candidate): Promise<SkillDefinition> {
    if (candidate.name !== CANDIDATE.name) {
      throw new Error(`unknown dsh-socai skill: ${candidate.name}`)
    }
    return {
      name: CANDIDATE.name,
      description: CANDIDATE.description,
      invocation: CANDIDATE.invocation,
      provider: CANDIDATE.provider,
      source: CANDIDATE.source,
      resourceBase: RESOURCE_BASE,
      content: await readFile(CANDIDATE.locator as URL, 'utf8'),
    }
  },
}
