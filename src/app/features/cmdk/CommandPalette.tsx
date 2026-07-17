import { useState } from 'react'
import { Command } from 'cmdk'
import type { TFunction } from 'i18next'
import { useTranslation } from 'react-i18next'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'
import { useContextFeaturesStore } from '@app/entities/context/model/context-features.store'
import { useSettingsStore } from '@app/entities/settings'
import { shouldShowGettingStarted } from '@app/shared/lib/gettingStartedPreference'

interface CommandPaletteProps {
  isOpen: boolean
  onClose: () => void
  onSelect?: (command: string) => void
  createTaskCommand?: {
    label: string
    description: string
    searchText?: string
  }
}

type CommandDefinition = {
  id: string
  labelKey: string
  descriptionKey: string
  searchText?: string
}

type PaletteCommand = {
  id: string
  label: string
  description: string
  searchText?: string
}

const NAV_COMMANDS = [
  {
    id: 'nav:start',
    labelKey: 'commandPalette.commands.nav.start.label',
    descriptionKey: 'commandPalette.commands.nav.start.description',
    searchText: 'setup checklist start tutorial onboarding 设置清单 新手 引导 开始',
  },
  {
    id: 'nav:tasks',
    labelKey: 'commandPalette.commands.nav.tasks.label',
    descriptionKey: 'commandPalette.commands.nav.tasks.description',
    searchText: 'tasks board work progress 任务 看板 工作 进度',
  },
  {
    id: 'nav:inbox',
    labelKey: 'commandPalette.commands.nav.inbox.label',
    descriptionKey: 'commandPalette.commands.nav.inbox.description',
    searchText: 'inbox alerts notifications updates 收件箱 提醒 通知 待处理',
  },
  {
    id: 'nav:context',
    labelKey: 'commandPalette.commands.nav.context.label',
    descriptionKey: 'commandPalette.commands.nav.context.description',
    searchText: 'context notes instructions 上下文 笔记 指令',
  },
  {
    id: 'nav:agents',
    labelKey: 'commandPalette.commands.nav.agents.label',
    descriptionKey: 'commandPalette.commands.nav.agents.description',
    searchText: 'agents assistants workers agent 智能体 代理 助手 agent',
  },
  {
    id: 'nav:skills',
    labelKey: 'commandPalette.commands.nav.skills.label',
    descriptionKey: 'commandPalette.commands.nav.skills.description',
    searchText: 'saved instructions skills reusable steps 保存指引 保存指令 技能 复用步骤',
  },
  {
    id: 'nav:settings',
    labelKey: 'commandPalette.commands.nav.settings.label',
    descriptionKey: 'commandPalette.commands.nav.settings.description',
    searchText: 'settings account teams projects tools 设置 账号 团队 项目 工具',
  },
]

type NavCommand = PaletteCommand

const EMPTY_SEARCH_QUICK_COMMAND_IDS = ['nav:tasks', 'nav:agents', 'nav:settings'] as const

const DEFAULT_CREATE_TASK_COMMAND = {
  id: 'action:create-task',
  labelKey: 'commandPalette.commands.actions.createTask.label',
  descriptionKey: 'commandPalette.commands.actions.createTask.description',
  searchText: 'new task create task send work 创建任务 新任务 任务 工作',
}

