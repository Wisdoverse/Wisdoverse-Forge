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
    expect(en.skills.detail.unknownAuthor).toContain('not listed yet')
    expect(zh.common.noResults).toContain('清除筛选')
    expect(zh.agents.noAgents).toContain('创建一个 Agent')
    expect(zh.skills.detail.noDescription).toContain('查看下面的说明')
    expect(zh.skills.detail.noContent).toContain('先补充说明')
    expect(zh.skills.detail.unknownAuthor).toContain('暂未列出')
  })

  test('shared instruction input copy avoids prompt jargon', () => {
    expect(en.prompt.placeholder).toBe('Type one instruction for the agent...')
    expect(en.prompt.placeholderShort).toBe('Type an instruction...')
    expect(en.prompt.emptyPrompt).toBe('Type an instruction before sending.')
    expect(en.prompt.selectAgent).toBe('Choose an agent first.')
    expect(JSON.stringify(en.prompt)).not.toContain('Type your prompt')
    expect(JSON.stringify(en.prompt)).not.toContain('Please select a agent')

    expect(zh.prompt.placeholder).toBe('输入一条给 Agent 的指令...')
    expect(zh.prompt.placeholderShort).toBe('输入一条指令...')
    expect(zh.prompt.emptyPrompt).toBe('请先输入一条指令。')
    expect(JSON.stringify(zh.prompt)).not.toContain('输入提示')
    expect(JSON.stringify(zh.prompt)).not.toContain('请输入提示')
  })

  test('getting started reuse copy explains saved instructions without expert terms', () => {
    expect(en.gettingStarted.readyDetail).toContain('review saved instructions')
    expect(en.gettingStarted.steps.reuse.title).toBe('Reuse what worked')
    expect(en.gettingStarted.steps.reuse.empty).toContain('save for next time')
    expect(en.gettingStarted.steps.reuse.ready).toContain('Saved instructions')
    expect(en.gettingStarted.steps.reuse.open).toBe('Show saved instructions')
    expect(JSON.stringify(en.gettingStarted.steps.reuse)).not.toContain('skill candidates')
    expect(JSON.stringify(en.gettingStarted.steps.reuse)).not.toContain('applied skill context')

    expect(zh.gettingStarted.readyDetail).toContain('保存好的指令')
    expect(zh.gettingStarted.steps.reuse.title).toBe('复用有效做法')
    expect(zh.gettingStarted.steps.reuse.empty).toContain('保存到下次使用')
    expect(zh.gettingStarted.steps.reuse.ready).toContain('保存好的指令')
    expect(zh.gettingStarted.steps.reuse.open).toBe('查看保存的指令')
    expect(JSON.stringify(zh.gettingStarted.steps.reuse)).not.toContain('技能候选')
    expect(JSON.stringify(zh.gettingStarted.steps.reuse)).not.toContain('技能上下文')
  })

  test('visual map labels avoid old scene and draw-mode jargon', () => {
    expect(en.workshop.title).toBe('Visual map')
    expect(en.workshop.loading).toBe('Loading visual map...')
    expect(en.workshop.shortcuts.drawMode).toBe('Press D to add drawing notes')
    expect(JSON.stringify(en.workshop)).not.toContain('Workshop')
    expect(JSON.stringify(en.workshop)).not.toContain('draw mode')

    expect(zh.workshop.title).toBe('视觉地图')
    expect(zh.workshop.loading).toBe('加载视觉地图...')
    expect(zh.workshop.shortcuts.drawMode).toBe('按 D 添加绘图备注')
    expect(JSON.stringify(zh.workshop)).not.toContain('工作坊')
    expect(JSON.stringify(zh.workshop)).not.toContain('绘图模式')
  })

  test('this-computer agent join errors avoid request-header and connection-policy jargon', () => {
    const englishJoin = en.errors.agent.enroll.missing_idempotency_key
    const englishSecure = en.errors.agent.enroll.plaintext_nats_blocked
    const chineseJoin = zh.errors.agent.enroll.missing_idempotency_key
    const chineseSecure = zh.errors.agent.enroll.plaintext_nats_blocked

    expect(englishJoin.title).toContain('Setup command needs to be run again')
    expect(englishJoin.detail).toContain('Run the setup command on this computer again')
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

    expect(chineseJoin.title).toContain('重新运行设置命令')
    expect(chineseJoin.detail).toContain('在这台电脑上重新运行设置命令')
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
    expect(zh.gettingStarted.steps.routing.title).toBe('任务队列')
    expect(JSON.stringify(zh.gettingStarted)).not.toContain('工作通道')
    expect(zh.admin.agents.title).toBe('Agent 管理')
    expect(JSON.stringify(zh)).not.toContain('会话')
  })

  test('admin metric labels describe live browser activity without protocol jargon', () => {
    expect(en.admin.metrics.wsConnections).toBe('Live browser connections')
    expect(en.admin.metrics.wsConnections).not.toContain('WS')
    expect(en.admin.metrics.wsConnections).not.toContain('WebSocket')

    expect(zh.admin.metrics.wsConnections).toBe('实时浏览器连接')
    expect(zh.admin.metrics.wsConnections).not.toContain('WebSocket')
  })
})
