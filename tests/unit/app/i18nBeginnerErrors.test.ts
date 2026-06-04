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
    expect(en.errors.agentError).not.toContain('Detail:')
    expect(en.errors.fileError).not.toContain('Detail:')
    expect(en.errors.uploadError).not.toMatch(/^Upload failed:/)
    expect(en.errors.uploadError).not.toContain('Detail:')
    expect(en.errors.downloadError).not.toContain('Detail:')
  })

  test('Chinese common errors avoid terse technical labels', () => {
    expect(zh.errors.network).toContain('检查网络')
    expect(zh.errors.serverError).toContain('稍等片刻')
    expect(zh.errors.forbidden).toContain('管理员')
    expect(zh.errors.agentError).toContain('检查会话状态')
    expect(zh.errors.agentError).not.toMatch(/^会话错误/)
    expect(zh.errors.agentError).not.toContain('详情：')
    expect(zh.errors.fileError).not.toContain('详情：')
    expect(zh.errors.uploadError).not.toMatch(/^上传失败/)
    expect(zh.errors.uploadError).not.toContain('详情：')
    expect(zh.errors.downloadError).not.toContain('详情：')
  })

  test('empty states include a next step', () => {
    expect(en.common.noResults).toContain('clear the filters')
    expect(en.agents.noAgents).toContain('Create one agent')
    expect(en.skills.detail.noDescription).toContain('Review the instructions')
    expect(en.skills.detail.noContent).toContain('Add instructions')
    expect(zh.common.noResults).toContain('清除筛选')
    expect(zh.agents.noAgents).toContain('创建一个会话')
    expect(zh.skills.detail.noDescription).toContain('查看下面的说明')
    expect(zh.skills.detail.noContent).toContain('先补充说明')
  })

  test('local agent join errors avoid request-header and connection-policy jargon', () => {
    const englishJoin = en.errors.agent.enroll.missing_idempotency_key
    const englishSecure = en.errors.agent.enroll.plaintext_nats_blocked
    const chineseJoin = zh.errors.agent.enroll.missing_idempotency_key
    const chineseSecure = zh.errors.agent.enroll.plaintext_nats_blocked

    expect(englishJoin.title).toContain('Join request needs to be sent again')
    expect(englishJoin.detail).toContain('Run the agent join step again')
    expect(englishJoin.title).not.toContain('Idempotency-Key')
    expect(englishJoin.detail).not.toContain('UUID')
    expect(englishSecure.detail).toContain('secure connection address')
    expect(englishSecure.detail).not.toContain('NATS_AGENT_URL')
    expect(englishSecure.detail).not.toContain('tls://')
    expect(englishSecure.detail).not.toContain('allow_plaintext')

    expect(chineseJoin.title).toContain('重新发送加入请求')
    expect(chineseJoin.detail).toContain('重新执行 Agent 加入步骤')
    expect(chineseJoin.title).not.toContain('Idempotency-Key')
    expect(chineseJoin.detail).not.toContain('UUID')
    expect(chineseSecure.detail).toContain('安全连接地址')
    expect(chineseSecure.detail).not.toContain('NATS_AGENT_URL')
    expect(chineseSecure.detail).not.toContain('tls://')
    expect(chineseSecure.detail).not.toContain('allow_plaintext')
  })
})
