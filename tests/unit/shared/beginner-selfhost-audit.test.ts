import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { spawnSync } from 'node:child_process'
import { describe, expect, it } from 'vitest'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const auditScript = fs.readFileSync(
  path.join(projectRoot, 'scripts/audit-beginner-selfhost.sh'),
  'utf8'
)

function runLocalSmoke(extraEnv: Record<string, string> = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-smoke-test-'))
  const bin = path.join(root, 'bin')
  const scripts = path.join(root, 'scripts')
  const callLog = path.join(root, 'calls.jsonl')
  fs.mkdirSync(bin)
  fs.mkdirSync(scripts)
  const commandStub = [
    "const fs = require('node:fs')",
    'const command = process.argv[2]',
    'const args = process.argv.slice(3)',
    "const networkFile = args.find(arg => arg.endsWith('/smoke-network.yml'))",
    'fs.appendFileSync(process.env.FORGE_SMOKE_TEST_CALL_LOG, JSON.stringify({',
    '  command, args, workspaceRoot: process.env.AGENTFORGE_WORKSPACE_ROOT,',
    "  workspaceExists: fs.existsSync(process.env.AGENTFORGE_WORKSPACE_ROOT || ''),",
    '  networkSubnet: process.env.BEGINNER_SMOKE_NETWORK_SUBNET,',
    '  bindAddress: process.env.BIND_ADDRESS,',
    '  ports: Object.fromEntries(["HTTP_PORT", "HTTPS_PORT", "DB_EXPOSED_PORT",',
    '    "REDIS_EXPOSED_PORT", "NATS_PORT", "NATS_MONITOR_PORT", "TEMPORAL_PORT",',
    '    "TEMPORAL_UI_PORT"].map(key => [key, process.env[key]])),',
    "  networkConfig: networkFile ? fs.readFileSync(networkFile, 'utf8') : null",
    "}) + '\\n')",
    "if (command === 'docker' && args[0] === 'network' && args[1] === 'create'",
    "    && process.env.FORGE_SMOKE_TEST_FAIL_NETWORK_CREATE === '1') process.exit(1)",
    "if (command === 'docker' && args[0] === 'compose' && args.includes('down')",
    "    && process.env.FORGE_SMOKE_TEST_FAIL_CLEANUP === '1') process.exit(1)",
    "if (command === 'docker' && args[0] === 'network' && args[1] === 'rm'",
    "    && process.env.FORGE_SMOKE_TEST_FAIL_NETWORK_REMOVAL === '1') process.exit(1)",
  ].join('\n')
  fs.writeFileSync(path.join(bin, 'command.cjs'), commandStub)
  for (const command of ['docker', 'make']) {
    fs.writeFileSync(
      path.join(bin, command),
      '#!/usr/bin/env bash\nexec node -- "$(dirname "$0")/command.cjs" "$(basename "$0")" "$@"\n',
      { mode: 0o700 }
    )
  }
  for (const helper of ['bootstrap-selfhost.sh', 'check-selfhost-runtime.sh']) {
    fs.writeFileSync(path.join(scripts, helper), '#!/usr/bin/env bash\nexit 0\n')
  }
  const runner = path.join(scripts, 'audit.sh')
  fs.writeFileSync(runner, auditScript.replace(/\nmain "\$@"\s*$/, '\nlocal_smoke\n'))
  try {
    const result = spawnSync('bash', [runner], {
      encoding: 'utf8',
      timeout: 10_000,
      env: {
        ...process.env,
        PATH: bin + path.delimiter + process.env.PATH,
        TMPDIR: root,
        FORGE_SMOKE_TEST_CALL_LOG: callLog,
        BEGINNER_SMOKE_AGENT_SUBNET: '',
        BEGINNER_SMOKE_NETWORK_SUBNET: '',
        ...extraEnv,
      },
    })
    if (result.error) throw result.error
    const calls = fs
      .readFileSync(callLog, 'utf8')
      .trim()
      .split('\n')
      .map((line) => JSON.parse(line))
    return { ...result, calls, root }
  } finally {
    fs.rmSync(root, { recursive: true, force: true })
  }
}

