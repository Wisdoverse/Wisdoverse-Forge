const CORE_SERVICES = ['agentforge-server', 'orchestrator']
const HEALTH_ENDPOINTS = {
  'agentforge-server': '/health',
  orchestrator: '/health',
}

function extractServiceBlock(composeContent, serviceName) {
  const lines = composeContent.split(/\r?\n/)
  const header = `  ${serviceName}:`
  const serviceHeaderPattern = /^ {2}[A-Za-z0-9_.-]+:\s*$/

  const startIndex = lines.findIndex((line) => line.trimEnd() === header)
  if (startIndex === -1) {
    return null
  }

  let endIndex = lines.length
  for (let index = startIndex + 1; index < lines.length; index += 1) {
    const line = lines[index]
    if (line.trim() === '') {
      continue
    }
    if (/^[^ ]/.test(line)) {
      endIndex = index
      break
    }
    if (serviceHeaderPattern.test(line)) {
      endIndex = index
      break
    }
  }

  return lines.slice(startIndex, endIndex).join('\n')
}

function hasNestedKey(block, ...keys) {
  return keys.every((key) => block.includes(`${key}:`))
}

function validateServiceCommon(serviceName, block) {
  const errors = []
  const prefix = `services.${serviceName}`

  if (!block.includes('security_opt:') || !/-\s*no-new-privileges:true/.test(block)) {
    errors.push(`${prefix} missing required control: security_opt no-new-privileges:true`)
  }

  if (!block.includes('cap_drop:') || !/-\s*ALL/.test(block)) {
    errors.push(`${prefix} missing required control: cap_drop ALL`)
  }

  if (!/^\s*read_only:\s*true\s*$/m.test(block)) {
    errors.push(`${prefix} missing required control: read_only: true`)
  }

  if (!block.includes('tmpfs:') || !/-\s*\/tmp(?:\s|$)/.test(block)) {
    errors.push(`${prefix} missing required control: tmpfs /tmp`)
  }

  if (!hasNestedKey(block, 'deploy', 'resources', 'limits')) {
    errors.push(`${prefix} missing required control: deploy.resources.limits`)
  }

  if (!/^\s*pids(?:_limit)?:\s*.+$/m.test(block)) {
    errors.push(
      `${prefix} missing required control: pids limit (deploy.resources.limits.pids or pids_limit)`
    )
  }

  if (!block.includes('logging:') || !block.includes('max-size:') || !block.includes('max-file:')) {
    errors.push(`${prefix} missing required control: bounded logging (max-size/max-file)`)
  }

  if (!block.includes('healthcheck:')) {
    errors.push(`${prefix} missing required control: healthcheck`)
  }

  return errors
}

function validateAgentforgeRustStorage(composeContent, block) {
  const errors = []
  const prefix = 'services.agentforge-server'

  if (!block.includes('STORAGE_PROVIDER=${STORAGE_PROVIDER:-local}')) {
    errors.push(`${prefix} must pass STORAGE_PROVIDER into the container`)
  }

  if (!block.includes('STORAGE_LOCAL_PATH=${STORAGE_LOCAL_PATH:-/var/lib/agentforge/uploads}')) {
    errors.push(`${prefix} must set a container-safe STORAGE_LOCAL_PATH default`)
  }

  if (
    !/-\s*agentforge-uploads:\$\{STORAGE_LOCAL_PATH:-\/var\/lib\/agentforge\/uploads\}/.test(block)
  ) {
    errors.push(`${prefix} must mount writable local attachment storage`)
  }

  if (!/^\s{2}agentforge-uploads:\s*$/m.test(composeContent)) {
    errors.push('volumes.agentforge-uploads must be declared')
  }

  return errors
}

export function validateComposeBaseline(composeContent) {
  const errors = []

  for (const serviceName of CORE_SERVICES) {
    const block = extractServiceBlock(composeContent, serviceName)
    if (!block) {
      errors.push(`services.${serviceName} block not found`)
      continue
    }

    errors.push(...validateServiceCommon(serviceName, block))
    if (serviceName === 'agentforge-server') {
      errors.push(...validateAgentforgeRustStorage(composeContent, block))
    }

    const expectedHealth = HEALTH_ENDPOINTS[serviceName]
    if (expectedHealth && !block.includes(expectedHealth)) {
      errors.push(`services.${serviceName} healthcheck must use ${expectedHealth}`)
    }
  }

  return errors
}

export function validateDockerfileBaseline(dockerfileContent) {
  const errors = []

  if (!/FROM\s+base\s+AS\s+production\b/.test(dockerfileContent)) {
    errors.push('docker/Dockerfile missing production stage')
  }

  if (!/^\s*USER\s+agentforge\s*$/m.test(dockerfileContent)) {
    errors.push('docker/Dockerfile production stage must run as non-root USER agentforge')
  }

  if (
    !/ENTRYPOINT\s+\["\/usr\/bin\/tini",\s*"--",\s*"\/entrypoint\.sh"\]/.test(dockerfileContent)
  ) {
    errors.push('docker/Dockerfile must use tini + entrypoint for PID 1 signal handling')
  }

  if (!/HEALTHCHECK[\s\S]*\/health\/live/.test(dockerfileContent)) {
    errors.push('docker/Dockerfile healthcheck must target /health/live')
  }

  return errors
}