const SECONDARY_ACTION_COMMANDS = [
  {
    id: 'action:work-tool-sign-ins',
    labelKey: 'commandPalette.commands.actions.workToolSignIns.label',
    descriptionKey: 'commandPalette.commands.actions.workToolSignIns.description',
    searchText: 'codex cli openai login sign in 登录 登陆 工作工具 设置',
  },
  {
    id: 'settings:keys',
    labelKey: 'commandPalette.commands.actions.keys.label',
    descriptionKey: 'commandPalette.commands.actions.keys.description',
    searchText:
      'api key access token tool access keys outside tool automation integration personal access key 外部工具 密钥 访问令牌 自动化 集成',
  },
  {
    id: 'settings:git-credentials',
    labelKey: 'commandPalette.commands.actions.gitCredentials.label',
    descriptionKey: 'commandPalette.commands.actions.gitCredentials.description',
    searchText:
      'https code access git credential private repository token password clone 代码访问 代码账号 私有仓库 令牌 密码 克隆',
  },
  {
    id: 'settings:ssh-keys',
    labelKey: 'commandPalette.commands.actions.sshKeys.label',
    descriptionKey: 'commandPalette.commands.actions.sshKeys.description',
    searchText:
      'ssh key ssh code access git private repository deploy key SSH 密钥 代码访问 私有仓库',
  },
  {
    id: 'settings:resources',
    labelKey: 'commandPalette.commands.actions.resources.label',
    descriptionKey: 'commandPalette.commands.actions.resources.description',
    searchText:
      'agent size resource limits cpu memory small standard large 智能体 大小 限制 资源 内存',
  },
  {
    id: 'settings:projects',
    labelKey: 'commandPalette.commands.actions.projects.label',
    descriptionKey: 'commandPalette.commands.actions.projects.description',
    searchText:
      'project settings projects workspace work area task files 项目设置 项目 工作区 文件',
  },
  {
    id: 'settings:teams',
    labelKey: 'commandPalette.commands.actions.teams.label',
    descriptionKey: 'commandPalette.commands.actions.teams.description',
    searchText: 'team settings teams people members access invite 团队设置 团队 成员 邀请 权限',
  },
  {
    id: 'settings:providers',
    labelKey: 'commandPalette.commands.actions.providers.label',
    descriptionKey: 'commandPalette.commands.actions.providers.description',
    searchText:
      'ai services model provider llm account key connection 模型服务 AI 服务 模型 账号 连接',
  },
  {
    id: 'settings:runtime',
    labelKey: 'commandPalette.commands.actions.runtime.label',
    descriptionKey: 'commandPalette.commands.actions.runtime.description',
    searchText:
      'where agents work runtime work tool files codex claude 智能体 工作位置 工作工具 文件',
  },
  {
    id: 'settings:account',
    labelKey: 'commandPalette.commands.actions.account.label',
    descriptionKey: 'commandPalette.commands.actions.account.description',
    searchText:
      'account profile password username setup checklist theme language 账号 密码 用户名 设置清单 主题 语言',
  },
  {
    id: 'action:toggle-theme',
    labelKey: 'commandPalette.commands.actions.theme.label',
    descriptionKey: 'commandPalette.commands.actions.theme.description',
  },
]

const SETUP_CHECKLIST_RECOVERY_COMMAND = {
  id: 'action:show-setup-checklist',
  labelKey: 'commandPalette.commands.actions.setupChecklistRecovery.label',
  descriptionKey: 'commandPalette.commands.actions.setupChecklistRecovery.description',
  searchText: 'start tutorial onboarding setup checklist reset restore show again',
}

const VIEW_COMMANDS = [
  {
    id: 'view:board',
    labelKey: 'commandPalette.commands.views.board.label',
    descriptionKey: 'commandPalette.commands.views.board.description',
    searchText: 'board columns kanban 看板 列',
  },
  {
    id: 'view:list',
    labelKey: 'commandPalette.commands.views.list.label',
    descriptionKey: 'commandPalette.commands.views.list.description',
    searchText: 'list table sort 列表 表格 排序',
  },
  {
    id: 'view:timeline',
    labelKey: 'commandPalette.commands.views.timeline.label',
    descriptionKey: 'commandPalette.commands.views.timeline.description',
    searchText: 'timeline history activity 时间线 历史 活动',
  },
  {
    id: 'view:3d',
    labelKey: 'commandPalette.commands.views.visualMap.label',
    descriptionKey: 'commandPalette.commands.views.visualMap.description',
    searchText: 'visual map 3d agents tasks 可视化 地图 智能体 任务',
  },
]

const COMMAND_DISCOVERY_STEP_KEYS = [
  'commandPalette.discovery.tasks',
  'commandPalette.discovery.inbox',
  'commandPalette.discovery.settings',
]

function translateCommand(command: CommandDefinition, t: TFunction): PaletteCommand {
  return {
    id: command.id,
    label: t(command.labelKey),
    description: t(command.descriptionKey),
    searchText: command.searchText,
  }
}

function commonWorkflowSuggestion(commands: Array<{ label: string }>, t: TFunction): string {
  const labels = commands.map((command) => command.label)
  if (labels.length === 0) return t('commandPalette.empty.tryShorter')
  if (labels.length === 1) return t('commandPalette.empty.tryOne', { label: labels[0] })
  const prefix = labels.slice(0, -1).join(t('commandPalette.empty.listSeparator'))
  return t('commandPalette.empty.tryMany', { prefix, last: labels[labels.length - 1] })
}