describe('beginner self-host audit', () => {
  it('keeps default smoke networks and workspace preparation scoped to the temporary run', () => {
    const { status, stderr, calls, root } = runLocalSmoke()
    expect(status, stderr).toBe(0)
    const create = calls.find((call) => call.command === 'docker' && call.args[1] === 'create')
    expect(create.args).toEqual(['network', 'create', expect.stringMatching(/^beginner-audit-/)])
    const setup = calls.find((call) => call.command === 'make')
    expect(setup.workspaceRoot).toMatch(new RegExp('^' + root + '/[^/]+/workspaces$'))
    expect(setup.workspaceExists).toBe(true)
    const compose = calls.find((call) => call.args[0] === 'compose')
    expect(compose.networkConfig).toBeNull()
    expect(calls.find((call) => call.args[1] === 'rm').args).toEqual([
      'network',
      'rm',
      create.args.at(-1),
    ])
  })

  it('passes explicit subnets to the owned agent network and the Compose override', () => {
    const { status, stderr, calls } = runLocalSmoke({
      BEGINNER_SMOKE_AGENT_SUBNET: '198.19.230.0/28',
      BEGINNER_SMOKE_NETWORK_SUBNET: '198.19.231.0/28',
    })
    expect(status, stderr).toBe(0)
    const create = calls.find((call) => call.command === 'docker' && call.args[1] === 'create')
    expect(create.args).toEqual([
      'network',
      'create',
      '--subnet',
      '198.19.230.0/28',
      expect.stringMatching(/^beginner-audit-/),
    ])
    const compose = calls.find((call) => call.args[0] === 'compose')
    expect(compose.networkConfig).toContain('ipam:')
    expect(compose.networkConfig).toContain('BEGINNER_SMOKE_NETWORK_SUBNET')
    expect(compose.networkSubnet).toBe('198.19.231.0/28')
  })

  it('binds smoke ports to loopback without changing the numeric health-check ports', () => {
    const { status, stderr, calls } = runLocalSmoke()
    expect(status, stderr).toBe(0)
    const compose = calls.find((call) => call.args[0] === 'compose')
    expect(compose.bindAddress).toBe('127.0.0.1')
    expect(Object.values(compose.ports)).toHaveLength(8)
    for (const port of Object.values(compose.ports)) {
      expect(port).toMatch(/^127\.0\.0\.1:\d+$/)
    }
    const setup = calls.find((call) => call.command === 'make')
    expect(setup.bindAddress).toBe('127.0.0.1')
    expect(setup.ports.HTTPS_PORT).toMatch(/^\d+$/)
    expect(setup.ports.NATS_MONITOR_PORT).toMatch(/^\d+$/)
  })

  it('reports a failed network creation before starting or removing any Compose resources', () => {
    const { status, stderr, calls } = runLocalSmoke({
      FORGE_SMOKE_TEST_FAIL_NETWORK_CREATE: '1',
    })
    expect(status).toBe(1)
    expect(stderr).toContain('isolated agent network creation failed')
    expect(stderr).toContain('BEGINNER_SMOKE_AGENT_SUBNET')
    expect(calls).toHaveLength(1)
    expect(calls[0].args.slice(0, 2)).toEqual(['network', 'create'])
  })

  it('keeps cleanup failure visible instead of reporting a successful smoke run', () => {
    const { status, stdout, stderr } = runLocalSmoke({
      FORGE_SMOKE_TEST_FAIL_CLEANUP: '1',
    })
    expect(status).toBe(1)
    expect(stderr).toContain('local smoke cleanup failed')
    expect(stdout).not.toContain('passes public ingress health, and cleans up')
  })

  it('keeps network removal failure visible after Compose cleanup', () => {
    const { status, stdout, stderr } = runLocalSmoke({
      FORGE_SMOKE_TEST_FAIL_NETWORK_REMOVAL: '1',
    })
    expect(status).toBe(1)
    expect(stderr).toContain('local smoke network cleanup failed')
    expect(stdout).not.toContain('passes public ingress health, and cleans up')
  })

  it('runs checked-in helper scripts through bash', () => {
    expect(auditScript.match(/bash "\$ROOT_DIR\/scripts\/bootstrap-selfhost\.sh"/g)).toHaveLength(3)
    expect(
      auditScript.match(/bash "\$ROOT_DIR\/scripts\/check-selfhost-runtime\.sh"/g)
    ).toHaveLength(2)
  })

  it('can verify Provider+Prompt using an existing verified provider', () => {
    expect(auditScript).toContain('BEGINNER_USE_EXISTING_PROVIDER')
    expect(auditScript).toContain('no enabled provider with persisted passed test status was found')
    expect(auditScript).toContain('"/llm-providers/$provider_id/test"')
    expect(auditScript).toContain('/agents/$agent_id/prompt')
    expect(auditScript).toContain('event: delta')
    expect(auditScript).toContain('provider prompt stream did not contain assistant text')
  })

  it('still requires an explicit key for cloud providers when not reusing an existing provider', () => {
    expect(auditScript).toContain('not required for ollama')
    expect(auditScript).toContain("tr '[:upper:]' '[:lower:]'")
    expect(auditScript).toContain('[ "$provider" != "ollama" ]')
    expect(auditScript).toContain('BEGINNER_API_KEY is required for --provider')
    expect(auditScript).toContain('BEGINNER_PROVIDER is required for --provider')
    expect(auditScript).toContain('BEGINNER_MODEL is required for --provider')
  })
})
