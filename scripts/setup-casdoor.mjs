#!/usr/bin/env node
import { spawnSync } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import { constants } from 'node:fs'
import { chmod, lstat, mkdir, open, readFile, rename, unlink } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const envPath = path.resolve(process.env.ENV_FILE ?? path.join(root, 'docker/.env'))
const secretKeys = [
  'CASDOOR_DB_PASSWORD',
  'CASDOOR_RADIUS_SECRET',
  'OIDC_CASDOOR_CLIENT_SECRET',
  'ORCHESTRATOR_OIDC_CLIENT_SECRET',
]
const isWindows = process.platform === 'win32'

function fail(message) {
  const error = new Error(message)
  error.safeMessage = message
  throw error
}

async function lstatOrNull(filePath) {
  try {
    return await lstat(filePath)
  } catch (error) {
    if (error.code === 'ENOENT') return null
    throw error
  }
}

async function requireRegularFile(filePath, label) {
  const stat = await lstatOrNull(filePath)
  if (stat && (!stat.isFile() || stat.isSymbolicLink()))
    fail(`${label} must be a regular file, not a symlink`)
  return stat
}

function setEnvValues(lines, values) {
  for (const [key, value] of values) {
    const pattern = new RegExp(`^(\\s*(?:export\\s+)?${key}\\s*=).*$`)
    let lastIndex = -1
    let prefix
    lines.forEach((line, index) => {
      const match = line.match(pattern)
      if (!match) return
      lastIndex = index
      prefix = match[1]
    })
    if (lastIndex >= 0) lines[lastIndex] = `${prefix}${value}`
    else lines.push(`${key}=${value}`)
  }
  return `${lines.join('\n').replace(/\n+$/, '')}\n`
}

function validateSecret(key, value) {
  if (value.length < 32) {
    fail(
      `${key} must be at least 32 characters. Previously shipped defaults are rejected; manually rotate any existing Casdoor data before continuing.`
    )
  }
  if (!/^[A-Za-z0-9_-]+$/.test(value)) {
    fail(
      `${key} must be at least 32 characters and use only letters, numbers, hyphens, or underscores.`
    )
  }
  if (/^REPLACE_WITH_/i.test(value))
    fail(`${key} still contains a configuration template placeholder.`)
}

function resolveComposeEnvironment(hostEnv) {
  const values = [
    ...secretKeys.map((key) => `      ${key}: "\${${key}-}"`),
    '      CASDOOR_CONFIG_DIR: "${CASDOOR_CONFIG_DIR:-./casdoor/private}"',
  ].join('\n')
  const input = `services:\n  casdoor_setup_values:\n    image: scratch\n    environment:\n${values}\n`
  const result = spawnSync(
    'docker',
    ['compose', '--env-file', envPath, '-f', '-', 'config', '--format', 'json'],
    { input, encoding: 'utf8', env: hostEnv, maxBuffer: 1024 * 1024 }
  )
  if (result.error || result.status !== 0)
    fail(
      'Could not resolve Casdoor configuration with Docker Compose. Check Docker Compose v2 and ENV_FILE.'
    )

  let config
  try {
    config = JSON.parse(result.stdout)
  } catch {
    fail('Docker Compose returned an invalid Casdoor configuration result.')
  }
  const environment = config?.services?.casdoor_setup_values?.environment
  if (!environment || typeof environment !== 'object')
    fail('Docker Compose did not resolve the Casdoor environment values.')
  return environment
}

async function ensurePrivateDirectory(configDir) {
  await mkdir(configDir, { recursive: true, mode: 0o700 })
  const stat = await lstat(configDir)
  if (!stat.isDirectory() || stat.isSymbolicLink())
    fail('Casdoor config directory must be a real directory, not a symlink')
  if (!isWindows) await chmod(configDir, 0o700)
}

async function writeEnvAtomically(contents) {
  const tempPath = `${envPath}.tmp-${process.pid}-${randomBytes(4).toString('hex')}`
  try {
    const handle = await open(tempPath, 'wx', 0o600)
    try {
      await handle.writeFile(contents, 'utf8')
    } finally {
      await handle.close()
    }
    await rename(tempPath, envPath)
  } catch (error) {
    await unlink(tempPath).catch(() => undefined)
    throw error
  }
  if (!isWindows) await chmod(envPath, 0o600)
}

async function writePrivateFile(filePath, contents) {
  const flags =
    constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL | (constants.O_NOFOLLOW ?? 0)
  const handle = await open(filePath, flags, 0o600)
  try {
    if (!isWindows) await handle.chmod(0o600)
    await handle.writeFile(contents, 'utf8')
  } finally {
    await handle.close()
  }
}

