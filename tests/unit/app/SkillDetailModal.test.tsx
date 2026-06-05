import '@app/i18n'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { SkillDetailModal } from '@app/features/skills/SkillDetailModal'
import type { Skill } from '@app/shared/model/skills.store'

const baseSkill: Skill = {
  id: 'skill-deploy-review',
  name: 'deploy-review',
  description: 'Check deployment steps before release.',
  plugin: 'Workspace skills',
  pluginAuthor: 'Platform team',
  content: 'Verify health checks, rollback notes, and user-facing release status.',
  path: 'skills/deploy-review',
  installed: true,
  marketplace: 'workspace',
  cliTool: 'codex',
  triggerPattern: 'deploy',
}

afterEach(() => {
  cleanup()
})

describe('SkillDetailModal', () => {
  test('explains a skill in beginner-readable terms', () => {
    render(<SkillDetailModal skill={baseSkill} onClose={() => {}} />)

    expect(screen.getByRole('dialog', { name: 'deploy-review' })).toBeInTheDocument()
    expect(
      screen.getByText('Reusable instructions agents can apply during task work.')
    ).toBeInTheDocument()
    expect(screen.getByText('Ready to use')).toBeInTheDocument()
    expect(screen.getByText('Best with Codex')).toBeInTheDocument()
    expect(screen.queryByText(/Codex C[L]I/)).toBeNull()
    expect(screen.getByText('Where it came from')).toBeInTheDocument()
    expect(screen.getByText('Workspace skills')).toBeInTheDocument()
    expect(screen.getByText('Maintainer')).toBeInTheDocument()
    expect(screen.getByText('Platform team')).toBeInTheDocument()
    expect(screen.getByText('Version')).toBeInTheDocument()
    expect(screen.getByText('workspace')).toBeInTheDocument()
    expect(screen.getByText('What this helps with')).toBeInTheDocument()
    expect(screen.getByText('Check deployment steps before release.')).toBeInTheDocument()
    expect(screen.getByText('When agents should consider it')).toBeInTheDocument()
    expect(
      screen.getByText('Agents can use this skill when the task matches this phrase.')
    ).toBeInTheDocument()
    expect(screen.getByText('deploy')).toBeInTheDocument()
    expect(screen.getByText('Instructions the agent will read')).toBeInTheDocument()
    expect(
      screen.getByText('Review this text if you need to check exactly what will be reused.')
    ).toBeInTheDocument()
    expect(
      screen.getByText('Verify health checks, rollback notes, and user-facing release status.')
    ).toBeInTheDocument()
  })

  test('shows safe next-state language when the skill is not ready', () => {
    render(
      <SkillDetailModal
        skill={{
          ...baseSkill,
          description: '',
          content: '',
          installed: false,
          plugin: '',
          pluginAuthor: '',
          marketplace: '',
          cliTool: '',
          triggerPattern: '',
        }}
        onClose={() => {}}
      />
    )

    expect(screen.getByText('Needs install before agents can use it')).toBeInTheDocument()
    expect(screen.getByText('Works with any agent')).toBeInTheDocument()
    expect(screen.getByText('Skills library')).toBeInTheDocument()
    expect(screen.getByText('Unknown')).toBeInTheDocument()
    expect(screen.getByText('latest')).toBeInTheDocument()
    expect(
      screen.getByText('No summary yet. Review the instructions below before using this skill.')
    ).toBeInTheDocument()
    expect(
      screen.getByText(
        'No reusable instructions have been saved yet. Add instructions before asking agents to use this skill.'
      )
    ).toBeInTheDocument()
  })

  test('closes from the beginner-friendly done action', () => {
    const onClose = vi.fn()
    render(<SkillDetailModal skill={baseSkill} onClose={onClose} />)

    fireEvent.click(screen.getByRole('button', { name: 'Done' }))

    expect(onClose).toHaveBeenCalledOnce()
  })
})
