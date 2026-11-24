#!/usr/bin/env node

import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import {
  validateComposeBaseline,
  validateComposePublishedPorts,
  validateDockerfileBaseline,
} from './lib/docker-baseline-policy.js'

const scriptDir = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.resolve(scriptDir, '..')

const dockerDir = path.join(repoRoot, 'docker')
const composePath = path.join(dockerDir, 'compose.yml')
const dockerfilePath = path.join(dockerDir, 'Dockerfile')

function readFileOrDie(targetPath) {
  try {
    return fs.readFileSync(targetPath, 'utf8')
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    console.error(`[docker-baseline] ERROR: unable to read ${targetPath}: ${message}`)
    process.exit(2)
  }
}

const composeContent = readFileOrDie(composePath)
const dockerfileContent = readFileOrDie(dockerfilePath)
const composeFiles = fs
  .readdirSync(dockerDir)
  .filter((name) => /^compose(\.[A-Za-z0-9_-]+)*\.ya?ml$/.test(name))
  .sort()

const errors = [
  ...validateComposeBaseline(composeContent),
  ...composeFiles.flatMap((name) =>
    validateComposePublishedPorts(readFileOrDie(path.join(dockerDir, name)), `docker/${name}`)
  ),
  ...validateDockerfileBaseline(dockerfileContent),
]

if (errors.length > 0) {
  console.error('[docker-baseline] ERROR: baseline policy check failed')
  for (const error of errors) {
    console.error(`- ${error}`)
  }
  process.exit(1)
}

console.log('[docker-baseline] Docker baseline policy check passed')
