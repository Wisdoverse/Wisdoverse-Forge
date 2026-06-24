import { describe, expect, test } from 'vitest'
import {
  SETTINGS_DEFAULT_SECTION,
  normalizeSettingsSection,
} from '@app/shared/model/settings.store'

describe('settings store routing helpers', () => {
  test('normalizes canonical settings sections', () => {
    expect(normalizeSettingsSection('projects')).toBe('projects')
    expect(normalizeSettingsSection('runtime')).toBe('runtime')
    expect(normalizeSettingsSection(SETTINGS_DEFAULT_SECTION)).toBe('providers')
  })

  test('normalizes common settings URL aliases', () => {
    expect(normalizeSettingsSection('api-keys')).toBe('keys')
    expect(normalizeSettingsSection('profile')).toBe('account')
    expect(normalizeSettingsSection('git')).toBe('git-credentials')
    expect(normalizeSettingsSection('ssh')).toBe('ssh-keys')
    expect(normalizeSettingsSection('workspace')).toBe('resources')
  })

  test('rejects unknown settings sections', () => {
    expect(normalizeSettingsSection('missing-section')).toBeNull()
    expect(normalizeSettingsSection('')).toBeNull()
  })
})
