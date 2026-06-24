#!/usr/bin/env node

import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import {
  validateComposeBaseline,
  validateDockerfileBaseline,
} from './lib/docker-baseline-policy.js'

const scriptDir = path.dirname(fileURLToPath(import.meta.url))
const repoRoot = path.resolve(scriptDir, '..')

const composePath = path.join(repoRoot, 'docker', 'compose.yml')
const dockerfilePath = path.join(repoRoot, 'docker', 'Dockerfile')

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

const errors = [
  ...validateComposeBaseline(composeContent),
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