export function CommandPalette({
  isOpen,
  onClose,
  onSelect,
  createTaskCommand,
}: CommandPaletteProps) {
  const { t } = useTranslation()
  const contextGovernanceEnabled = useContextFeaturesStore((s) => s.governance)
  const showGettingStarted = useSettingsStore((s) => shouldShowGettingStarted(s.preferences))
  const [search, setSearch] = useState('')
  const [showSecondaryActions, setShowSecondaryActions] = useState(false)
  if (!isOpen) return null
  const hasSearch = search.trim().length > 0
  const navCommands = NAV_COMMANDS.filter(
    (cmd) =>
      (cmd.id !== 'nav:context' || contextGovernanceEnabled) &&
      (cmd.id !== 'nav:start' || showGettingStarted)
  ).map((cmd) => translateCommand(cmd, t))
  const taskCommand = { ...translateCommand(DEFAULT_CREATE_TASK_COMMAND, t), ...createTaskCommand }
  const primaryActionCommands = showGettingStarted
    ? [taskCommand]
    : [translateCommand(SETUP_CHECKLIST_RECOVERY_COMMAND, t), taskCommand]
  const secondaryActionCommands = SECONDARY_ACTION_COMMANDS.map((cmd) => translateCommand(cmd, t))
  const visibleSecondaryActionCommands =
    hasSearch || showSecondaryActions ? secondaryActionCommands : []
  const actionCommands = [...primaryActionCommands, ...visibleSecondaryActionCommands]
  const showSecondaryActionToggle = !hasSearch && secondaryActionCommands.length > 0
  const viewCommands = VIEW_COMMANDS.map((cmd) => translateCommand(cmd, t))
  const emptySearchSuggestion = commonWorkflowSuggestion(navCommands, t)
  const emptySearchQuickCommands = EMPTY_SEARCH_QUICK_COMMAND_IDS.map((id) =>
    navCommands.find((command) => command.id === id)
  ).filter((command): command is NavCommand => Boolean(command))
  const discoverySteps = COMMAND_DISCOVERY_STEP_KEYS.map((key) => t(key))

  function handleSelect(commandId: string) {
    onSelect?.(commandId)
    onClose()
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="command-palette-title"
        aria-describedby="command-palette-help"
        className={cn(
          'w-full max-w-lg mx-4',
          'bg-surface dark:bg-surface-dark',
          'rounded-card border border-black/[0.08] shadow-panel dark:border-white/[0.1]',
          'overflow-hidden'
        )}
      >
        <Command>
          <div className="border-b border-black/[0.08] px-4 py-3 dark:border-white/[0.08]">
            <p
              id="command-palette-title"
              className="text-ui-caption font-semibold text-foreground-light dark:text-foreground-dark"
            >
              {t('commandPalette.title')}
            </p>
            <ol
              id="command-palette-help"
              className="mt-2 list-decimal space-y-1 pl-4 text-ui-caption text-secondary-light dark:text-secondary-dark"
            >
              {discoverySteps.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          </div>
          <Command.Input
            aria-label={t('commandPalette.inputLabel')}
            value={search}
            onValueChange={setSearch}
            placeholder={t('commandPalette.placeholder')}
            className={cn(
              'w-full px-4 py-3 text-ui-body outline-none',
              'bg-transparent border-b border-black/[0.08] dark:border-white/[0.08]',
              'text-foreground-light dark:text-foreground-dark',
              'placeholder:text-secondary-light dark:placeholder:text-secondary-dark'
            )}
          />
          <Command.List className="max-h-80 overflow-y-auto py-2">
            <Command.Empty className="px-4 py-6 text-center text-ui-body text-secondary-light dark:text-secondary-dark">
              <div role="status" aria-live="polite">
                <p className="font-medium text-foreground-light dark:text-foreground-dark">
                  {t('commandPalette.empty.title')}
                </p>
                <p className="mt-1">{emptySearchSuggestion}</p>
              </div>
              {emptySearchQuickCommands.length > 0 && (
                <div
                  className="mt-3 flex flex-wrap justify-center gap-2"
                  aria-label={t('commandPalette.empty.commonPages')}
                >
                  {emptySearchQuickCommands.map((command) => (
                    <button
                      key={command.id}
                      type="button"
                      onClick={() => handleSelect(command.id)}
                      className={uiStyles.secondaryButton}
                    >
                      {t('commandPalette.empty.openPage', { label: command.label })}
                    </button>
                  ))}
                </div>
              )}
              <button
                type="button"
                onClick={() => {
                  setSearch('')
                  setShowSecondaryActions(true)
                }}
                className={cn(uiStyles.secondaryButton, 'mt-3')}
              >
                {t('commandPalette.empty.showAll')}
              </button>
            </Command.Empty>

            <Command.Group
              heading={t('commandPalette.groups.navigation')}
              className="[&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-ui-caption [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-secondary-light dark:[&_[cmdk-group-heading]]:text-secondary-dark"
            >
              {navCommands.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={`${cmd.label} ${cmd.description} ${cmd.searchText ?? ''}`}
                  onSelect={() => handleSelect(cmd.id)}
                  className={cn(
                    'flex cursor-pointer flex-col gap-0.5 px-4 py-2 text-ui-body',
                    'text-foreground-light dark:text-foreground-dark',
                    'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]',
                    'aria-selected:bg-black/[0.06] dark:aria-selected:bg-white/[0.08]'
                  )}
                >
                  <span className="font-medium">{cmd.label}</span>
                  <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {cmd.description}
                  </span>
                </Command.Item>
              ))}
              {showSecondaryActionToggle && (
                <button
                  type="button"
                  aria-expanded={showSecondaryActions}
                  onClick={() => setShowSecondaryActions((value) => !value)}
                  className={cn(
                    'mx-3 my-1 flex w-[calc(100%-1.5rem)] flex-col gap-0.5 rounded-button px-3 py-2 text-left text-ui-body',
                    'border border-black/[0.08] bg-black/[0.02] text-foreground-light',
                    'transition-colors hover:bg-black/[0.04] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-apple-blue-focus',
                    'dark:border-white/[0.1] dark:bg-white/[0.04] dark:text-foreground-dark dark:hover:bg-white/[0.08]'
                  )}
                >
                  <span className="font-medium">
                    {t(
                      showSecondaryActions
                        ? 'commandPalette.setupActions.hide'
                        : 'commandPalette.setupActions.show'
                    )}
                  </span>
                  <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {t('commandPalette.setupActions.description')}
                  </span>
                </button>
              )}
            </Command.Group>

            <Command.Group
              heading={t('commandPalette.groups.actions')}
              className="[&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-ui-caption [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-secondary-light dark:[&_[cmdk-group-heading]]:text-secondary-dark"
            >
              {actionCommands.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={`${cmd.label} ${cmd.description} ${cmd.searchText ?? ''}`}
                  onSelect={() => handleSelect(cmd.id)}
                  className={cn(
                    'flex cursor-pointer flex-col gap-0.5 px-4 py-2 text-ui-body',
                    'text-foreground-light dark:text-foreground-dark',
                    'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]',
                    'aria-selected:bg-black/[0.06] dark:aria-selected:bg-white/[0.08]'
                  )}
                >
                  <span className="font-medium">{cmd.label}</span>
                  <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {cmd.description}
                  </span>
                </Command.Item>
              ))}
            </Command.Group>

            <Command.Group
              heading={t('commandPalette.groups.views')}
              className="[&_[cmdk-group-heading]]:px-4 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-ui-caption [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-secondary-light dark:[&_[cmdk-group-heading]]:text-secondary-dark"
            >
              {viewCommands.map((cmd) => (
                <Command.Item
                  key={cmd.id}
                  value={`${cmd.label} ${cmd.description} ${cmd.searchText ?? ''}`}
                  onSelect={() => handleSelect(cmd.id)}
                  className={cn(
                    'flex cursor-pointer flex-col gap-0.5 px-4 py-2 text-ui-body',
                    'text-foreground-light dark:text-foreground-dark',
                    'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]',
                    'aria-selected:bg-black/[0.06] dark:aria-selected:bg-white/[0.08]'
                  )}
                >
                  <span className="font-medium">{cmd.label}</span>
                  <span className="text-ui-caption text-secondary-light dark:text-secondary-dark">
                    {cmd.description}
                  </span>
                </Command.Item>
              ))}
            </Command.Group>
          </Command.List>
        </Command>
      </div>
    </div>
  )
}
