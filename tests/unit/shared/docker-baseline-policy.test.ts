import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import {
  listComposeFiles,
  selectComposeFiles,
  validateComposeBaseline,
  validateComposePublishedPorts,
  validateDockerfileBaseline,
} from '../../../scripts/lib/docker-baseline-policy.js'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const composePath = path.join(projectRoot, 'docker/compose.yml')
const dockerfilePath = path.join(projectRoot, 'docker/Dockerfile')

describe('docker baseline policy', () => {
  it('validates current docker assets against baseline controls', () => {
    const composeContent = fs.readFileSync(composePath, 'utf8')
    const dockerfileContent = fs.readFileSync(dockerfilePath, 'utf8')

    expect(validateComposeBaseline(composeContent)).toEqual([])
    expect(validateDockerfileBaseline(dockerfileContent)).toEqual([])
  })

  it('requires a loopback host address on every published port in every tracked Compose file', () => {
    const composeFiles = listComposeFiles(projectRoot)
    expect(composeFiles).toEqual(
      expect.arrayContaining(['docker/compose.yml', 'docker/compose.dev.yml'])
    )
    for (const file of composeFiles) {
      const content = fs.readFileSync(path.join(projectRoot, file), 'utf8')
      expect(validateComposePublishedPorts(content, file)).toEqual([])
    }
  })

  describe('Compose file discovery', () => {
    it('selects compose*.y*ml and docker-compose*.y*ml anywhere in the repository', () => {
      const paths = [
        'docker/compose.yml',
        'docker/compose.dev.yml',
        'deploy/edge/docker-compose.override.yaml',
        'compose.yaml',
        'docker/nats.conf',
        'docs/compose-guide.md',
        'tests/fixtures/not-compose.yml',
      ]
      expect(selectComposeFiles(paths)).toEqual([
        'compose.yaml',
        'deploy/edge/docker-compose.override.yaml',
        'docker/compose.dev.yml',
        'docker/compose.yml',
      ])
    })

    it('finds a tracked Compose file outside docker/', () => {
      const repo = fs.mkdtempSync(path.join(os.tmpdir(), 'compose-discovery-'))
      try {
        fs.mkdirSync(path.join(repo, 'deploy'))
        fs.writeFileSync(path.join(repo, 'deploy/compose.yaml'), 'services: {}\n')
        fs.writeFileSync(path.join(repo, 'untracked-compose.yml'), 'services: {}\n')
        execFileSync('git', ['init', '-q'], { cwd: repo })
        execFileSync('git', ['add', 'deploy/compose.yaml'], { cwd: repo })
        expect(listComposeFiles(repo)).toEqual(['deploy/compose.yaml'])
      } finally {
        fs.rmSync(repo, { recursive: true, force: true })
      }
    })
  })

  describe('published port host addresses', () => {
    const portsOf = (...ports: string[]) =>
      `services:\n  nats:\n    ports:\n${ports.map((port) => `      - ${port}\n`).join('')}`

    it('flags a published port without a host address', () => {
      const errors = validateComposePublishedPorts(portsOf("'4222:4222'"), 'docker/x.yml')
      expect(errors).toEqual([
        expect.stringContaining("docker/x.yml services.nats.ports[0] '4222:4222'"),
      ])
    })

    it('flags a container-only port, which Docker publishes on a random port of every address', () => {
      expect(validateComposePublishedPorts(portsOf('4222'))).toHaveLength(1)
    })

    it.each([
      ["'${X:-127.0.0.1}:4222:4222'", 'variable with a loopback default'],
      ["'127.0.0.1:${NATS_PORT:-4222}:4222/tcp'", 'literal loopback with a protocol suffix'],
      ["'127.0.0.1:5353:5353/udp'", 'udp suffix'],
      ["'127.0.0.2:8000-8010:8000-8010'", 'port range on 127.0.0.0/8'],
      ["'[::1]:8222:8222'", 'bracketed IPv6 loopback'],
      ["'[::1]:9000-9001:9000-9001/tcp'", 'bracketed IPv6 with range and suffix'],
      ["'${X-127.0.0.1}:4222:4222'", 'default without a colon'],
      ["'${A:-${B:-127.0.0.1}}:4222:4222'", 'nested defaults'],
      ["'127.0.0.1::4222'", 'random host port on loopback'],
    ])('accepts %s (%s)', (entry) => {
      expect(validateComposePublishedPorts(portsOf(entry))).toEqual([])
    })

    it.each([
      ["'${NATS_BIND_ADDRESS}:4222:4222'", 'variable without a default'],
      ["'${NATS_BIND_ADDRESS:?set it}:4222:4222'", 'required variable without a default'],
      ["'${X:-localhost}:4222:4222'", 'hostname default'],
      ["'${X:-0.0.0.0}:4222:4222'", 'all-IPv4 default'],
      ["'${A:-${B:-0.0.0.0}}:4222:4222'", 'nested non-loopback default'],
      ["'192.0.2.10:4222:4222'", 'literal non-loopback address'],
      ["'0.0.0.0:8000-8010:8000-8010'", 'all-IPv4 range'],
      ["'[::]:4222:4222'", 'all-IPv6 address'],
      ["'[2001:db8::10]:4222:4222'", 'literal non-loopback IPv6'],
    ])('flags %s (%s)', (entry) => {
      expect(validateComposePublishedPorts(portsOf(entry))).toEqual([
        expect.stringContaining('must publish on a loopback host address by default'),
      ])
    })

    it('checks host_ip in long syntax, including a variable default', () => {
      const longPort = (hostIp?: string) =>
        `services:\n  nats:\n    ports:\n      - target: 4222\n        published: 4222\n${
          hostIp === undefined ? '' : `        host_ip: '${hostIp}'\n`
        }`
      expect(validateComposePublishedPorts(longPort())).toEqual([
        expect.stringContaining('services.nats.ports[0]'),
      ])
      expect(validateComposePublishedPorts(longPort('${NATS_BIND_ADDRESS:-127.0.0.1}'))).toEqual([])
      expect(validateComposePublishedPorts(longPort('::1'))).toEqual([])
      expect(validateComposePublishedPorts(longPort('${NATS_BIND_ADDRESS:-0.0.0.0}'))).toHaveLength(
        1
      )
      expect(validateComposePublishedPorts(longPort('${NATS_BIND_ADDRESS}'))).toHaveLength(1)
    })

    it('allows only the reverse proxy to keep the all-addresses default', () => {
      const content =
        "services:\n  caddy:\n    ports:\n      - '80:80'\n  temporal:\n    ports:\n      - '8233:8233'\n"
      expect(validateComposePublishedPorts(content)).toEqual([
        expect.stringContaining('services.temporal.ports[0]'),
      ])
    })
  })

  it('flags missing hardening controls for core services', () => {
    const composeContent = `
services:
  agentforge-server:
    image: example/app:latest
  orchestrator:
    image: example/orchestrator:latest
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    healthcheck:
      test: ['CMD', 'curl', '-f', 'http://localhost:4003/health']
`

    const errors = validateComposeBaseline(composeContent)
    expect(errors).toEqual(
      expect.arrayContaining([
        expect.stringContaining('services.agentforge-server'),
        expect.stringContaining('services.orchestrator'),
        expect.stringContaining('read_only: true'),
        expect.stringContaining('tmpfs'),
        expect.stringContaining('/health'),
      ])
    )
  })

  it('flags missing writable attachment storage for the read-only Rust API container', () => {
    const composeContent = `
services:
  agentforge-server:
    image: example/app:latest
    read_only: true
    tmpfs:
      - /tmp
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    deploy:
      resources:
        limits:
          pids: 256
    logging:
      options:
        max-size: '10m'
        max-file: '5'
    healthcheck:
      test: ['CMD', 'curl', '-sf', 'http://localhost:4003/health']
  orchestrator:
    image: example/orchestrator:latest
    read_only: true
    tmpfs:
      - /tmp
    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL
    deploy:
      resources:
        limits:
          pids: 256
    logging:
      options:
        max-size: '10m'
        max-file: '5'
    healthcheck:
      test: ['CMD', 'curl', '-sf', 'http://localhost:4010/health']
`

    const errors = validateComposeBaseline(composeContent)
    expect(errors).toEqual(
      expect.arrayContaining([
        'services.agentforge-server must pass STORAGE_PROVIDER into the container',
        'services.agentforge-server must set a container-safe STORAGE_LOCAL_PATH default',
        'services.agentforge-server must mount writable local attachment storage',
        'volumes.agentforge-uploads must be declared',
      ])
    )
  })

  // ===========================================================================
  // validateDockerfileBaseline negative-path tests
  // ===========================================================================

  it('flags Dockerfile missing production stage', () => {
    const dockerfile = `
FROM node:24-slim AS base
RUN apt-get update
USER agentforge
ENTRYPOINT ["/usr/bin/tini", "--", "/entrypoint.sh"]
HEALTHCHECK CMD curl -sf http://localhost:4003/health/live || exit 1
`
    const errors = validateDockerfileBaseline(dockerfile)
    expect(errors).toEqual(
      expect.arrayContaining([expect.stringContaining('missing production stage')])
    )
  })

  it('flags Dockerfile missing non-root USER', () => {
    const dockerfile = `
FROM base AS production
ENTRYPOINT ["/usr/bin/tini", "--", "/entrypoint.sh"]
HEALTHCHECK CMD curl -sf http://localhost:4003/health/live || exit 1
`
    const errors = validateDockerfileBaseline(dockerfile)
    expect(errors).toEqual(
      expect.arrayContaining([expect.stringContaining('non-root USER agentforge')])
    )
  })

  it('flags Dockerfile missing tini entrypoint', () => {
    const dockerfile = `
FROM base AS production
USER agentforge
ENTRYPOINT ["node", "server.js"]
HEALTHCHECK CMD curl -sf http://localhost:4003/health/live || exit 1
`
    const errors = validateDockerfileBaseline(dockerfile)
    expect(errors).toEqual(
      expect.arrayContaining([expect.stringContaining('tini + entrypoint for PID 1')])
    )
  })

  it('flags Dockerfile missing healthcheck /health/live', () => {
    const dockerfile = `
FROM base AS production
USER agentforge
ENTRYPOINT ["/usr/bin/tini", "--", "/entrypoint.sh"]
`
    const errors = validateDockerfileBaseline(dockerfile)
    expect(errors).toEqual(
      expect.arrayContaining([expect.stringContaining('healthcheck must target /health/live')])
    )
  })

  it('returns all errors for completely empty Dockerfile', () => {
    const errors = validateDockerfileBaseline('')
    expect(errors.length).toBe(4)
  })

  // ===========================================================================
  // extractServiceBlock parser boundary tests (via validateComposeBaseline)
  // ===========================================================================

  it('extracts service block when services are separated by blank lines', () => {
    const composeContent = `services:
  agentforge-server:
    image: example/app:latest

    security_opt:
      - no-new-privileges:true
    cap_drop:
      - ALL

  orchestrator:
    image: example/orchestrator:latest
`
    const errors = validateComposeBaseline(composeContent)
    // Both services found (no "block not found" errors)
    expect(errors.every((e) => !e.includes('block not found'))).toBe(true)
  })

  it('extracts service block when terminated by top-level key', () => {
    const composeContent = `services:
  agentforge-server:
    image: example/app:latest
  orchestrator:
    image: example/orchestrator:latest
volumes:
  data:
`
    const errors = validateComposeBaseline(composeContent)
    expect(errors.every((e) => !e.includes('block not found'))).toBe(true)
  })

  it('reports block-not-found when core service is missing', () => {
    const composeContent = `services:
  orchestrator:
    image: example/orchestrator:latest
`
    const errors = validateComposeBaseline(composeContent)
    expect(errors).toContainEqual('services.agentforge-server block not found')
  })

  it('extracts last service when it ends at EOF', () => {
    const composeContent = `services:
  agentforge-server:
    image: example/app:latest
  orchestrator:
    image: example/orchestrator:latest`
    const errors = validateComposeBaseline(composeContent)
    expect(errors.every((e) => !e.includes('block not found'))).toBe(true)
  })
})
