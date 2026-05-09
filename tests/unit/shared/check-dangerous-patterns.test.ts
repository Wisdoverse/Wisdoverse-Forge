import fs from 'node:fs'
import path from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'

import { runDangerousPatternScan } from '../../../scripts/check-dangerous-patterns.mjs'

const tmpDir = path.join(import.meta.dirname, '.tmp-dangerous-patterns')

function writeFile(relPath: string, content: string): void {
  const filePath = path.join(tmpDir, relPath)
  fs.mkdirSync(path.dirname(filePath), { recursive: true })
  fs.writeFileSync(filePath, content)
}

function runScript(): { exitCode: number; stdout: string; stderr: string } {
  let stdout = ''
  let stderr = ''
  const stdoutStream = { write: (chunk: string) => void (stdout += chunk) } as NodeJS.WritableStream
  const stderrStream = { write: (chunk: string) => void (stderr += chunk) } as NodeJS.WritableStream
  const exitCode = runDangerousPatternScan({
    cwd: tmpDir,
    stdout: stdoutStream,
    stderr: stderrStream,
  })

  return {
    exitCode,
    stdout,
    stderr,
  }
}

describe('check-dangerous-patterns.mjs', () => {
  beforeEach(() => {
    fs.mkdirSync(tmpDir, { recursive: true })
  })

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true })
  })

  it('fails on unapproved eval usage in active source roots', () => {
    writeFile('src/modules/a.ts', 'const x = eval("1+1")\n')

    const result = runScript()

    expect(result.exitCode).toBe(1)
    expect(result.stdout + result.stderr).toContain('eval(')
  })

  it('fails on new Function usage in scripts', () => {
    writeFile('scripts/build/tool.js', 'const x = new Function("return 1")\n')

    const result = runScript()

    expect(result.exitCode).toBe(1)
    expect(result.stdout + result.stderr).toContain('new Function()')
  })

  it('fails on unapproved interpolated SQL template in active script roots', () => {
    writeFile(
      'scripts/db/repo.ts',
      'await db.query(`SELECT * FROM users ORDER BY ${request.query.sort}`)\n'
    )

    const result = runScript()

    expect(result.exitCode).toBe(1)
    expect(result.stdout + result.stderr).toContain('Possible SQL injection')
  })

  it('ignores the legacy server tree by default', () => {
    writeFile('server/src/modules/a.ts', 'const x = eval("1+1")\n')

    const result = runScript()

    expect(result.exitCode).toBe(0)
    expect(result.stdout).toContain('No dangerous patterns found')
  })
})
