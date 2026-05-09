#!/usr/bin/env node
import fs from 'node:fs'
import path from 'node:path'
import process from 'node:process'
import YAML from 'yaml'

const USAGE = `Usage: scripts/release/orchestration_alert_route_contract.mjs --config-file <path|-> --output <path>

Generate a sanitized Alertmanager route contract from a live loaded
Alertmanager config. The output includes only receiver names, route matchers,
and notification integration counts. It never prints webhook URLs, tokens, or
raw receiver settings.

Options:
  --config-file <path|->   Raw Alertmanager YAML/JSON config, or "-" for stdin.
  --output <path>          Sanitized JSON output path. Use "-" for stdout.
  --component <name>       Component matcher to select. Default: orchestration.
  -h, --help               Show this help.
`

function parseArgs(argv) {
  const args = {
    component: 'orchestration',
    configFile: '',
    output: '',
  }

  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i]
    switch (arg) {
      case '--config-file':
        args.configFile = argv[++i] ?? ''
        break
      case '--output':
        args.output = argv[++i] ?? ''
        break
      case '--component':
        args.component = argv[++i] ?? ''
        break
      case '-h':
      case '--help':
        process.stdout.write(USAGE)
        process.exit(0)
        break
      default:
        throw new Error(`unknown argument: ${arg}`)
    }
  }

  if (!args.configFile) {
    throw new Error('--config-file is required')
  }
  if (!args.output) {
    throw new Error('--output is required')
  }
  if (!args.component) {
    throw new Error('--component must not be empty')
  }

  return args
}

function readInput(path) {
  if (path === '-') {
    return fs.readFileSync(0, 'utf8')
  }
  return fs.readFileSync(path, 'utf8')
}

function parseConfig(raw) {
  const parsed = YAML.parse(raw)
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error('Alertmanager config must be an object')
  }
  return parsed
}

function matcherStrings(route) {
  const matchers = []

  if (Array.isArray(route.matchers)) {
    for (const matcher of route.matchers) {
      if (typeof matcher === 'string' && matcher.trim()) {
        matchers.push(matcher.trim())
      }
    }
  }

  if (route.match && typeof route.match === 'object' && !Array.isArray(route.match)) {
    for (const [key, value] of Object.entries(route.match)) {
      matchers.push(`${key}="${String(value)}"`)
    }
  }

  if (route.match_re && typeof route.match_re === 'object' && !Array.isArray(route.match_re)) {
    for (const [key, value] of Object.entries(route.match_re)) {
      matchers.push(`${key}=~"${String(value)}"`)
    }
  }

  return [...new Set(matchers)]
}

function routeMatchesComponent(route, component) {
  const expected = component.trim()
  return matcherStrings(route).some((matcher) => {
    const normalized = matcher.replace(/\s+/g, '')
    return (
      normalized === `component="${expected}"` ||
      normalized === `component=${expected}` ||
      normalized === `component=~"${expected}"` ||
      normalized === `component=~${expected}`
    )
  })
}

function collectRoutes(route, inheritedReceiver = '', path = 'route') {
  if (!route || typeof route !== 'object' || Array.isArray(route)) {
    return []
  }

  const receiver =
    typeof route.receiver === 'string' && route.receiver.trim()
      ? route.receiver.trim()
      : inheritedReceiver

  const current = {
    receiver,
    matchers: matcherStrings(route),
    path,
    matchesComponent: false,
  }
  const routes = [current]

  const children = Array.isArray(route.routes) ? route.routes : []
  children.forEach((child, index) => {
    routes.push(...collectRoutes(child, receiver, `${path}.routes[${index}]`))
  })

  return routes
}

function integrationSummary(receiver) {
  const integrationTypes = []
  let integrationCount = 0

  for (const [key, value] of Object.entries(receiver)) {
    if (!key.endsWith('_configs') || !Array.isArray(value) || value.length === 0) {
      continue
    }
    integrationTypes.push(key)
    integrationCount += value.length
  }

  return {
    name: String(receiver.name ?? ''),
    integration_count: integrationCount,
    integration_types: integrationTypes.sort(),
  }
}

function buildContract(config, component) {
  const route = config.route
  if (!route || typeof route !== 'object' || Array.isArray(route)) {
    throw new Error('Alertmanager config is missing route object')
  }

  const routes = collectRoutes(route)
  for (const item of routes) {
    item.matchesComponent = routeMatchesComponent({ matchers: item.matchers }, component)
  }

  const selected = routes.find((item) => item.matchesComponent) ?? routes[0]
  const receivers = Array.isArray(config.receivers)
    ? config.receivers
        .filter((receiver) => receiver && typeof receiver === 'object' && !Array.isArray(receiver))
        .map(integrationSummary)
        .filter((receiver) => receiver.name)
    : []

  return {
    routes: [
      {
        receiver: selected.receiver,
        matchers: selected.matchers,
        source_path: selected.path,
      },
    ],
    receivers,
    selected_component: component,
    secret_policy:
      'receiver settings, webhook URLs, bearer tokens, passwords, and raw Alertmanager config are omitted',
  }
}

function writeOutput(outputPath, payload) {
  const serialized = `${JSON.stringify(payload, null, 2)}\n`
  if (outputPath === '-') {
    process.stdout.write(serialized)
    return
  }
  fs.mkdirSync(path.dirname(outputPath), { recursive: true })
  fs.writeFileSync(outputPath, serialized, { mode: 0o600 })
}

try {
  const args = parseArgs(process.argv)
  const config = parseConfig(readInput(args.configFile))
  const contract = buildContract(config, args.component)
  writeOutput(args.output, contract)
  if (args.output !== '-') {
    process.stdout.write(`Wrote sanitized Alertmanager route contract: ${args.output}\n`)
  }
} catch (error) {
  process.stderr.write(`ERROR: ${error.message}\n`)
  process.exit(1)
}
