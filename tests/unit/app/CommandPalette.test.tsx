import { describe, test, expect, afterEach, vi } from 'vitest'
import { fireEvent, render, screen, cleanup, waitFor } from '@testing-library/react'
import { CommandPalette } from '@app/features/cmdk/CommandPalette'
import { i18n } from '@app/i18n'
import { useContextFeaturesStore } from '@app/entities/context/model/context-features.store'
import { useSettingsStore } from '@app/entities/settings'

afterEach(async () => {
  cleanup()
  useContextFeaturesStore.getState().reset()
  useSettingsStore.setState({ preferences: null, preferencesLoaded: false })
  await i18n.changeLanguage('en')
})

describe('CommandPalette', () => {
  const previousDiscoveryTitle = ['Command', 'discovery', 'path'].join(' ')
  const previousEmptyTitle = ['No', 'command', 'matches', 'that', 'search'].join(' ')
  const previousFullListCopy = ['full', 'command', 'list'].join(' ')
  const previousActionHeading = ['Start', 'an', 'action'].join(' ')
  const previousSavedItemsLabel = new RegExp(`^${['Saved', ' items'].join('')}$`)
  const previousSavedItemsDescription = new RegExp(
    ['Review', 'knowledge', 'before', 'agents', 'use', 'it', 'in', 'tasks'].join('\\s+'),
    'i'
  )

  test('renders when open', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    const dialog = screen.getByRole('dialog', { name: /find what you need/i })
    expect(dialog).toHaveAttribute('aria-modal', 'true')
    expect(dialog).toHaveAccessibleDescription(/Write one small task/i)
    expect(dialog).toHaveAccessibleDescription(/when you want an agent to do something/i)
    expect(screen.getByLabelText('Search pages and things to do')).toBeDefined()
    expect(
      screen.getByPlaceholderText(
        'Search what you want to do, e.g. send a task, add agent, sign in'
      )
    ).toBeDefined()
    expect(screen.getByText('Find what you need')).toBeDefined()
    expect(
      screen.getByText('Write one small task when you want an agent to do something.')
    ).toBeDefined()
    expect(screen.queryByText('Write one small task when you want work done.')).toBeNull()
    expect(
      screen.getByText('Check updates that need a person before you keep working.')
    ).toBeDefined()
    expect(
      screen.getByText('Fix setup blockers for agents, sign-ins, projects, and access.')
    ).toBeDefined()
    expect(screen.queryByText(/something needs your attention/i)).toBeNull()
    expect(screen.queryByText(/runtime status/i)).toBeNull()
    expect(screen.queryByText(previousDiscoveryTitle)).toBeNull()
  })

  test('shows beginner-readable Chinese copy when the app language is Chinese', async () => {
    await i18n.changeLanguage('zh')

    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    const dialog = screen.getByRole('dialog', { name: '找到你要做的事' })
    expect(dialog).toHaveAccessibleDescription(/想让智能体做事时/)
    expect(screen.getByLabelText('搜索页面和可做的事')).toBeDefined()
    const input = screen.getByPlaceholderText(
      '搜索你想做什么，例如：发送任务、添加智能体、登录工具'
    )
    expect(input).toBeDefined()
    expect(screen.getByText('打开页面')).toBeDefined()
    expect(screen.getByText('创建或修改')).toBeDefined()
    expect(screen.getByText('切换任务视图')).toBeDefined()
    expect(screen.getByText('任务')).toBeDefined()
    expect(screen.getByText('查看计划中、进行中或已完成的工作。')).toBeDefined()
    expect(screen.getByText('新任务')).toBeDefined()
    expect(screen.getByText('告诉智能体你想要的结果，以及如何检查是否完成。')).toBeDefined()
    expect(screen.getByText('可视化地图')).toBeDefined()
    expect(screen.getByText('想让智能体做事时，先写一条小任务。')).toBeDefined()
    expect(screen.getByText('继续前先查看需要人工处理的更新。')).toBeDefined()
    expect(screen.getByText('处理智能体、登录、项目和访问权限里的设置卡点。')).toBeDefined()

    fireEvent.change(input, { target: { value: 'zzzzzz' } })

    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('搜索正在隐藏页面和操作')
    })
    expect(screen.queryByText('没有匹配的页面或选项')).toBeNull()
    expect(screen.getByText(/可以试试设置清单、任务、收件箱、智能体、Skills或设置/)).toBeDefined()
    expect(screen.getByRole('button', { name: '打开任务' })).toBeDefined()
    expect(screen.getByRole('button', { name: '打开智能体' })).toBeDefined()
    expect(screen.getByRole('button', { name: '打开设置' })).toBeDefined()
    expect(screen.getByRole('button', { name: '显示全部页面和操作' })).toBeDefined()
  })

  test('does not render when closed', () => {
    render(<CommandPalette isOpen={false} onClose={() => {}} />)
    expect(screen.queryByPlaceholderText(/search what you want to do/i)).toBeNull()
  })

  test('shows navigation commands', () => {
    useContextFeaturesStore.setState({ governance: true, loaded: true, loading: false })

    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    expect(screen.getByText('Go to a page')).toBeDefined()
    expect(screen.getByText('Tasks')).toBeDefined()
    expect(screen.getByText('See work that is planned, active, or done.')).toBeDefined()
    expect(screen.getByText('Inbox')).toBeDefined()
    expect(screen.getByText('Context')).toBeDefined()
    expect(
      screen.getByText('Check saved notes and guidance before agents reuse them.')
    ).toBeDefined()
    expect(
      screen.queryByText('Check saved notes and instructions before agents reuse them.')
    ).toBeNull()
    expect(screen.getByText('Setup checklist')).toBeDefined()
    expect(screen.getByText('Agents')).toBeDefined()
    expect(screen.getByText('Create or check agents for tasks or chat.')).toBeDefined()
    expect(screen.queryByText('Create or check agents that handle work.')).toBeNull()
    expect(screen.getByText('Skills')).toBeDefined()
    expect(screen.getByText('Reuse guidance for repeated work.')).toBeDefined()
    expect(screen.queryByText('Saved instructions')).toBeNull()
    expect(screen.queryByText('Reuse instructions for repeated work.')).toBeNull()
    expect(screen.getByText('Connect tools, account access, teams, and projects.')).toBeDefined()
    expect(screen.queryByText(/workers doing tasks/i)).toBeNull()
    expect(screen.queryByText(previousSavedItemsLabel)).toBeNull()
    expect(screen.queryByText(previousSavedItemsDescription)).toBeNull()
    expect(screen.queryByText(/^Saved guidance$/)).toBeNull()
    expect(screen.queryByText(/tools, keys/i)).toBeNull()
  })

  test('shows the setup checklist command unless Start is hidden', async () => {
    const onSelect = vi.fn()
    const onClose = vi.fn()
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: false },
      preferencesLoaded: true,
    })

    render(<CommandPalette isOpen={true} onClose={onClose} onSelect={onSelect} />)

    expect(screen.getByText('Setup checklist')).toBeDefined()
    expect(
      screen.getByText('Open setup steps again when you want a guided checklist.')
    ).toBeDefined()

    fireEvent.change(screen.getByPlaceholderText(/search what you want to do/i), {
      target: { value: 'setup checklist' },
    })

    await waitFor(() => expect(screen.getByText('Setup checklist')).toBeDefined())
    fireEvent.click(screen.getByText('Open setup steps again when you want a guided checklist.'))

    expect(onSelect).toHaveBeenCalledWith('nav:start')
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(screen.queryByText('Reset setup checklist')).toBeNull()
  })

  test('finds setup checklist recovery when Start is hidden', async () => {
    useSettingsStore.setState({
      preferences: { gettingStartedDismissed: true },
      preferencesLoaded: true,
    })

    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    expect(screen.queryByText('Setup checklist')).toBeNull()

    fireEvent.change(screen.getByPlaceholderText(/search what you want to do/i), {
      target: { value: 'start tutorial' },
    })

    await waitFor(() => {
      expect(screen.getByText('Reset setup checklist')).toBeDefined()
    })
    expect(screen.queryByText('Show setup checklist')).toBeNull()
    expect(
      screen.getByText(
        'Show the setup checklist in the left menu again. Projects, agents, and tasks stay unchanged.'
      )
    ).toBeDefined()
    expect(screen.queryByText('No page or option matches that search')).toBeNull()
  })

  test('keeps setup actions collapsed until people ask for them', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)
    expect(screen.getByText('Create or change something')).toBeDefined()
    expect(screen.queryByText('Reset setup checklist')).toBeNull()
    expect(screen.queryByText('Show setup checklist')).toBeNull()
    expect(screen.getByText('New task')).toBeDefined()
    expect(screen.getByText('Tell an agent the result you want and how to check it.')).toBeDefined()
    expect(screen.getByText('More setup actions')).toBeDefined()
    expect(screen.getByText('Show sign-ins, projects, teams, and other setup pages.')).toBeDefined()
    expect(screen.queryByText('File-change tool sign-in')).toBeNull()
    expect(screen.queryByText('Code tool sign-in')).toBeNull()
    expect(screen.queryByText('Work tool sign-in')).toBeNull()
    expect(screen.queryByText('Codex and work tool sign-in')).toBeNull()
    expect(screen.queryByText('Codex sign-in')).toBeNull()
    expect(screen.queryByText('Projects')).toBeNull()
    expect(screen.queryByText('Teams')).toBeNull()
    expect(screen.queryByText('AI services')).toBeNull()
    expect(screen.queryByText('Where agents work')).toBeNull()

    fireEvent.click(screen.getByRole('button', { name: /more setup actions/i }))

    expect(screen.getByText('Sign in to code tools')).toBeDefined()
    expect(screen.queryByText('Code tool sign-in')).toBeNull()
    expect(screen.queryByText('File-change tool sign-in')).toBeNull()
    expect(
      screen.getByText('Sign in before agents edit project files with Codex or another tool.')
    ).toBeDefined()
    expect(screen.queryByText('Work tool sign-in')).toBeNull()
    expect(screen.getByText('Projects')).toBeDefined()
    expect(
      screen.getByText('Create or choose where tasks, agents, and files belong.')
    ).toBeDefined()
    expect(screen.getByText('Teams')).toBeDefined()
    expect(screen.getByText('Create teams and manage who can change work.')).toBeDefined()
    expect(screen.getByText('AI services')).toBeDefined()
    expect(screen.getByText('Connect the AI account agents use to answer.')).toBeDefined()
    expect(screen.getByText('Where agents work')).toBeDefined()
    expect(
      screen.getByText(
        'Choose Project files for the usual setup, or This computer for local-only work.'
      )
    ).toBeDefined()
    expect(screen.queryByText('Codex CLI sign-in')).toBeNull()
    expect(screen.queryByText(previousActionHeading)).toBeNull()
    expect(screen.queryByText('Create task')).toBeNull()
    expect(screen.queryByText('Start a new piece of work.')).toBeNull()
  })

  test('uses a beginner setup action when task creation is not ready', async () => {
    const onSelect = vi.fn()
    const onClose = vi.fn()

    render(
      <CommandPalette
        isOpen={true}
        onClose={onClose}
        onSelect={onSelect}
        createTaskCommand={{
          label: 'Set up project before task',
          description: 'Open project settings so tasks have a place to belong.',
          searchText: 'new task create task project setup',
        }}
      />
    )

    fireEvent.change(screen.getByPlaceholderText(/search what you want to do/i), {
      target: { value: 'new task' },
    })

    await waitFor(() => {
      expect(screen.getByText('Set up project before task')).toBeDefined()
    })
    fireEvent.click(screen.getByText('Open project settings so tasks have a place to belong.'))

    expect(onSelect).toHaveBeenCalledWith('action:create-task')
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(screen.queryByText('Tell an agent the result you want and how to check it.')).toBeNull()
  })

  test('finds Codex sign-in through beginner Chinese login search terms', async () => {
    const onSelect = vi.fn()
    const onClose = vi.fn()
    render(<CommandPalette isOpen={true} onClose={onClose} onSelect={onSelect} />)

    fireEvent.change(screen.getByPlaceholderText(/search what you want to do/i), {
      target: { value: 'codex 登录' },
    })

    await waitFor(() => {
      expect(screen.getByText('Sign in to code tools')).toBeDefined()
    })
    expect(screen.queryByText('Code tool sign-in')).toBeNull()
    expect(
      screen.getByText('Sign in before agents edit project files with Codex or another tool.')
    ).toBeDefined()
    fireEvent.click(
      screen.getByText('Sign in before agents edit project files with Codex or another tool.')
    )
    expect(onSelect).toHaveBeenCalledWith('action:work-tool-sign-ins')
    expect(onClose).toHaveBeenCalledTimes(1)
    expect(screen.queryByText('Codex and work tool sign-in')).toBeNull()
    expect(screen.queryByText('No page or option matches that search')).toBeNull()
  })

  test.each([
    ['任务', 'Tasks', 'nav:tasks'],
    ['智能体', 'Agents', 'nav:agents'],
    ['创建任务', 'New task', 'action:create-task'],
    ['项目设置', 'Projects', 'settings:projects'],
    ['模型服务', 'AI services', 'settings:providers'],
  ])('finds %s through Chinese beginner search terms', async (query, label, commandId) => {
    const onSelect = vi.fn()
    const onClose = vi.fn()
    render(<CommandPalette isOpen={true} onClose={onClose} onSelect={onSelect} />)

    fireEvent.change(screen.getByPlaceholderText(/search what you want to do/i), {
      target: { value: query },
    })

    await waitFor(() => {
      expect(screen.getByText(label)).toBeDefined()
    })
    expect(screen.queryByText('No page or option matches that search')).toBeNull()

    fireEvent.click(screen.getByText(label))

    expect(onSelect).toHaveBeenCalledWith(commandId)
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  test('finds direct Settings sections through beginner search terms', async () => {
    const onSelect = vi.fn()
    const onClose = vi.fn()
    render(<CommandPalette isOpen={true} onClose={onClose} onSelect={onSelect} />)

    fireEvent.change(screen.getByPlaceholderText(/search what you want to do/i), {
      target: { value: 'project settings' },
    })

    await waitFor(() => {
      expect(screen.getByText('Projects')).toBeDefined()
    })
    fireEvent.click(screen.getByText('Create or choose where tasks, agents, and files belong.'))

    expect(onSelect).toHaveBeenCalledWith('settings:projects')
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  test.each([
    ['api key', 'Tool access keys', 'settings:keys'],
    ['https code access', 'HTTPS code access', 'settings:git-credentials'],
    ['ssh key', 'SSH code access', 'settings:ssh-keys'],
    ['agent size', 'Agent size limits', 'settings:resources'],
    ['password', 'Account', 'settings:account'],
  ])('finds %s in Settings without knowing the section URL', async (query, label, commandId) => {
    const onSelect = vi.fn()
    const onClose = vi.fn()
    render(<CommandPalette isOpen={true} onClose={onClose} onSelect={onSelect} />)

    fireEvent.change(screen.getByPlaceholderText(/search what you want to do/i), {
      target: { value: query },
    })

    await waitFor(() => {
      expect(screen.getByText(label)).toBeDefined()
    })
    expect(screen.queryByText('No page or option matches that search')).toBeNull()

    fireEvent.click(screen.getByText(label))

    expect(onSelect).toHaveBeenCalledWith(commandId)
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  test('uses beginner-safe view names instead of old scene jargon', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    expect(screen.getByText('Visual map')).toBeDefined()
    expect(screen.getByText('See agents and tasks on a visual map.')).toBeDefined()
    expect(screen.queryByText('3D view')).toBeNull()
    expect(screen.queryByText(/workshop/i)).toBeNull()
  })

  test('searches beginner descriptions and shows an empty state', async () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    const input = screen.getByPlaceholderText(/search what you want to do/i)
    fireEvent.change(input, { target: { value: 'alerts' } })

    await waitFor(() => {
      expect(screen.getByText('Inbox')).toBeDefined()
    })

    fireEvent.change(input, { target: { value: 'zzzzzz' } })

    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('Search is hiding pages and actions')
    })
    expect(screen.getByRole('status')).toHaveAttribute('aria-live', 'polite')
    expect(
      screen.getByText(/try setup checklist, tasks, inbox, agents, skills, or settings/i)
    ).toBeDefined()
    expect(
      screen.queryByText(/try setup checklist, tasks, inbox, context, agents, skills, or settings/i)
    ).toBeNull()
    expect(screen.getByRole('button', { name: /open tasks/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /open agents/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /open settings/i })).toBeDefined()
    fireEvent.click(screen.getByRole('button', { name: /show all pages and actions/i }))

    await waitFor(() => {
      expect(input).toHaveValue('')
      expect(screen.getByText('Tasks')).toBeDefined()
    })
    expect(screen.queryByRole('button', { name: 'Clear search' })).toBeNull()
    expect(screen.queryByText('No page or option matches that search')).toBeNull()
    expect(screen.queryByText('No page or action matches that search')).toBeNull()
    expect(screen.queryByText(/common workflow/i)).toBeNull()
    expect(screen.queryByText(previousEmptyTitle)).toBeNull()
    expect(screen.queryByText(new RegExp(previousFullListCopy, 'i'))).toBeNull()
  })

  test('empty search lets beginners open a common page directly', async () => {
    const onSelect = vi.fn()
    const onClose = vi.fn()
    render(<CommandPalette isOpen={true} onClose={onClose} onSelect={onSelect} />)

    fireEvent.change(screen.getByPlaceholderText(/search what you want to do/i), {
      target: { value: 'zzzz no matching beginner action' },
    })

    await waitFor(() => {
      expect(screen.getByRole('status')).toHaveTextContent('Search is hiding pages and actions')
    })
    expect(screen.queryByText('No page or option matches that search')).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: /open agents/i }))

    expect(onSelect).toHaveBeenCalledWith('nav:agents')
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  test('suggests common workflow terms when search has no matches', () => {
    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText(/search what you want to do/i), {
      target: { value: 'missing workflow' },
    })

    expect(screen.getByText('Search is hiding pages and actions')).toBeDefined()
    expect(screen.queryByText('No page or option matches that search')).toBeNull()
    expect(
      screen.getByText(/try setup checklist, tasks, inbox, agents, skills, or settings/i)
    ).toBeDefined()
    expect(screen.getByText(/open a page people use often/i)).toBeDefined()
    expect(
      screen.queryByText(/try setup checklist, tasks, inbox, context, agents, skills, or settings/i)
    ).toBeNull()
    expect(screen.getByRole('button', { name: /open tasks/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /open agents/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /open settings/i })).toBeDefined()
    expect(screen.getByRole('button', { name: /show all pages and actions/i })).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Clear search' })).toBeNull()
    expect(screen.queryByText(previousEmptyTitle)).toBeNull()
    expect(screen.queryByText(new RegExp(previousFullListCopy, 'i'))).toBeNull()
  })

  test('includes Saved items in empty-search help only when the page is visible', () => {
    useContextFeaturesStore.setState({ governance: true, loaded: true, loading: false })

    render(<CommandPalette isOpen={true} onClose={() => {}} />)

    fireEvent.change(screen.getByPlaceholderText(/search what you want to do/i), {
      target: { value: 'missing workflow' },
    })

    expect(
      screen.getByText(/try setup checklist, tasks, inbox, context, agents, skills, or settings/i)
    ).toBeDefined()
  })
})
