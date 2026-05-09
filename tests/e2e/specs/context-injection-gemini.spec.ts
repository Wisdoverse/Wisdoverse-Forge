import { expect, test } from '@playwright/test'
import { spawn } from 'node:child_process'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.resolve(here, '../../..')
const rustRoot = path.join(repoRoot, 'rust')

test.describe('Gemini context injection E2E', () => {
  test.setTimeout(180_000)

  test('writes Gemini GEMINI.md and adapter report from a runtime envelope', async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), 'agentforge-gemini-context-e2e-'))
    const home = path.join(root, 'home')
    const envelopePath = path.join(root, 'envelope.json')
    const reportPath = path.join(root, 'report.json')

    try {
      await writeFile(envelopePath, JSON.stringify(envelope(), null, 2))

      await runContextHelper([
        '--adapter',
        'gemini',
        '--envelope',
        envelopePath,
        '--home',
        home,
        '--report',
        reportPath,
      ])

      const geminiMd = await readFile(path.join(home, '.gemini/GEMINI.md'), 'utf8')
      const state = await readFile(path.join(home, '.gemini/state.json'), 'utf8')
      const trustedFolders = await readFile(path.join(home, '.gemini/trustedFolders.json'), 'utf8')
      const report = JSON.parse(await readFile(reportPath, 'utf8')) as {
        adapter: string
        applied_items: number
        degradation: string[]
      }

      expect(geminiMd).toContain('<!-- agentforge-context:start v1 -->')
      expect(geminiMd).toContain('These instructions are generated for Gemini CLI')
      expect(geminiMd).toContain('~/.gemini/GEMINI.md')
      expect(geminiMd).toContain('Prod deploy rule')
      expect(geminiMd).toContain('Run make prod-ext after main pipeline succeeds.')
      expect(geminiMd).toContain('[redacted: secret_detected]')
      expect(geminiMd).not.toContain('raw-token-should-not-be-written')
      expect(geminiMd).toContain('prod-ext-check v1')
      expect(geminiMd).toContain('/home/agent/.gemini/skills/project/prod-ext-check')
      expect(geminiMd).toContain('Degradation: budget_truncated, no_subagents')
      expect(geminiMd).toContain('<!-- agentforge-context:end -->')
      expect(state).toBe('{"hasCompletedOnboarding":true}')
      expect(trustedFolders).toBe('{"/workspace":"TRUST_FOLDER","/":"TRUST_PARENT"}')

      expect(report.adapter).toBe('gemini')
      expect(report.applied_items).toBe(2)
      expect(report.degradation).toContain('budget_truncated')
      expect(report.degradation).toContain('no_subagents')
    } finally {
      await rm(root, { recursive: true, force: true })
    }
  })
})

function envelope() {
  return {
    envelope_version: 'v1',
    run_id: '018f6f75-8f2f-7f00-9e25-111111111111',
    task_id: '018f6f75-8f2f-7f00-9e25-222222222222',
    agent_id: '018f6f75-8f2f-7f00-9e25-333333333333',
    capability: {
      cli_tool: 'gemini',
      runtime_kind: 'container',
      max_context_tokens: 1000000,
      supports_skills_mount: true,
      supports_hooks: true,
      supports_subagents: false,
    },
    applied: [
      {
        id: '018f6f75-8f2f-7f00-9e25-444444444444',
        kind: 'memory',
        title: 'Prod deploy rule',
        content: 'Run make prod-ext after main pipeline succeeds.',
        content_ref: 'memory_items/prod-deploy-rule',
        sensitivity: 'internal',
        source: {
          source_type: 'task_run',
          source_id: '018f6f75-8f2f-7f00-9e25-555555555555',
          title: 'Previous governed context run',
        },
      },
      {
        id: '018f6f75-8f2f-7f00-9e25-666666666666',
        kind: 'memory',
        title: 'Do not expose secret',
        content: 'raw-token-should-not-be-written',
        content_ref: 'memory_items/secret',
        sensitivity: 'secret_detected',
        source: {
          source_type: 'manual',
        },
      },
    ],
    skills_mount: [
      {
        name: 'prod-ext-check',
        version: 1,
        path: '/home/agent/.gemini/skills/project/prod-ext-check',
      },
    ],
    degradation: ['budget_truncated'],
  }
}

async function runContextHelper(args: string[]): Promise<void> {
  const child = spawn(
    'cargo',
    [
      'run',
      '--quiet',
      '-p',
      'agent-context-helper',
      '--bin',
      'agent-context-helper',
      '--',
      ...args,
    ],
    {
      cwd: rustRoot,
      env: process.env,
    }
  )

  let stdout = ''
  let stderr = ''
  child.stdout.on('data', (chunk) => {
    stdout += chunk.toString()
  })
  child.stderr.on('data', (chunk) => {
    stderr += chunk.toString()
  })

  const code = await new Promise<number | null>((resolve, reject) => {
    child.once('error', reject)
    child.once('exit', resolve)
  })

  if (code !== 0) {
    throw new Error(
      `agent-context-helper failed with code ${code}\nstdout:\n${stdout}\nstderr:\n${stderr}`
    )
  }
}
