import { describe, expect, test } from 'vitest'
import { en } from '@app/shared/i18n/locales/en'
import { zh } from '@app/shared/i18n/locales/zh'

describe('beginner error translations', () => {
  test('English common errors lead with recovery steps', () => {
    expect(en.errors.network).toContain('Check your connection')
    expect(en.errors.serverError).toContain('try again')
    expect(en.auth.networkError).toContain('signing in')
    expect(en.auth.networkError).not.toContain('service')
    expect(en.errors.network).not.toContain('service')
    expect(en.errors.serverError).not.toContain('service')
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
    expect(zh.auth.networkError).toContain('登录')
    expect(zh.auth.networkError).not.toContain('服务器')
    expect(zh.errors.network).not.toContain('服务器')
    expect(zh.errors.serverError).not.toContain('服务器')
    expect(zh.errors.forbidden).toContain('管理员')
    expect(zh.errors.agentError).toContain('检查 Agent 状态')
    expect(zh.errors.agentError).not.toMatch(/^Agent 错误/)
    expect(zh.errors.agentError).not.toContain('详情：')
    expect(zh.errors.fileError).not.toContain('详情：')
    expect(zh.errors.uploadError).not.toMatch(/^上传失败/)
    expect(zh.errors.uploadError).not.toContain('详情：')
    expect(zh.errors.downloadError).not.toContain('详情：')
  })

  test('authentication entry messages give clear next steps', () => {
    expect(en.auth.loginSuccess).toBe('You are signed in.')
    expect(en.auth.invalidCredentials).toContain('Check your email and password')
    expect(en.auth.accountLocked).toContain('Wait a few minutes')
    expect(en.auth.accountLocked).toContain('owner or admin')
    expect(en.auth.agentExpired).toContain('Sign in again')
    expect(en.auth.agentExpired).not.toContain('agent')
    expect(en.auth.agentExpired).not.toContain('login')
    expect(en.auth.passwordResetSent).toContain('password reset link')
    expect(en.auth.fillAllFields).toContain('then try again')

    expect(zh.auth.loginSuccess).toBe('你已登录。')
    expect(zh.auth.invalidCredentials).toContain('检查邮箱和密码')
    expect(zh.auth.accountLocked).toContain('等几分钟后重试')
    expect(zh.auth.accountLocked).toContain('管理员')
    expect(zh.auth.agentExpired).toContain('重新登录后继续')
    expect(zh.auth.agentExpired).not.toContain('Agent')
    expect(zh.auth.passwordResetSent).toContain('密码重置链接')
    expect(zh.auth.fillAllFields).toContain('然后重试')
  })

  test('empty states include a next step', () => {
    expect(en.common.noResults).toContain('clear the filters')
    expect(en.agents.noAgents).toContain('Create one agent')
    expect(en.skills.detail.noDescription).toContain('Review the instructions')
    expect(en.skills.detail.noContent).toContain('Add instructions')
    expect(zh.common.noResults).toContain('清除筛选')
    expect(zh.agents.noAgents).toContain('创建一个 Agent')
    expect(zh.skills.detail.noDescription).toContain('查看下面的说明')
    expect(zh.skills.detail.noContent).toContain('先补充说明')
  })

  test('this-computer agent join errors avoid request-header and connection-policy jargon', () => {
    const englishJoin = en.errors.agent.enroll.missing_idempotency_key
    const englishSecure = en.errors.agent.enroll.plaintext_nats_blocked
    const chineseJoin = zh.errors.agent.enroll.missing_idempotency_key
    const chineseSecure = zh.errors.agent.enroll.plaintext_nats_blocked

    expect(englishJoin.title).toContain('Join request needs to be sent again')
    expect(englishJoin.detail).toContain('Run the join command on this computer again')
    expect(englishJoin.detail).toContain('Agent Work Setup')
    expect(englishJoin.title).not.toContain('Idempotency-Key')
    expect(englishJoin.detail).not.toContain('UUID')
    expect(englishJoin.detail).not.toContain('local agent')
    expect(englishSecure.detail).toContain('secure connection address')
    expect(englishSecure.detail).toContain('agents joined from this computer')
    expect(englishSecure.detail).not.toContain('local agents')
    expect(englishSecure.detail).not.toContain('NATS_AGENT_URL')
    expect(englishSecure.detail).not.toContain('tls://')
    expect(englishSecure.detail).not.toContain('allow_plaintext')

    expect(chineseJoin.title).toContain('重新发送加入请求')
    expect(chineseJoin.detail).toContain('在这台电脑上重新运行加入命令')
    expect(chineseJoin.detail).toContain('Agent 工作设置')
    expect(chineseJoin.title).not.toContain('Idempotency-Key')
    expect(chineseJoin.detail).not.toContain('UUID')
    expect(chineseJoin.detail).not.toContain('本地 Agent')
    expect(chineseSecure.detail).toContain('安全连接地址')
    expect(chineseSecure.detail).toContain('从这台电脑接入 Agent')
    expect(chineseSecure.detail).not.toContain('本地 Agent')
    expect(chineseSecure.detail).not.toContain('NATS_AGENT_URL')
    expect(chineseSecure.detail).not.toContain('tls://')
    expect(chineseSecure.detail).not.toContain('allow_plaintext')
  })

  test('Chinese agent-facing copy uses the current Agent vocabulary', () => {
    expect(zh.nav.agents).toBe('Agent')
    expect(zh.agents.title).toBe('Agent')
    expect(zh.gettingStarted.steps.agent.create).toBe('创建 Agent')
    expect(zh.admin.agents.title).toBe('Agent 管理')
    expect(JSON.stringify(zh)).not.toContain('会话')
  })
})
