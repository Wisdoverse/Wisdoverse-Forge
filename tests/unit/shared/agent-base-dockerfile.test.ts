import fs from 'node:fs'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const projectRoot = path.resolve(import.meta.dirname, '../../..')
const agentBaseDockerfile = fs.readFileSync(
  path.join(projectRoot, 'docker/Dockerfile.agent-base'),
  'utf8'
)

describe('agent base Dockerfile', () => {
  it('builds Docker Compose from pinned source using a patched Go toolchain', () => {
    expect(agentBaseDockerfile).toContain('ARG COMPOSE_GO_VERSION=1.26.4')
    expect(agentBaseDockerfile).toContain('ARG COMPOSE_VERSION=5.1.3')
    expect(agentBaseDockerfile).toContain(
      'ARG COMPOSE_COMMIT=977a4310f9f6d89d4b176fee01a5b7c109c1816a'
    )
    expect(agentBaseDockerfile).toContain(
      'ARG COMPOSE_SOURCE_SHA256=01a3d4c2cd44105c29dce266cb6e021381c8ed959219eb05922f958c63ea27db'
    )
    expect(agentBaseDockerfile).toContain('ARG COMPOSE_DOCKER_MODULE_VERSION=29.3.1')
    expect(agentBaseDockerfile).toContain('ARG COMPOSE_IN_TOTO_VERSION=0.11.0')
    expect(agentBaseDockerfile).toContain('ARG COMPOSE_OTEL_VERSION=1.43.0')
    expect(agentBaseDockerfile).toContain(
      'FROM golang:${COMPOSE_GO_VERSION}-bookworm AS compose-builder'
    )
    expect(agentBaseDockerfile).toContain('compose/archive/${COMPOSE_COMMIT}.tar.gz')
    expect(agentBaseDockerfile).toContain('third_party/docker-compat/pkg/namesgenerator')
    expect(agentBaseDockerfile).toContain(
      'go mod edit -require="github.com/docker/docker@v${COMPOSE_DOCKER_MODULE_VERSION}+incompatible"'
    )
    expect(agentBaseDockerfile).toContain(
      'go mod edit -replace="github.com/docker/docker=./third_party/docker-compat"'
    )
    expect(agentBaseDockerfile).toContain(
      '"github.com/in-toto/in-toto-golang@v${COMPOSE_IN_TOTO_VERSION}"'
    )
    expect(agentBaseDockerfile).toContain('"go.opentelemetry.io/otel/sdk@v${COMPOSE_OTEL_VERSION}"')
    expect(agentBaseDockerfile).toContain(
      'go list -m all | awk \'$1 == "github.com/docker/docker" && $3 == "=>" && $4 == "./third_party/docker-compat" { print $2 }\''
    )
    expect(agentBaseDockerfile).toContain('= "v${COMPOSE_DOCKER_MODULE_VERSION}+incompatible"')
    expect(agentBaseDockerfile).toContain('go version -m /out/docker-compose')
    expect(agentBaseDockerfile).toContain('grep -q "go${COMPOSE_GO_VERSION}"')
    expect(agentBaseDockerfile).not.toContain('docker/compose/releases/download')
  })
})
