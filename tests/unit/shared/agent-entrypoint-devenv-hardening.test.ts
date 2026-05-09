import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const entrypointPath = path.join(projectRoot, 'docker/scripts/agent-entrypoint.sh')
const codexHookTemplatePath = path.join(projectRoot, 'hooks/templates/codex.json')

describe('agent entrypoint devenv hardening', () => {
  const script = fs.readFileSync(entrypointPath, 'utf8')

  it('sets safe default compose parallel limit in devenv mode', () => {
    expect(script).toContain('export COMPOSE_PARALLEL_LIMIT="${COMPOSE_PARALLEL_LIMIT:-1}"')
  })

  it('supports configurable minimum memory threshold for devenv containers', () => {
    expect(script).toContain('AGENTFORGE_DEVENV_MIN_MEMORY_MB')
  })

  it('emits explicit cgroup OOM diagnostics when sidecar exits unexpectedly', () => {
    expect(script).toContain('/sys/fs/cgroup/memory.events')
    expect(script).toContain('oom_kill')
    expect(script).toContain('sidecar exited unexpectedly')
  })

  it('emits machine-parseable alert markers for sidecar crash diagnostics', () => {
    expect(script).toContain('sidecar_unexpected_exit_total')
    expect(script).toContain('sidecar_oom_kill_total')
  })

  it('starts Gemini in YOLO permission mode', () => {
    expect(script).toContain('DEFAULT_CMD="gemini --yolo --skip-trust"')
    expect(script).toContain('export GEMINI_CLI_TRUST_WORKSPACE=true')
  })

  it('prefers Codex YOLO permission mode with a safe fallback for current CLI releases', () => {
    expect(script).toContain('DEFAULT_CMD="codex --yolo"')
    expect(script).toContain(
      'DEFAULT_CMD_FALLBACK="codex --dangerously-bypass-approvals-and-sandbox"'
    )
    expect(script).toContain('grep -q -- "--yolo"')
    expect(script).not.toContain('codex exec --json')
  })

  it('configures Codex native hooks without relying on notify output', () => {
    expect(script).toContain('HOOKS_FILE=~/.codex/hooks.json')
    expect(script).toContain('HOOK_COMPAT="native"')
    expect(script).toContain('codex_hooks = true')

    const template = JSON.parse(fs.readFileSync(codexHookTemplatePath, 'utf8'))
    expect(Object.keys(template.hooks).sort()).toEqual([
      'PermissionRequest',
      'PostToolUse',
      'PreToolUse',
      'SessionStart',
      'Stop',
      'UserPromptSubmit',
    ])
  })
})
