import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { CliImagesPanel } from '@app/features/admin/CliImagesPanel'
import { useAdminStore, type CliImageStatus } from '@app/shared/model/admin.store'

const originalAdminState = useAdminStore.getState()

afterEach(() => {
  cleanup()
  useAdminStore.setState(originalAdminState, true)
  vi.restoreAllMocks()
})

function sampleStatus(overrides: Partial<CliImageStatus> = {}): CliImageStatus {
  return {
    autoUpdateEnabled: true,
    pollIntervalSecs: 900,
    registry: 'ghcr.io/wisdoverse/wisdoverse-forge',
    imageTag: 'latest',
    tools: [
      {
        tool: 'codex',
        state: 'up_to_date',
        localDigest: 'sha256:aaaaaaaaaaaa1111',
        remoteDigest: 'sha256:aaaaaaaaaaaa1111',
        lastCheckedUnix: Math.floor(Date.now() / 1000) - 30,
        lastUpdatedUnix: null,
        lastError: null,
        agentsWithContainer: 2,
      },
      {
        tool: 'gemini',
        state: 'failed',
        localDigest: null,
        remoteDigest: null,
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
    expect(screen.getByText(/Tool package source is managed in Admin settings/i)).toBeDefined()
    expect(screen.queryByText(/ghcr\.io\/wisdoverse\/wisdoverse-forge/i)).toBeNull()
    expect(screen.queryByText(/agent-<tool>/i)).toBeNull()
    expect(screen.queryByText(/^source:/i)).toBeNull()
    expect(screen.getByText('Codex')).toBeDefined()
    expect(screen.getByText('Up to date')).toBeDefined()
    // failed tool shows a safe support note instead of raw updater text
    expect(screen.getByText('Check failed')).toBeDefined()
    expect(screen.getByText(/Support note:/i)).toBeDefined()
    expect(screen.getByText(/could not reach the tool package source/i)).toBeDefined()
    expect(screen.queryByText(/registry timeout/i)).toBeNull()
    expect(screen.queryByText(/Reported detail/i)).toBeNull()
    expect(screen.getByText('2 agents are currently using this tool')).toBeDefined()
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
    expect(screen.getByText(/3 removed/)).toBeDefined()
    expect(screen.getByText(/1 still in use/)).toBeDefined()
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

    expect(screen.getByText(/The last cleanup hit 1 error/i)).toBeDefined()
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
            localDigest: null,
            remoteDigest: null,
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

    expect(screen.getByText('Not checked — updates off')).toBeDefined()
    expect(
      screen.getByText('This tool has never been checked because automatic updates are off.')
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
    expect(screen.getByText(/may be out of date/i)).toBeDefined()
    expect(
      screen.getByText(/do not restart agents from this table until check now succeeds/i)
    ).toBeDefined()
    expect(screen.queryByText('HTTP 401')).toBeNull()
    expect(
      screen.getByText(/ask an owner or admin to check tool update setup/i)
    ).toBeDefined()
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

    expect(screen.getByText('Checking agent tool update status...')).toBeDefined()
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

    expect(screen.getByText(/The agent tool update status could not load/i)).toBeDefined()
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
    const rollButtons = screen.getAllByRole('button', { name: 'Restart on latest tool' })
    expect(rollButtons).toHaveLength(1)

    // First click only arms a destructive confirm — it must NOT roll yet.
    fireEvent.click(rollButtons[0])
    expect(rollCliImage).not.toHaveBeenCalled()
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
    expect(screen.getByText(/1 skipped \(busy\)/)).toBeDefined()
    expect(screen.getByText(/Restart again once they show Ready/i)).toBeDefined()
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
    expect(screen.getByText(/The restart could not be started/i)).toBeDefined()
    expect(screen.getByText(/Another restart is already running/i)).toBeDefined()
    expect(screen.queryByText(/a roll for this tool is already in progress/i)).toBeNull()
  })
})
