import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { spawnSync } from 'node:child_process'
import yaml from 'yaml'
import { describe, expect, it } from 'vitest'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const makefile = fs.readFileSync(path.join(projectRoot, 'Makefile'), 'utf8')
const compose = yaml.parse(
  fs.readFileSync(path.join(projectRoot, 'docker/compose.yml'), 'utf8')
) as {
  services?: Record<string, { group_add?: string[] }>
}

describe('workspace root runtime contract', () => {
  it('shares the agent gid with the server and prepares only a guarded workspace root', () => {
    expect(compose.services?.['agentforge-server']?.group_add).toContain('${CLAUDE_GID:-1012}')

    const setup = makefile.match(/\.PHONY: setup[\s\S]*?\n\.PHONY: setup-external/)?.[0]
    expect(setup).toBeDefined()
    expect(setup).toContain('case "$(_WORKSPACE_ROOT_DIR)" in /*)')
    expect(setup).toContain('[ "$(_WORKSPACE_ROOT_DIR)" != "/" ]')
    expect(setup).toContain('[ ! -L "$$current" ]')
    expect(setup).toContain('chgrp $(_WORKSPACE_GID) /workspace-root')
    expect(setup).toContain('chmod 2775 /workspace-root')
    expect(setup).not.toMatch(/\b(?:chgrp|chmod)\s+-R\b/)
    expect(setup).not.toContain('chmod 777')
  })

  it('rejects a symlinked workspace root before invoking Docker', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'forge-workspace-root-'))
    const bin = path.join(root, 'bin')
    const target = path.join(root, 'target')
    const linkedRoot = path.join(root, 'workspaces')
    const docker = path.join(bin, 'docker')
    fs.mkdirSync(bin)
    fs.mkdirSync(target)
    fs.symlinkSync(target, linkedRoot)
    fs.writeFileSync(docker, '#!/bin/sh\necho docker-was-invoked >&2\nexit 99\n', { mode: 0o755 })

    try {
      const result = spawnSync(
        'make',
        [
          '--silent',
          'setup',
          `AGENTFORGE_WORKSPACE_ROOT=${linkedRoot}`,
          `OAUTH_MOUNT_DIR_NAME=${path.join(root, 'oauth')}`,
        ],
        {
          cwd: projectRoot,
          encoding: 'utf8',
          env: { ...process.env, PATH: `${bin}:${process.env.PATH ?? ''}` },
        }
      )

      expect(result.status).not.toBe(0)
      expect(result.stderr).toContain('AGENTFORGE_WORKSPACE_ROOT must not contain symbolic links')
      expect(result.stderr).not.toContain('docker-was-invoked')
    } finally {
      fs.rmSync(root, { recursive: true, force: true })
    }
  })
})
