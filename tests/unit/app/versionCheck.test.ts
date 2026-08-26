import { describe, expect, test } from 'vitest'
import { isNewerVersion, parseVersion } from '@app/shared/lib/versionCheck'

describe('versionCheck', () => {
  test('parses tagged and bare versions, ignoring prefixes and junk', () => {
    expect(parseVersion('v1.2.3')).toEqual({ major: 1, minor: 2, patch: 3 })
    expect(parseVersion('1.2.3')).toEqual({ major: 1, minor: 2, patch: 3 })
    expect(parseVersion('release-1.2.3-alpha.1+build')).toEqual({ major: 1, minor: 2, patch: 3 })
    expect(parseVersion('not-a-version')).toBeNull()
    expect(parseVersion(null)).toBeNull()
    expect(parseVersion('')).toBeNull()
    expect(parseVersion(42)).toBeNull()
  })

  test('orders versions strictly', () => {
    expect(isNewerVersion('v2.0.0', 'v1.9.9')).toBe(true)
    expect(isNewerVersion('v1.0.1', 'v1.0.0')).toBe(true)
    expect(isNewerVersion('v1.8.0', 'v1.9.0')).toBe(false)
    expect(isNewerVersion('v0.1.15', 'v0.1.15')).toBe(false)
    expect(isNewerVersion('garbage', 'v0.1.15')).toBe(false)
    expect(isNewerVersion(null, 'v0.1.15')).toBe(false)
  })
})
