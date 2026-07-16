import '@app/i18n'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, test, vi } from 'vitest'
import { SkillDetailModal } from '@app/features/skills/SkillDetailModal'
import type { Skill } from '@app/entities/skill'

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
    expect(screen.getByText('A skill the team can reuse.')).toBeInTheDocument()
    const modalFrame = screen
      .getByText('A skill the team can reuse.')
      .closest('[class*="max-w-md"]') as HTMLElement
    expect(modalFrame).toHaveClass('rounded-md', 'border', 'bg-transparent')
    expect(modalFrame.className).not.toContain('rounded-card')
    expect(modalFrame.className).not.toMatch(/(^|\s)bg-white(\s|$)/)
    expect(modalFrame.className).not.toContain('dark:bg-[#2c2c2e]')
    const closeButton = screen.getByRole('button', { name: /close/i })
    expect(closeButton).toHaveClass('rounded-md')
    expect(closeButton.className).toContain('text-secondary-light')
    expect(closeButton.className).toContain('dark:text-secondary-dark')
    expect(closeButton.className).toContain('hover:text-foreground-light')
    expect(closeButton.className).toContain('dark:hover:text-foreground-dark')
    expect(closeButton.className).not.toContain('hover:bg-black/[0.04]')
    expect(closeButton.className).not.toContain('dark:hover:bg-white/[0.06]')
    expect(
      screen.queryByText('Reusable instructions agents can apply during task work.')
    ).toBeNull()
    expect(screen.getByText('Ready to use')).toBeInTheDocument()
    expect(screen.getByText('Best with file-editing app: Codex')).toBeInTheDocument()
    expect(screen.getByText('Best with file-editing app: Codex')).toHaveAttribute(
      'title',
      'File editing tool: Codex'
    )
    expect(screen.queryByText('Best with Codex')).toBeNull()
    expect(screen.queryByTitle('Work tool: Codex')).toBeNull()
    expect(screen.queryByText(/Codex C[L]I/)).toBeNull()
    expect(screen.getByText('What to do next')).toBeInTheDocument()
    const nextStepFrame = screen.getByText('What to do next').closest('section') as HTMLElement
    expect(nextStepFrame).toHaveClass('rounded-md', 'border', 'bg-transparent')
    expect(nextStepFrame.className).not.toContain('rounded-card')
    expect(nextStepFrame.className).not.toContain('bg-apple-blue/[0.06]')
    expect(
      screen.getByText(
        'Use this skill when creating a task, or let matching words suggest it for similar work.'
      )
    ).toBeInTheDocument()
    expect(screen.getByText('Saved in')).toBeInTheDocument()
    const savedInFrame = screen.getByText('Saved in').parentElement as HTMLElement
    expect(savedInFrame).toHaveClass('rounded-md', 'border', 'bg-transparent')
    expect(savedInFrame.className).not.toContain('rounded-card')
    expect(screen.getAllByText('This team space')).toHaveLength(2)
    expect(screen.queryByText('Where it came from')).toBeNull()
    expect(screen.queryByText('Saved for this team space')).toBeNull()
    expect(screen.queryByText('Team space saved instructions')).toBeNull()
    expect(screen.queryByText('Workspace skills')).toBeNull()
    expect(screen.queryByText('Workspace saved instructions')).toBeNull()
    expect(screen.getByText('Updated by')).toBeInTheDocument()
    expect(screen.getByText('Platform team')).toBeInTheDocument()
    expect(screen.getByText('Who can use it')).toBeInTheDocument()
    expect(screen.queryByText('Available to')).toBeNull()
    expect(screen.queryByText('Version')).toBeNull()
    expect(screen.queryByText('workspace')).toBeNull()
    expect(screen.getByText('What this helps with')).toBeInTheDocument()
    expect(screen.getByText('Check deployment steps before release.')).toBeInTheDocument()
    expect(screen.getByText('When this helps')).toBeInTheDocument()
    expect(
      screen.getByText('Use this skill for tasks that include words like these.')
    ).toBeInTheDocument()
    expect(screen.getByText('deploy')).toBeInTheDocument()
    const triggerFrame = screen.getByText('deploy').closest('section') as HTMLElement
    expect(triggerFrame).toHaveClass('rounded-md', 'border')
    expect(triggerFrame.className).not.toContain('rounded-card')
    const triggerChip = screen.getByText('deploy')
    expect(triggerChip).toHaveClass('rounded-md', 'border', 'bg-transparent')
    expect(triggerChip.className).not.toContain('rounded-full')
    expect(triggerChip.className).not.toContain('bg-black/[0.04]')
    expect(triggerChip.className).not.toContain('dark:bg-white/[0.06]')
    expect(screen.getByText('Reusable guidance')).toBeInTheDocument()
    expect(screen.queryByText('Reusable instructions')).toBeNull()
    expect(
      screen.getByText('Read these reusable steps before using this skill.')
    ).toBeInTheDocument()
    expect(
      screen.getByText('Verify health checks, rollback notes, and user-facing release status.')
    ).toBeInTheDocument()
    const savedGuidancePreview = screen.getByText(
      'Verify health checks, rollback notes, and user-facing release status.'
    )
    expect(savedGuidancePreview).toHaveClass('rounded-md', 'border', 'bg-transparent')
    expect(savedGuidancePreview.className).not.toContain('rounded-card')
    expect(savedGuidancePreview.className).not.toContain('bg-black/[0.04]')
    expect(savedGuidancePreview.className).not.toContain('dark:bg-white/[0.04]')
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

    expect(screen.getByText('Check before use')).toBeInTheDocument()
    expect(screen.queryByText('Needs setup before use')).toBeNull()
    expect(screen.queryByText(/needs install/i)).toBeNull()
    expect(screen.getByText('No specific tool needed')).toBeInTheDocument()
    expect(screen.getByText('No specific tool needed')).toHaveAttribute(
      'title',
      'This skill does not require a specific file editing tool.'
    )
    expect(screen.queryByText('Works with any agent')).toBeNull()
    expect(screen.queryByTitle('No specific work tool is required.')).toBeNull()
    expect(
      screen.getByText('Ask an owner or admin to finish setup, then use this skill in a task.')
    ).toBeInTheDocument()
    expect(screen.queryByText(/install it/i)).toBeNull()
    expect(screen.getByText('Skills')).toBeInTheDocument()
    expect(screen.queryByText('Saved as saved guidance')).toBeNull()
    expect(screen.queryByText('Saved as a saved instruction')).toBeNull()
    expect(screen.queryByText('Saved instructions')).toBeNull()
    expect(screen.getByText('Open Skills again to show who keeps this updated')).toBeInTheDocument()
    expect(screen.queryByText('Unknown')).toBeNull()
    expect(screen.getByText('Latest saved copy')).toBeInTheDocument()
    expect(
      screen.getByText('Check the reusable guidance below before using this skill.')
    ).toBeInTheDocument()
    expect(
      screen.queryByText('Check the reusable instructions below before using this saved guidance.')
    ).toBeNull()
    expect(
      screen.getByText(
        'No reusable steps are saved yet. Add the steps agents should follow before using this skill.'
      )
    ).toBeInTheDocument()
    const noGuidanceNote = screen.getByText(
      'No reusable steps are saved yet. Add the steps agents should follow before using this skill.'
    )
    expect(noGuidanceNote).toHaveClass('rounded-md', 'border', 'bg-transparent')
    expect(noGuidanceNote.className).not.toContain('rounded-card')
    expect(noGuidanceNote.className).not.toContain('bg-black/[0.025]')
    expect(noGuidanceNote.className).not.toContain('dark:bg-white/[0.03]')
  })

  test('hides raw source and work tool slugs in skill details', () => {
    render(
      <SkillDetailModal
        skill={{
          ...baseSkill,
          plugin: '@example/team_skill_pack',
          marketplace: 'private_beta_scope',
          cliTool: 'future_tool_alpha',
        }}
        onClose={() => {}}
      />
    )

    expect(screen.getByText('Skills')).toBeInTheDocument()
    expect(screen.getByText('Check who can use this')).toBeInTheDocument()
    expect(screen.queryByText('Check saved guidance access')).toBeNull()
    expect(screen.getByText('Check the file-editing app in Settings')).toBeInTheDocument()
    expect(screen.getByText('Check the file-editing app in Settings')).toHaveAttribute(
      'title',
      'Open Settings, check the file-editing app, then use this skill.'
    )
    expect(screen.queryByText('Check work tool in Settings')).toBeNull()
    expect(screen.queryByText('Check required tool in Settings')).toBeNull()
    expect(screen.queryByText('Check file editing tool in Settings')).toBeNull()
    expect(screen.queryByTitle(/use this saved instruction/i)).toBeNull()
    expect(screen.queryByText('@example/team_skill_pack')).toBeNull()
    expect(screen.queryByText('private_beta_scope')).toBeNull()
    expect(screen.queryByText('Private Beta Scope')).toBeNull()
    expect(screen.queryByText('future_tool_alpha')).toBeNull()
    expect(screen.queryByText('Future Tool Alpha')).toBeNull()
  })

  test('closes from the beginner-friendly done action', () => {
    const onClose = vi.fn()
    render(<SkillDetailModal skill={baseSkill} onClose={onClose} />)

    fireEvent.click(screen.getByRole('button', { name: 'Done' }))

    expect(onClose).toHaveBeenCalledOnce()
  })
})
