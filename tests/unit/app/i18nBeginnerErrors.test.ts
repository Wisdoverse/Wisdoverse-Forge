import { describe, expect, test } from 'vitest'
import { en } from '@app/shared/i18n/locales/en'
import { zh } from '@app/shared/i18n/locales/zh'

describe('beginner error translations', () => {
  test('English common errors lead with recovery steps', () => {
    expect(en.errors.network).toContain('Check your connection')
    expect(en.errors.serverError).toContain('try again')
    expect(en.errors.forbidden).toContain('Ask an owner or admin')
    expect(en.errors.agentError).toContain('check the agent status')
    expect(en.errors.agentError).not.toMatch(/^Agent error:/)
    expect(en.errors.uploadError).not.toMatch(/^Upload failed:/)
  })

  test('Chinese common errors avoid terse technical labels', () => {
    expect(zh.errors.network).toContain('检查网络')
    expect(zh.errors.serverError).toContain('稍等片刻')
    expect(zh.errors.forbidden).toContain('管理员')
    expect(zh.errors.agentError).toContain('检查会话状态')
    expect(zh.errors.agentError).not.toMatch(/^会话错误/)
    expect(zh.errors.uploadError).not.toMatch(/^上传失败/)
  })

  test('empty states include a next step', () => {
    expect(en.common.noResults).toContain('clear the filters')
    expect(en.agents.noAgents).toContain('Create one agent')
    expect(zh.common.noResults).toContain('清除筛选')
    expect(zh.agents.noAgents).toContain('创建一个会话')
  })
})
