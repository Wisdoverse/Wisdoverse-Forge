import fs from 'node:fs'
import path from 'node:path'
import yaml from 'yaml'
import { describe, expect, it } from 'vitest'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const baseComposePath = path.join(projectRoot, 'docker/compose.yml')
const devComposePath = path.join(projectRoot, 'docker/compose.dev.yml')
const externalComposePath = path.join(projectRoot, 'docker/compose.external.yml')
const prodComposePath = path.join(projectRoot, 'docker/compose.prod.yml')

const legacyReferencePatterns = ['http://agentforge:', 'platform:50052']
const legacyServiceNames = ['agentforge', 'platform', 'orchestrator-legacy']
const helperServiceNames = ['agentforge-mcp', 'platform-runtime']

function parseCompose(filePath: string): { services?: Record<string, unknown> } {
  const parsed = yaml.parse(fs.readFileSync(filePath, 'utf8')) as {
    services?: Record<string, unknown>
  }

  expect(parsed).toBeTruthy()
  return parsed
}

function collectStrings(value: unknown, acc: string[] = []): string[] {
  if (typeof value === 'string') {
    acc.push(value)
    return acc
  }

  if (Array.isArray(value)) {
    for (const item of value) {
      collectStrings(item, acc)
    }
    return acc
  }

  if (value && typeof value === 'object') {
    for (const nested of Object.values(value as Record<string, unknown>)) {
      collectStrings(nested, acc)
    }
  }

  return acc
}

function expectDependsOnService(value: unknown, serviceName: string): void {
  if (Array.isArray(value)) {
    expect(value).toContain(serviceName)
    return
  }

  if (value && typeof value === 'object') {
    expect(value).toHaveProperty(serviceName)
    return
  }

  throw new Error('missing depends_on service ' + serviceName)
}

function expectNotDependsOnService(value: unknown, serviceName: string): void {
  if (Array.isArray(value)) {
    expect(value).not.toContain(serviceName)
    return
  }

  if (value && typeof value === 'object') {
    expect(value).not.toHaveProperty(serviceName)
    return
  }
}

function assertOverlayDefaults(filePath: string, expectedDependsOn: string[]): void {
  const compose = parseCompose(filePath)
  const services = compose.services ?? {}

  expect(services).toHaveProperty('agentforge-server')
  expect(services).toHaveProperty('orchestrator')
  for (const legacyService of legacyServiceNames) {
    expect(services).not.toHaveProperty(legacyService)
  }
  for (const helperService of helperServiceNames) {
    expect(services).not.toHaveProperty(helperService)
  }

  const orchestrator = services.orchestrator as { depends_on?: unknown }
  expect(orchestrator).toHaveProperty('depends_on')

  const dependsOn = orchestrator.depends_on
  for (const dependency of expectedDependsOn) {
    expectDependsOnService(dependsOn, dependency)
  }

  for (const [serviceName, service] of Object.entries(services)) {
    const strings = collectStrings(service)
    for (const pattern of legacyReferencePatterns) {
      expect(
        strings.some((value) => value.includes(pattern)),
        `${filePath}:${serviceName}`
      ).toBe(false)
    }

    const serviceDependsOn = (service as { depends_on?: unknown })?.depends_on
    for (const legacyService of legacyServiceNames) {
      expectNotDependsOnService(serviceDependsOn, legacyService)
    }
    for (const helperService of helperServiceNames) {
      expectNotDependsOnService(serviceDependsOn, helperService)
    }
  }
}

function assertMcpEndpoint(filePath: string, expectedEndpoint: string): void {
  const compose = parseCompose(filePath)
  const services = compose.services ?? {}
  const orchestrator = services.orchestrator as { environment?: unknown }
  const strings = collectStrings(orchestrator?.environment)

  expect(strings).toContain(expectedEndpoint)
}

