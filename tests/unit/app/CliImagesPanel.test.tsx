import { afterEach, describe, expect, test, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { CliImagesPanel } from '@app/features/admin/CliImagesPanel'
import {
  useAdminStore,
  type CliImageStatus,
  type CliImageTool,
} from '@app/shared/model/admin.store'

const originalAdminState = useAdminStore.getState()

afterEach(() => {
  vi.useRealTimers()
  cleanup()
  useAdminStore.setState(originalAdminState, true)
  vi.restoreAllMocks()
})

/** A claude row in the local-build contract (no digests; npm versions). */
function claudeTool(overrides: Partial<CliImageTool> = {}): CliImageTool {
  return {
    tool: 'claude',
    state: 'update_available',
    updateMode: 'local_build',
    localDigest: null,
    remoteDigest: null,
    localVersion: '2.1.100',
    remoteVersion: '2.1.173',
    building: false,
    lastCheckedUnix: Math.floor(Date.now() / 1000) - 30,
    lastUpdatedUnix: null,
    lastError: null,
    agentsWithContainer: 0,
    ...overrides,
  }
}

function sampleStatus(overrides: Partial<CliImageStatus> = {}): CliImageStatus {
  return {
    autoUpdateEnabled: true,
    claudeAutoBuildEnabled: false,
    pollIntervalSecs: 900,
    registry: 'ghcr.io/wisdoverse/wisdoverse-forge',
    imageTag: 'latest',
    tools: [
      {
        tool: 'codex',
        state: 'up_to_date',
        updateMode: 'registry',
        localDigest: 'sha256:aaaaaaaaaaaa1111',
        remoteDigest: 'sha256:aaaaaaaaaaaa1111',
        localVersion: null,
        remoteVersion: null,
        building: false,
        lastCheckedUnix: Math.floor(Date.now() / 1000) - 30,
        lastUpdatedUnix: null,
        lastError: null,
        agentsWithContainer: 2,
      },
      {
        tool: 'gemini',
        state: 'failed',
        updateMode: 'registry',
        localDigest: null,
        remoteDigest: null,
        localVersion: null,
        remoteVersion: null,
        building: false,
        lastCheckedUnix: Math.floor(Date.now() / 1000) - 120,
        lastUpdatedUnix: null,
        lastError: 'registry timeout',
        agentsWithContainer: 0,
      },
    ],
    prune: {
      enabled: false,
      lastRunUnix: null,
      scanned: 0,
      removed: 0,
      skippedInUse: 0,
      skippedConflict: 0,
      errors: 0,
      lastError: null,
    },
    ...overrides,
  }
}

describe('CliImagesPanel', () => {
  test('loads on mount and shows per-tool state in plain language', async () => {
    const loadCliImages = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus(),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages,
    })

    render(<CliImagesPanel />)

    await waitFor(() => expect(loadCliImages).toHaveBeenCalledOnce())
    expect(screen.getByText('Agent tool updates')).toBeDefined()
    expect(screen.getByText('Automatic updates are on')).toBeDefined()
    expect(screen.getByText(/Where updates come from is managed in Admin settings/i)).toBeDefined()
    expect(screen.queryByText(/ghcr\.io\/wisdoverse\/wisdoverse-forge/i)).toBeNull()
    expect(screen.queryByText(/agent-<tool>/i)).toBeNull()
    expect(screen.queryByText(/^source:/i)).toBeNull()
    expect(screen.getByText('Codex')).toBeDefined()
    expect(screen.getByText('Up to date')).toBeDefined()
    expect(screen.getByText(/New agents use: Ready for new agents/i)).toBeDefined()
    expect(screen.getByText(/Latest check: No update needed/i)).toBeDefined()
    expect(screen.queryByText(/aaaaaaaaaaaa/i)).toBeNull()
    expect(screen.queryByText(/package ID/i)).toBeNull()
    expect(screen.queryByText('—')).toBeNull()
    // failed tool shows a safe next step instead of raw updater text
    expect(screen.getByText('Choose Check now')).toBeDefined()
    expect(screen.queryByText('Check failed')).toBeNull()
    expect(
      screen.getByText(/New agents use: Choose Check now to prepare the first tool/i)
    ).toBeDefined()
    expect(screen.getByText(/Latest check: Choose Check now to check for updates/i)).toBeDefined()
    expect(screen.getByText(/What to do:/i)).toBeDefined()
    expect(screen.getByText(/could not reach the tool package source/i)).toBeDefined()
    expect(screen.queryByText(/registry timeout/i)).toBeNull()
    expect(screen.queryByText(/Reported detail/i)).toBeNull()
    expect(screen.getByText('2 agents are currently using this tool')).toBeDefined()
  })

  test('uses the saved update cadence and pauses hidden tabs', async () => {
    vi.useFakeTimers()
    const loadCliImages = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({ pollIntervalSecs: 120 }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages,
    })

    render(<CliImagesPanel />)

    expect(loadCliImages).toHaveBeenCalledOnce()
    expect(screen.getByText(/about every 2 minutes while visible/i)).toBeDefined()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000)
    })
    expect(loadCliImages).toHaveBeenCalledOnce()

    await act(async () => {
      await vi.advanceTimersByTimeAsync(90_000)
    })
    expect(loadCliImages).toHaveBeenCalledTimes(2)

    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden')
    await act(async () => {
      await vi.advanceTimersByTimeAsync(120_000)
    })
    expect(loadCliImages).toHaveBeenCalledTimes(2)
  })

  test('explains how to recover when no agent tools can be checked', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({ tools: [] }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    const emptyState = screen.getByTestId('cli-images-empty-tools')
    expect(within(emptyState).getByText('Open Agents to add or enable a work tool')).toBeDefined()
    expect(within(emptyState).getByText(/open agents to add or enable a work tool/i)).toBeDefined()
    expect(within(emptyState).getByText(/choose check now before restarting agents/i)).toBeDefined()
    expect(screen.queryByText('No agent tools are configured for update checks.')).toBeNull()
    expect(screen.queryByText('No agent tools are ready for update checks')).toBeNull()
  })

  test('shows the prune sweep summary when pruning is enabled', () => {
    const loadCliImages = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({
        prune: {
          enabled: true,
          lastRunUnix: Math.floor(Date.now() / 1000) - 60,
          scanned: 4,
          removed: 3,
          skippedInUse: 1,
          skippedConflict: 0,
          errors: 0,
          lastError: null,
        },
      }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages,
    })

    render(<CliImagesPanel />)
    expect(screen.getByText('Old tool package cleanup')).toBeDefined()
    expect(screen.getByText(/3 old packages removed/)).toBeDefined()
    expect(screen.getByText(/1 kept because agents use them/)).toBeDefined()
    expect(screen.queryByText(/1 still in use/)).toBeNull()
  })

  test('hides raw cleanup errors from the prune summary', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({
        prune: {
          enabled: true,
          lastRunUnix: Math.floor(Date.now() / 1000) - 60,
          scanned: 4,
          removed: 1,
          skippedInUse: 1,
          skippedConflict: 0,
          errors: 1,
          lastError: 'no space left on /var/lib/docker/overlay2 secret token abc',
        },
      }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    expect(screen.getByText(/The last cleanup needs a check for 1 package/i)).toBeDefined()
    expect(screen.getByText(/1 need a check/i)).toBeDefined()
    expect(screen.queryByText(/hit 1 error/i)).toBeNull()
    expect(screen.getByText(/access setup problem/i)).toBeDefined()
    expect(screen.queryByText(/\/var\/lib\/docker/i)).toBeNull()
    expect(screen.queryByText(/overlay2/i)).toBeNull()
    expect(screen.queryByText(/secret token/i)).toBeNull()
  })

  test('flags prune configured on but never run (likely auto-update off)', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({
        prune: {
          enabled: true,
          lastRunUnix: null,
          scanned: 0,
          removed: 0,
          skippedInUse: 0,
          skippedConflict: 0,
          errors: 0,
          lastError: null,
        },
      }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)
    expect(screen.getByText(/no cleanup has run yet/i)).toBeDefined()
    expect(screen.getByText(/ask an owner or admin to confirm/i)).toBeDefined()
    expect(screen.getByText(/automatic tool updates are on/i)).toBeDefined()
    expect(screen.queryByText(/CLI_IMAGE_AUTO_UPDATE_ENABLED/)).toBeNull()
  })

  test('explains prune is off by default', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus(),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)
    expect(screen.getByText(/ask an owner or admin to turn on automatic cleanup/i)).toBeDefined()
    expect(screen.getByText(/old tool packages in Admin settings/i)).toBeDefined()
    expect(screen.queryByText(/CLI_IMAGE_PRUNE_ENABLED/)).toBeNull()
    expect(screen.queryByText(/deployment settings/i)).toBeNull()
  })

  test('explains the off state when auto-update is disabled', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({ autoUpdateEnabled: false }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    expect(screen.getByText('Automatic updates are off')).toBeDefined()
    expect(
      screen.getByText(/ask an owner or admin to turn on automatic tool updates/i)
    ).toBeDefined()
    expect(screen.getByText(/turn on automatic tool updates in Admin settings/i)).toBeDefined()
    expect(screen.queryByText(/CLI_IMAGE_AUTO_UPDATE_ENABLED/)).toBeNull()
    expect(screen.queryByText(/deployment settings/i)).toBeNull()
  })

  test('distinguishes never-checked-because-off from a healthy state', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({
        autoUpdateEnabled: false,
        tools: [
          {
            tool: 'codex',
            state: 'pending',
            updateMode: 'registry',
            localDigest: null,
            remoteDigest: null,
            localVersion: null,
            remoteVersion: null,
            building: false,
            lastCheckedUnix: null,
            lastUpdatedUnix: null,
            lastError: null,
            agentsWithContainer: 0,
          },
        ],
      }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    expect(screen.getByText('Check manually or turn updates on')).toBeDefined()
    expect(
      screen.getByText(
        'Automatic updates are off. Choose Check now for a manual check, or ask an owner or admin to turn updates on.'
      )
    ).toBeDefined()
  })

  test('warns that shown data is stale when a background refresh fails', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus(),
      cliImagesLoading: false,
      cliImagesError: 'HTTP 401',
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    // Stale data still renders, but with an explicit out-of-date warning.
    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(/may be out of date/i)
    expect(screen.getByText(/may be out of date/i)).toBeDefined()
    expect(
      screen.getByText(/do not restart agents from this table until check now succeeds/i)
    ).toBeDefined()
    expect(screen.queryByText('HTTP 401')).toBeNull()
    expect(screen.getByText(/ask an owner or admin to check tool update setup/i)).toBeDefined()
    expect(screen.queryByText(/admin service/i)).toBeNull()
    expect(screen.getByText('Codex')).toBeDefined()
  })

  test('uses clear loading copy while the first check runs', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: null,
      cliImagesLoading: true,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    expect(screen.getByText('Checking agent tool updates...')).toBeDefined()
    expect(screen.getByRole('button', { name: 'Checking...' })).toBeDisabled()
  })

  test('explains what to do when the status cannot load', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: null,
      cliImagesLoading: false,
      cliImagesError: 'HTTP 500',
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(/Choose Check now to load tool update status/i)
    expect(screen.getByText(/Choose Check now to load tool update status/i)).toBeDefined()
    expect(screen.queryByText('HTTP 500')).toBeNull()
    expect(screen.getByText(/choose check now again/i)).toBeDefined()
    expect(screen.getByText(/ask an owner or admin to check tool update setup/i)).toBeDefined()
    expect(screen.getByRole('button', { name: 'Check now' })).toBeDefined()
  })

  test('roll requires an explicit confirm before calling rollCliImage', () => {
    const rollCliImage = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus(), // codex has 2 agents-with-container; gemini has 0
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
      rollCliImage,
    })

    render(<CliImagesPanel />)

    // codex (agentsWithContainer=2) offers a roll; gemini (0) does not.
    const rollButtons = screen.getAllByRole('button', { name: 'Restart agents on latest tool' })
    expect(rollButtons).toHaveLength(1)

    // First click only arms a destructive confirm — it must NOT roll yet.
    fireEvent.click(rollButtons[0])
    expect(rollCliImage).not.toHaveBeenCalled()
    expect(screen.getByText('Restart 2 agents on the latest tool?')).toBeDefined()
    expect(screen.getByText(/These agents may briefly stop and reopen/i)).toBeDefined()
    expect(screen.getByText(/still working are left running/i)).toBeDefined()
    expect(screen.getByRole('button', { name: 'Keep agents running' })).toBeDefined()
    const confirm = screen.getByRole('button', { name: /Restart 2 agents now/ })

    // Confirm fires the roll for the right tool.
    fireEvent.click(confirm)
    expect(rollCliImage).toHaveBeenCalledWith('codex')
  })

  test('shows the last roll result including failed and busy agents', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus(),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
      cliImageRollResult: {
        tool: 'codex',
        total: 4,
        succeeded: 1,
        failed: 2,
        skippedBusy: 1,
        results: [
          { agentId: 'a1', ok: true, stopped: false },
          { agentId: 'a2', ok: false, stopped: true, error: 'respawn failed' },
          { agentId: 'a3', ok: false, stopped: false, error: 'stop failed' },
        ],
      },
    })

    render(<CliImagesPanel />)
    expect(screen.getByText('Last restart: Codex')).toBeDefined()
    expect(screen.getByText(/1 of 4 agents restarted/)).toBeDefined()
    expect(screen.getByText(/1 still working/)).toBeDefined()
    expect(screen.getByText(/2 need a retry/)).toBeDefined()
    expect(screen.getByText(/Restart again once they show Ready/i)).toBeDefined()
    expect(screen.getByText(/Agents still working were left running/i)).toBeDefined()
    expect(screen.queryByText(/skipped \(busy\)/i)).toBeNull()
    expect(screen.queryByText(/2 failed/i)).toBeNull()
    expect(screen.queryByText(/idle/i)).toBeNull()
    // start-fail → "now stopped"; stop-fail → unconfirmed post-condition (may be
    // running on the old image OR already down after a partial stop).
    expect(screen.getByText(/did not restart and .* now stopped/)).toBeDefined()
    expect(
      screen.getByText(
        /could not be stopped cleanly.*may still be running on.*the previous tool version/s
      )
    ).toBeDefined()
    expect(screen.getByText(/Some agents could not restart cleanly/i)).toBeDefined()
    expect(screen.queryByText(/respawn failed/i)).toBeNull()
    expect(screen.queryByText(/stop failed/i)).toBeNull()
    expect(screen.queryByText(/Reported detail/i)).toBeNull()
  })

  test('surfaces a roll error', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus(),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
      cliImageRollError: 'a roll for this tool is already in progress',
    })

    render(<CliImagesPanel />)
    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(/Check the note below, then restart affected agents again/i)
    expect(alert).toHaveTextContent(/Another restart is already running/i)
    expect(alert).not.toHaveTextContent(/The restart could not be started/i)
    expect(
      screen.getByText(/Check the note below, then restart affected agents again/i)
    ).toBeDefined()
    expect(screen.getByText(/Another restart is already running/i)).toBeDefined()
    expect(screen.queryByText(/a roll for this tool is already in progress/i)).toBeNull()
  })

  // --------------------------------------------------------------------
  // claude local build
  // --------------------------------------------------------------------

  test('claude row shows the local-prepare chip, versions, and a one-click prepare action', () => {
    const buildClaudeImage = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({ tools: [claudeTool()] }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
      buildClaudeImage,
    })

    render(<CliImagesPanel />)

    expect(screen.getByText('Prepared by Forge')).toBeDefined()
    expect(screen.getByText('Update available')).toBeDefined()
    // installed vs latest, in plain versions (claude has no registry digests).
    expect(screen.getByText(/Current version: v2\.1\.100/)).toBeDefined()
    expect(screen.getByText(/latest available: v2\.1\.173/)).toBeDefined()
    expect(screen.queryByText(/npm/i)).toBeNull()
    expect(screen.queryByText(/built here/i)).toBeNull()

    // ONE click prepares the tool package — no confirm step, agents untouched.
    const build = screen.getByRole('button', { name: 'Prepare v2.1.173' })
    fireEvent.click(build)
    expect(buildClaudeImage).toHaveBeenCalledOnce()
  })

  test('claude row disables the prepare button with a progress label while preparing', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({ tools: [claudeTool({ building: true })] }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    expect(screen.getByRole('button', { name: 'Preparing…' })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /Prepare v/ })).toBeNull()
    expect(screen.queryByText(/building on this server/i)).toBeNull()
    expect(screen.getByText(/Forge is preparing this tool package/i)).toBeDefined()
    expect(screen.getByText(/usually a few minutes/i)).toBeDefined()
  })

  test('claude row notes when auto-build is on and hides the roll control', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({
        claudeAutoBuildEnabled: true,
        // claude has live agents, but local-build tools are not rollable.
        tools: [claudeTool({ agentsWithContainer: 3 })],
      }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    expect(screen.getByText(/Prepares automatically/)).toBeDefined()
    expect(screen.queryByText(/versions build themselves/i)).toBeNull()
    expect(screen.queryByRole('button', { name: 'Restart agents on latest tool' })).toBeNull()
  })

  test('claude up to date shows no build button', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({
        tools: [claudeTool({ state: 'up_to_date', localVersion: '2.1.173' })],
      }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    expect(screen.getByText('Up to date')).toBeDefined()
    expect(screen.queryByRole('button', { name: /Prepare/ })).toBeNull()
  })

  test('claude never-checked still offers Prepare latest (works with checks off)', () => {
    // With auto-update off nothing ever flips claude to update_available — the
    // build endpoint resolves npm itself, so the row must still offer a build.
    const buildClaudeImage = vi.fn()
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({
        autoUpdateEnabled: false,
        tools: [
          claudeTool({
            state: 'pending',
            localVersion: null,
            remoteVersion: null,
            lastCheckedUnix: null,
          }),
        ],
      }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
      buildClaudeImage,
    })

    render(<CliImagesPanel />)

    const build = screen.getByRole('button', { name: 'Prepare latest' })
    fireEvent.click(build)
    expect(buildClaudeImage).toHaveBeenCalledOnce()
  })

  test('claude failed check offers a Prepare latest retry', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({
        tools: [claudeTool({ state: 'failed', lastError: 'npm registry timeout' })],
      }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    expect(screen.getByText(/What to do:/)).toBeDefined()
    expect(screen.getByText(/could not reach the tool package source/i)).toBeDefined()
    expect(screen.queryByText(/npm registry timeout/)).toBeNull()
    expect(screen.getByRole('button', { name: 'Prepare latest' })).toBeDefined()
  })

  test('claude missing version uses a next-step label instead of unknown', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({
        tools: [claudeTool({ state: 'failed', localVersion: null, lastError: 'build failed' })],
      }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    expect(screen.getByText(/Current version: Choose Check now to find it/i)).toBeDefined()
    expect(screen.queryByText(/Version not reported yet/i)).toBeNull()
    expect(screen.queryByText(/current version: unknown/i)).toBeNull()
  })

  test('surfaces a build error without losing the report', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({ tools: [claudeTool()] }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
      cliImageBuildError: 'a claude image build is already in progress',
    })

    render(<CliImagesPanel />)
    const alert = screen.getByRole('alert')
    expect(alert).toHaveAttribute('aria-live', 'polite')
    expect(alert).toHaveTextContent(/Check the note below, then choose Prepare again/i)
    expect(alert).not.toHaveTextContent(/The build could not be started/i)
    expect(screen.getByText(/Check the note below, then choose Prepare again/i)).toBeDefined()
    expect(screen.getByText(/Another tool update is already running/i)).toBeDefined()
    expect(screen.queryByText(/tool update service is busy/i)).toBeNull()
    expect(screen.queryByText(/a claude image build is already in progress/i)).toBeNull()
    // the row still renders for retry.
    expect(screen.getByRole('button', { name: 'Prepare v2.1.173' })).toBeDefined()
  })
})
