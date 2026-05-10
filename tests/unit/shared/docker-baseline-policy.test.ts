import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'
import {
  validateComposeBaseline,
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