function assertAppSecrets(contents, values) {
  const dsn = contents.match(/^\s*dataSourceName\s*=\s*"([^"]*)"\s*$/m)?.[1] ?? ''
  const databasePassword = dsn.match(/(?:^|\s)password=([^\s"]+)(?=\s|$)/)?.[1]
  const radiusSecret = contents.match(/^\s*radiusSecret\s*=\s*"([^"]*)"\s*$/m)?.[1]
  if (
    databasePassword !== values.get('CASDOOR_DB_PASSWORD') ||
    radiusSecret !== values.get('CASDOOR_RADIUS_SECRET')
  ) {
    fail(
      'Existing Casdoor app.conf secrets do not match the effective environment. Reconcile them manually; refusing overwrite.'
    )
  }
}

function assertClientSecrets(contents, values) {
  let config
  try {
    config = JSON.parse(contents)
  } catch {
    fail(
      'Existing Casdoor init_data.json is invalid JSON. Repair the private file manually; refusing overwrite.'
    )
  }

  if (!Array.isArray(config?.applications))
    fail(
      'Existing Casdoor init_data.json has no applications list. Repair the private file manually; refusing overwrite.'
    )

  for (const [name, key] of [
    ['agentforge', 'OIDC_CASDOOR_CLIENT_SECRET'],
    ['orchestrator', 'ORCHESTRATOR_OIDC_CLIENT_SECRET'],
  ]) {
    const matches = config.applications.filter((app) => app?.name === name)
    if (matches.length !== 1 || matches[0].clientSecret !== values.get(key)) {
      fail(
        `Existing Casdoor ${name} client secret does not match the effective environment. Reconcile it manually; refusing overwrite.`
      )
    }
  }
}

async function main() {
  const envStat = await requireRegularFile(envPath, 'ENV_FILE')
  if (!envStat) fail('docker/.env is missing. Run make bootstrap-local first.')
  const originalEnv = await readFile(envPath, 'utf8')
  const hostEnvWithoutSecrets = { ...process.env }
  for (const key of secretKeys) delete hostEnvWithoutSecrets[key]
  const fileValues = resolveComposeEnvironment(hostEnvWithoutSecrets)
  const effectiveValues = resolveComposeEnvironment(process.env)
  const values = new Map()
  const envUpdates = new Map()

  for (const key of secretKeys) {
    const current = fileValues[key] ?? ''
    const exported = Object.hasOwn(process.env, key)
    const override = effectiveValues[key] ?? ''
    if (exported) {
      if (override.trim() === '')
        fail(`${key} is exported as blank. Set a valid value or clear the exported variable.`)
      validateSecret(key, override)
      if (current.trim() !== '' && current !== override) {
        fail(
          `${key} differs between ENV_FILE and the exported environment. Synchronize the values or clear the exported variable; ENV_FILE was left unchanged.`
        )
      }
      values.set(key, override)
      if (current.trim() === '') envUpdates.set(key, override)
    } else if (current.trim() === '') {
      const generated = randomBytes(32).toString('hex')
      values.set(key, generated)
      envUpdates.set(key, generated)
    } else {
      values.set(key, current)
    }
    validateSecret(key, values.get(key))
  }

  const configDirInput = effectiveValues.CASDOOR_CONFIG_DIR || './casdoor/private'
  const configDir = path.resolve(root, 'docker', configDirInput)

  const appTemplate = await readFile(path.join(root, 'docker/casdoor/app.conf'), 'utf8')
  let initTemplate
  try {
    initTemplate = JSON.parse(
      await readFile(path.join(root, 'docker/casdoor/init_data.json'), 'utf8')
    )
  } catch {
    fail(
      'Tracked Casdoor init_data.json template is invalid JSON; refusing to render private config.'
    )
  }
  const appConfig = appTemplate
    .replace('REPLACE_WITH_PRIVATE_CASDOOR_DB_PASSWORD', values.get('CASDOOR_DB_PASSWORD'))
    .replace('REPLACE_WITH_PRIVATE_CASDOOR_RADIUS_SECRET', values.get('CASDOOR_RADIUS_SECRET'))
  for (const [name, key] of [
    ['agentforge', 'OIDC_CASDOOR_CLIENT_SECRET'],
    ['orchestrator', 'ORCHESTRATOR_OIDC_CLIENT_SECRET'],
  ]) {
    const matches = initTemplate.applications?.filter((app) => app.name === name) ?? []
    if (matches.length !== 1)
      fail(`Casdoor init template must contain exactly one ${name} application`)
    matches[0].clientSecret = values.get(key)
  }
  const expected = new Map([
    ['app.conf', appConfig],
    ['init_data.json', `${JSON.stringify(initTemplate, null, 2)}\n`],
  ])

  await ensurePrivateDirectory(configDir)
  const existing = new Map()
  for (const [name] of expected) {
    const filePath = path.join(configDir, name)
    const stat = await requireRegularFile(filePath, `Casdoor ${name}`)
    if (!stat) continue
    existing.set(name, await readFile(filePath, 'utf8'))
  }
  if (existing.has('app.conf')) assertAppSecrets(existing.get('app.conf'), values)
  if (existing.has('init_data.json')) assertClientSecrets(existing.get('init_data.json'), values)

  const updatedEnv = setEnvValues(originalEnv.split(/\r?\n/), envUpdates)
  if (envUpdates.size && updatedEnv !== originalEnv) await writeEnvAtomically(updatedEnv)
  else if (!isWindows) await chmod(envPath, 0o600)

  for (const [name, contents] of expected) {
    const filePath = path.join(configDir, name)
    if (!existing.has(name)) await writePrivateFile(filePath, contents)
    if (!isWindows) await chmod(filePath, 0o600)
  }

  console.log('Casdoor private configuration is ready.')
}

main().catch((error) => {
  const message =
    typeof error?.safeMessage === 'string'
      ? error.safeMessage
      : 'Casdoor setup failed safely. Check file access and configuration, then retry.'
  console.error(`[casdoor-setup] ERROR: ${message}`)
  process.exitCode = 1
})
