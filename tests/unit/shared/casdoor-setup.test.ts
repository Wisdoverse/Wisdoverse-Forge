import { spawnSync } from 'node:child_process'
import {
  lstatSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const root = path.resolve(import.meta.dirname, '../../..')
const script = path.join(root, 'scripts/setup-casdoor.mjs')

function run(envFile: string, configDir?: string, exported: Record<string, string> = {}) {
  return spawnSync(process.execPath, [script], {
    env: {
      PATH: process.env.PATH ?? '',
      HOME: process.env.HOME ?? os.homedir(),
      ENV_FILE: envFile,
      ...(configDir ? { CASDOOR_CONFIG_DIR: configDir } : {}),
      ...exported,
    },
    encoding: 'utf8',
  })
}

function envWithSecrets(overrides = '') {
  return [
    'CASDOOR_DB_PASSWORD=',
    'CASDOOR_RADIUS_SECRET=',
    'OIDC_CASDOOR_CLIENT_SECRET=',
    'ORCHESTRATOR_OIDC_CLIENT_SECRET=',
    overrides,
  ]
    .filter(Boolean)
    .join('\n')
}

describe('Casdoor private setup CLI', () => {
  it('generates private values once and preserves matching configs on rerun', () => {
    const temp = mkdtempSync(path.join(os.tmpdir(), 'forge-casdoor-'))
    const envFile = path.join(temp, 'docker.env')
    const configDir = path.join(temp, 'private')

    try {
      writeFileSync(
        envFile,
        [
          'CASDOOR_DB_PASSWORD=',
          'export CASDOOR_RADIUS_SECRET=',
          'OIDC_CASDOOR_CLIENT_SECRET=',
          'ORCHESTRATOR_OIDC_CLIENT_SECRET=',
        ].join('\n')
      )
      const first = run(envFile, configDir)
      expect(first.status, first.stderr).toBe(0)

      const env = readFileSync(envFile, 'utf8')
      const keys = [
        'CASDOOR_DB_PASSWORD',
        'CASDOOR_RADIUS_SECRET',
        'OIDC_CASDOOR_CLIENT_SECRET',
        'ORCHESTRATOR_OIDC_CLIENT_SECRET',
      ]
      const values = Object.fromEntries(
        keys.map((key) => {
          const value = env.match(
            new RegExp(`^\\s*(?:export\\s+)?${key}=([a-f0-9]{64})$`, 'm')
          )?.[1]
          expect(value, `${key} is generated as 64 hex characters`).toBeDefined()
          return [key, value]
        })
      )
      const appConfigPath = path.join(configDir, 'app.conf')
      const initDataPath = path.join(configDir, 'init_data.json')
      const appConfig = readFileSync(appConfigPath, 'utf8')
      const initData = JSON.parse(readFileSync(initDataPath, 'utf8')) as {
        applications: Array<{ name: string; clientSecret: string; redirectUris: string[] }>
      }

      expect(appConfig).toContain(`password=${values.CASDOOR_DB_PASSWORD}`)
      expect(appConfig).toContain(`radiusSecret = "${values.CASDOOR_RADIUS_SECRET}"`)
      expect(initData.applications.find((app) => app.name === 'agentforge')?.clientSecret).toBe(
        values.OIDC_CASDOOR_CLIENT_SECRET
      )
      expect(initData.applications.find((app) => app.name === 'orchestrator')?.clientSecret).toBe(
        values.ORCHESTRATOR_OIDC_CLIENT_SECRET
      )
      expect(`${first.stdout}${first.stderr}`).not.toContain(values.CASDOOR_DB_PASSWORD)
      if (process.platform !== 'win32') {
        expect(lstatSync(configDir).mode & 0o777).toBe(0o700)
        expect(lstatSync(appConfigPath).mode & 0o777).toBe(0o600)
        expect(lstatSync(initDataPath).mode & 0o777).toBe(0o600)
      }

      const customizedAppConfig = `${appConfig}\n# operator customization\norigin = "https://login.example.test"\n`
      initData.applications
        .find((app) => app.name === 'agentforge')!
        .redirectUris.push('https://login.example.test/callback')
      const customizedInitData = `${JSON.stringify(initData, null, 2)}\n`
      writeFileSync(appConfigPath, customizedAppConfig)
      writeFileSync(initDataPath, customizedInitData)
      const configBefore = [customizedAppConfig, customizedInitData]
      const second = run(envFile, configDir)
      expect(second.status, second.stderr).toBe(0)
      expect(readFileSync(envFile, 'utf8')).toBe(env)
      expect([readFileSync(appConfigPath, 'utf8'), readFileSync(initDataPath, 'utf8')]).toEqual(
        configBefore
      )

      const changedEnv = env.replace(values.CASDOOR_DB_PASSWORD!, 'a'.repeat(64))
      writeFileSync(envFile, changedEnv)
      const mismatch = run(envFile, configDir)
      expect(mismatch.status).toBe(1)
      expect(mismatch.stderr).toContain('Existing Casdoor app.conf secrets do not match')
      expect(mismatch.stderr).not.toContain(values.CASDOOR_DB_PASSWORD!)
      expect(mismatch.stderr).not.toContain('a'.repeat(64))
      expect(readFileSync(envFile, 'utf8')).toBe(changedEnv)
      expect([readFileSync(appConfigPath, 'utf8'), readFileSync(initDataPath, 'utf8')]).toEqual(
        configBefore
      )
    } finally {
      rmSync(temp, { recursive: true, force: true })
    }
  })

  it('uses .env config directory and rejects conflicting exported secrets without printing them', () => {
    const temp = mkdtempSync(path.join(os.tmpdir(), 'forge-casdoor-effective-'))
    const envFile = path.join(temp, 'docker.env')
    const configDir = path.join(temp, 'private-from-env')

    try {
      writeFileSync(envFile, `${envWithSecrets()}\nCASDOOR_CONFIG_DIR=${configDir}\n`)
      const exportedSecret = 'e'.repeat(64)
      const first = run(envFile, undefined, { CASDOOR_DB_PASSWORD: exportedSecret })
      expect(first.status, first.stderr).toBe(0)
      expect(lstatSync(path.join(configDir, 'app.conf')).isFile()).toBe(true)
      expect(readFileSync(envFile, 'utf8')).toContain(`CASDOOR_DB_PASSWORD=${exportedSecret}`)

      const dbSecret = readFileSync(envFile, 'utf8').match(/^CASDOOR_DB_PASSWORD=(.+)$/m)?.[1]
      const conflicting = 'z'.repeat(64)
      expect(dbSecret).toBeDefined()
      const result = run(envFile, undefined, { CASDOOR_DB_PASSWORD: conflicting })
      expect(result.status).toBe(1)
      expect(result.stderr).toContain('Synchronize the values or clear the exported variable')
      expect(result.stderr).not.toContain(dbSecret!)
      expect(result.stderr).not.toContain(conflicting)
      expect(readFileSync(envFile, 'utf8')).toContain(`CASDOOR_DB_PASSWORD=${dbSecret}`)
    } finally {
      rmSync(temp, { recursive: true, force: true })
    }
  })

  it('rejects malformed private JSON with a safe diagnostic that does not echo its contents', () => {
    const temp = mkdtempSync(path.join(os.tmpdir(), 'forge-casdoor-bad-json-'))
    const envFile = path.join(temp, 'docker.env')
    const configDir = path.join(temp, 'private')
    const secrets = {
      CASDOOR_DB_PASSWORD: 'a'.repeat(64),
      CASDOOR_RADIUS_SECRET: 'b'.repeat(64),
      OIDC_CASDOOR_CLIENT_SECRET: 'c'.repeat(64),
      ORCHESTRATOR_OIDC_CLIENT_SECRET: 'd'.repeat(64),
    }

    try {
      writeFileSync(
        envFile,
        envWithSecrets(
          Object.entries(secrets)
            .map(([k, v]) => `${k}=${v}`)
            .join('\n')
        )
      )
      mkdirSync(configDir)
      writeFileSync(
        path.join(configDir, 'app.conf'),
        `dataSourceName = "user=root password=${secrets.CASDOOR_DB_PASSWORD} host=db"\nradiusSecret = "${secrets.CASDOOR_RADIUS_SECRET}"\n`
      )
      writeFileSync(
        path.join(configDir, 'init_data.json'),
        `{"secret":"${secrets.OIDC_CASDOOR_CLIENT_SECRET}",`
      )

      const result = run(envFile, configDir)
      expect(result.status).toBe(1)
      expect(result.stderr).toContain('Existing Casdoor init_data.json is invalid JSON')
      for (const value of Object.values(secrets)) expect(result.stderr).not.toContain(value)
    } finally {
      rmSync(temp, { recursive: true, force: true })
    }
  })

  it('uses Docker Compose dotenv parsing and leaves existing quoted values untouched', () => {
    const temp = mkdtempSync(path.join(os.tmpdir(), 'forge-casdoor-dotenv-'))
    const envFile = path.join(temp, 'docker.env')
    const configDir = path.join(temp, 'private config')
    const secrets = {
      CASDOOR_DB_PASSWORD: 'a'.repeat(64),
      CASDOOR_RADIUS_SECRET: 'b'.repeat(64),
      OIDC_CASDOOR_CLIENT_SECRET: 'c'.repeat(64),
      ORCHESTRATOR_OIDC_CLIENT_SECRET: 'd'.repeat(64),
    }
    const env = [
      `export CASDOOR_DB_PASSWORD="${secrets.CASDOOR_DB_PASSWORD}" # retained comment`,
      `CASDOOR_RADIUS_SECRET='${secrets.CASDOOR_RADIUS_SECRET}' # retained comment`,
      `OIDC_CASDOOR_CLIENT_SECRET="${secrets.OIDC_CASDOOR_CLIENT_SECRET}"`,
      `ORCHESTRATOR_OIDC_CLIENT_SECRET=${secrets.ORCHESTRATOR_OIDC_CLIENT_SECRET} # retained comment`,
      `CASDOOR_CONFIG_DIR="${configDir}" # path with spaces`,
    ].join('\n')

    try {
      writeFileSync(envFile, env)
      const result = run(envFile)
      expect(result.status, result.stderr).toBe(0)
      expect(readFileSync(envFile, 'utf8')).toBe(env)
      expect(lstatSync(path.join(configDir, 'app.conf')).isFile()).toBe(true)
      expect(readFileSync(path.join(configDir, 'app.conf'), 'utf8')).toContain(
        `password=${secrets.CASDOOR_DB_PASSWORD}`
      )
    } finally {
      rmSync(temp, { recursive: true, force: true })
    }
  })

  it('rejects long template placeholders without modifying .env', () => {
    const temp = mkdtempSync(path.join(os.tmpdir(), 'forge-casdoor-placeholder-'))
    const envFile = path.join(temp, 'docker.env')
    const configDir = path.join(temp, 'private')
    const placeholder = `REPLACE_WITH_${'x'.repeat(40)}`
    const env = envWithSecrets(`CASDOOR_DB_PASSWORD=${placeholder}`)

    try {
      writeFileSync(envFile, env)
      const result = run(envFile, configDir)
      expect(result.status).toBe(1)
      expect(result.stderr).toContain('configuration template placeholder')
      expect(result.stderr).not.toContain(placeholder)
      expect(readFileSync(envFile, 'utf8')).toBe(env)
      expect(() => lstatSync(configDir)).toThrow()
    } finally {
      rmSync(temp, { recursive: true, force: true })
    }
  })

  it('rejects short values such as the former public defaults without replacing them', () => {
    const temp = mkdtempSync(path.join(os.tmpdir(), 'forge-casdoor-default-'))
    const envFile = path.join(temp, 'docker.env')
    const configDir = path.join(temp, 'private')

    try {
      writeFileSync(
        envFile,
        envWithSecrets(
          'CASDOOR_DB_PASSWORD=old-public-default\nOIDC_CASDOOR_CLIENT_SECRET=old-client-default'
        )
      )
      const before = readFileSync(envFile, 'utf8')
      const result = run(envFile, configDir)
      expect(result.status).toBe(1)
      expect(result.stderr).toContain('Previously shipped defaults are rejected')
      expect(result.stderr).not.toContain('old-client-default')
      expect(readFileSync(envFile, 'utf8')).toBe(before)
      expect(() => lstatSync(configDir)).toThrow()
    } finally {
      rmSync(temp, { recursive: true, force: true })
    }
  })

  it('rejects a symlinked private config target without touching its target', () => {
    const temp = mkdtempSync(path.join(os.tmpdir(), 'forge-casdoor-link-'))
    const envFile = path.join(temp, 'docker.env')
    const configDir = path.join(temp, 'private')
    const outsideFile = path.join(temp, 'outside.json')

    try {
      writeFileSync(envFile, envWithSecrets())
      mkdirSync(configDir)
      writeFileSync(outsideFile, 'keep')
      try {
        symlinkSync(outsideFile, path.join(configDir, 'init_data.json'))
      } catch (error) {
        const code = (error as NodeJS.ErrnoException).code
        if (
          process.platform !== 'win32' ||
          !['EPERM', 'EACCES', 'ENOTSUP', 'EOPNOTSUPP'].includes(code ?? '')
        ) {
          throw error
        }
        return
      }

      const result = run(envFile, configDir)
      expect(result.status).toBe(1)
      expect(result.stderr).toContain('regular file, not a symlink')
      expect(readFileSync(outsideFile, 'utf8')).toBe('keep')
      expect(readFileSync(envFile, 'utf8')).toBe(envWithSecrets())
    } finally {
      rmSync(temp, { recursive: true, force: true })
    }
  })
})
