import fs from 'node:fs'
import path from 'node:path'
import { createRequire } from 'node:module'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

const require = createRequire(import.meta.url)
const { runCoverageGate } = require('../../../scripts/check-critical-coverage.cjs') as {
  runCoverageGate: (options?: {
    coveragePath?: string
    stdout?: NodeJS.WritableStream
    stderr?: NodeJS.WritableStream
  }) => number
}
const tmpDir = path.join(import.meta.dirname, '.tmp-coverage')

function writeXml(content: string): string {
  const filePath = path.join(tmpDir, 'cobertura-coverage.xml')
  fs.writeFileSync(filePath, content)
  return filePath
}

function runScript(coveragePath: string): { exitCode: number; stdout: string; stderr: string } {
  let stdout = ''
  let stderr = ''
  const stdoutStream = { write: (chunk: string) => void (stdout += chunk) } as NodeJS.WritableStream
  const stderrStream = { write: (chunk: string) => void (stderr += chunk) } as NodeJS.WritableStream
  const exitCode = runCoverageGate({
    coveragePath,
    stdout: stdoutStream,
    stderr: stderrStream,
  })

  return {
    exitCode,
    stdout,
    stderr,
  }
}

function createCoveredLines({
  hits,
  withBranches = false,
}: {
  hits: number
  withBranches?: boolean
}): string {
  return Array.from({ length: 10 }, (_, i) => {
    const branch = withBranches ? ' condition-coverage="100% (2/2)"' : ''
    return `<line number="${i + 1}" hits="${hits}"${branch}/>`
  }).join('\n')
}

describe('check-critical-coverage.cjs', () => {
  beforeEach(() => {
    fs.mkdirSync(tmpDir, { recursive: true })
  })

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true })
  })

  it('exits 2 when coverage file does not exist', () => {
    const result = runScript('/nonexistent/path.xml')
    expect(result.exitCode).toBe(2)
    expect(result.stderr).toContain('Coverage file not found')
  })

  it('exits 1 when no classes match target prefixes', () => {
    const xml = `<?xml version="1.0"?>
<coverage>
  <packages>
    <package>
      <classes>
        <class filename="server/src/other/foo.ts">
          <lines><line number="1" hits="1"/></lines>
        </class>
      </classes>
    </package>
  </packages>
</coverage>`
    const filePath = writeXml(xml)
    const result = runScript(filePath)
    expect(result.exitCode).toBe(1)
    expect(result.stdout + result.stderr).toContain('no matched files')
  })

  it('passes when active app/shared critical coverage meets thresholds', () => {
    const lines = createCoveredLines({ hits: 1, withBranches: true })
    const xml = `<?xml version="1.0"?>
<coverage>
  <packages>
    <package>
      <classes>
        <class filename="shared/turn-builder.ts">
          <lines>${lines}</lines>
        </class>
        <class filename="src/app/shared/api/legacy.ts">
          <lines>${lines}</lines>
        </class>
      </classes>
    </package>
  </packages>
</coverage>`
    const filePath = writeXml(xml)
    const result = runScript(filePath)
    expect(result.exitCode).toBe(0)
    expect(result.stdout).toContain('passed')
  })

  it('exits 1 when line coverage is below threshold', () => {
    const coveredLines = Array.from(
      { length: 8 },
      (_, i) => `<line number="${i + 1}" hits="1"/>`
    ).join('\n')
    const uncoveredLines = Array.from(
      { length: 2 },
      (_, i) => `<line number="${i + 9}" hits="0"/>`
    ).join('\n')
    const xml = `<?xml version="1.0"?>
<coverage>
  <packages>
    <package>
      <classes>
        <class filename="shared/turn-builder.ts">
          <lines>${coveredLines}${uncoveredLines}</lines>
        </class>
        <class filename="src/app/shared/api/legacy.ts">
          <lines>${coveredLines}${uncoveredLines}</lines>
        </class>
      </classes>
    </package>
  </packages>
</coverage>`
    const filePath = writeXml(xml)
    const result = runScript(filePath)
    expect(result.exitCode).toBe(1)
    expect(result.stdout + result.stderr).toContain('FAILED')
  })

  it('exits 1 when branch coverage is below threshold', () => {
    const lines = Array.from({ length: 10 }, (_, i) => {
      return `<line number="${i + 1}" hits="1" condition-coverage="50% (1/2)"/>`
    }).join('\n')
    const xml = `<?xml version="1.0"?>
<coverage>
  <packages>
    <package>
      <classes>
        <class filename="shared/turn-builder.ts">
          <lines>${lines}</lines>
        </class>
        <class filename="src/app/shared/api/legacy.ts">
          <lines>${lines}</lines>
        </class>
      </classes>
    </package>
  </packages>
</coverage>`
    const filePath = writeXml(xml)
    const result = runScript(filePath)
    expect(result.exitCode).toBe(1)
    expect(result.stdout + result.stderr).toContain('FAILED')
  })
})
