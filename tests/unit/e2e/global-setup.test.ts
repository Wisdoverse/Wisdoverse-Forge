import { lstat, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises'
import os from 'node:os'
import path from 'node:path'
import { expect, it } from 'vitest'
import { writePrivateStorageState } from '../../e2e/global-setup'

it('writes auth state privately, repairs existing modes, and rejects symlinks', async () => {
  const tempDir = await mkdtemp(path.join(os.tmpdir(), 'forge-auth-state-'))
  const authDir = path.join(tempDir, '.auth')
  const statePath = path.join(authDir, 'user.json')
  const targetPath = path.join(tempDir, 'target')

  try {
    await mkdir(authDir, { mode: 0o755 })
    await writeFile(statePath, 'old credentials', { mode: 0o644 })
    await writePrivateStorageState(statePath, 'new credentials')

    expect(await readFile(statePath, 'utf8')).toBe('new credentials')
    if (process.platform !== 'win32') {
      expect((await lstat(authDir)).mode & 0o777).toBe(0o700)
      expect((await lstat(statePath)).mode & 0o777).toBe(0o600)
    }

    await writeFile(targetPath, 'untouched')
    await rm(statePath)
    try {
      await symlink(targetPath, statePath)
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code
      if (
        process.platform !== 'win32' ||
        !['EPERM', 'EACCES', 'ENOTSUP', 'EOPNOTSUPP'].includes(code ?? '')
      ) {
        throw error
      }
      return // Windows may deny symlink creation without Developer Mode/elevation.
    }

    await expect(writePrivateStorageState(statePath, 'secret')).rejects.toThrow(
      'E2E auth state must be a regular file'
    )
    expect(await readFile(targetPath, 'utf8')).toBe('untouched')
  } finally {
    await rm(tempDir, { recursive: true, force: true })
  }
})