describe('rust runtime defaults', () => {
  it('routes the default orchestrator MCP bridge through agentforge-server without any legacy runtime services', () => {
    const compose = parseCompose(baseComposePath)
    const services = compose.services ?? {}
    const orchestrator = services.orchestrator as { depends_on?: unknown; environment?: unknown }
    const rustApi = services['agentforge-server'] as { depends_on?: unknown; environment?: unknown }
    const postgres = services.postgres as { volumes?: unknown }

    expect(services).toHaveProperty('agentforge-server')
    for (const legacyService of legacyServiceNames) {
      expect(services).not.toHaveProperty(legacyService)
    }
    for (const helperService of helperServiceNames) {
      expect(services).not.toHaveProperty(helperService)
    }
    assertMcpEndpoint(baseComposePath, 'ORCHESTRATOR_MCP_ENDPOINT=http://agentforge-server:${SERVER_PORT:-4003}/mcp')
    expectDependsOnService(orchestrator.depends_on, 'agentforge-server')
    for (const legacyService of legacyServiceNames) {
      expectNotDependsOnService(orchestrator.depends_on, legacyService)
    }
    for (const helperService of helperServiceNames) {
      expectNotDependsOnService(orchestrator.depends_on, helperService)
      expectNotDependsOnService(rustApi.depends_on, helperService)
    }

    const rustApiStrings = collectStrings(rustApi)
    const postgresStrings = collectStrings(postgres)
    expect(rustApiStrings).not.toContain('PLATFORM_GRPC_ADDRESS=platform-runtime:50052')
    expect(postgresStrings).not.toContain('../server/src/migrations/init:/docker-entrypoint-initdb.d:ro')
  })

  it('routes the dev overlay through agentforge-server without any legacy runtime services', () => {
    assertOverlayDefaults(devComposePath, ['orchestrator-db', 'temporal'])
  })

  it('uses the bundled Temporal dynamic config path', () => {
    const compose = parseCompose(baseComposePath)
    const services = compose.services ?? {}
    const temporal = services.temporal as { environment?: unknown }
    const strings = collectStrings(temporal.environment)

    expect(strings).toContain(
      'DYNAMIC_CONFIG_FILE_PATH=/etc/temporal/config/dynamicconfig/docker.yaml'
    )
    expect(strings).not.toContain('DYNAMIC_CONFIG_FILE_PATH=config/dynamicconfig/development-sql.yaml')
  })

  it('routes the external overlay through agentforge-server without any legacy runtime services', () => {
    assertOverlayDefaults(externalComposePath, ['temporal-ext'])
    assertMcpEndpoint(externalComposePath, 'ORCHESTRATOR_MCP_ENDPOINT=http://agentforge-server:${SERVER_PORT:-4003}/mcp')
  })

  it('routes the prod overlay through agentforge-server without any legacy runtime services', () => {
    const compose = parseCompose(prodComposePath)
    const services = compose.services ?? {}
    const rustApi = services['agentforge-server'] as { depends_on?: unknown; environment?: unknown }

    expect(services).toHaveProperty('agentforge-server')
    expect(services).toHaveProperty('backup')
    for (const legacyService of legacyServiceNames) {
      expect(services).not.toHaveProperty(legacyService)
    }
    for (const helperService of helperServiceNames) {
      expect(services).not.toHaveProperty(helperService)
    }

    expectDependsOnService(rustApi.depends_on, 'db')
    expectDependsOnService(rustApi.depends_on, 'redis')

    const rustApiStrings = collectStrings(rustApi.environment)
    expect(rustApiStrings).toContain(
      'DATABASE_URL=postgres://agentforge:${POSTGRES_PASSWORD:?POSTGRES_PASSWORD is required in production}@db:5432/agentforge'
    )
    expect(rustApiStrings).toContain(
      'REDIS_URL=redis://:${REDIS_PASSWORD:?REDIS_PASSWORD is required in production}@redis:6379'
    )
  })
})
