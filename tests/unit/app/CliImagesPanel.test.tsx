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
    expect(screen.getByText('CLI agent images')).toBeDefined()
    expect(screen.getByText('Automatic updates are on')).toBeDefined()
    expect(screen.getByText('codex')).toBeDefined()
    expect(screen.getByText('Up to date')).toBeDefined()
    // failed tool surfaces its reported detail
    expect(screen.getByText('Check failed')).toBeDefined()
    expect(screen.getByText(/registry timeout/)).toBeDefined()
    expect(screen.getByText('2 agents currently have a container for this tool')).toBeDefined()
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
    expect(screen.getByText('Old image cleanup')).toBeDefined()
    expect(screen.getByText(/3 removed/)).toBeDefined()
    expect(screen.getByText(/1 still in use/)).toBeDefined()
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
    expect(screen.getByText(/CLI_IMAGE_AUTO_UPDATE_ENABLED/)).toBeDefined()
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
    expect(screen.getByText(/CLI_IMAGE_PRUNE_ENABLED/)).toBeDefined()
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
    expect(screen.getByText(/CLI_IMAGE_AUTO_UPDATE_ENABLED/)).toBeDefined()
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
      screen.getByText('This image has never been checked because automatic updates are off.')
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
    expect(screen.getByText('HTTP 401')).toBeDefined()
    expect(screen.getByText('codex')).toBeDefined()
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

    expect(screen.getByText('Checking CLI image status...')).toBeDefined()
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

    expect(screen.getByText(/Could not load CLI image status/i)).toBeDefined()
    expect(screen.getByText('HTTP 500')).toBeDefined()
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
    const rollButtons = screen.getAllByRole('button', { name: 'Roll onto new image' })
    expect(rollButtons).toHaveLength(1)

    // First click only arms a destructive confirm — it must NOT roll yet.
    fireEvent.click(rollButtons[0])
    expect(rollCliImage).not.toHaveBeenCalled()
    const confirm = screen.getByRole('button', { name: /Interrupt 2 & roll/ })

    // Confirm fires the roll for the right tool.
    fireEvent.click(confirm)
    expect(rollCliImage).toHaveBeenCalledWith('codex')
  })

  test('shows the last roll result including failed agents', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus(),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
      cliImageRollResult: {
        tool: 'codex',
        total: 2,
        succeeded: 1,
        failed: 1,
        skippedBusy: 0,
        results: [
          { agentId: 'a1', ok: true },
          { agentId: 'a2', ok: false, error: 'docker unavailable' },
        ],
      },
    })

    render(<CliImagesPanel />)
    expect(screen.getByText('Last roll: codex')).toBeDefined()
    expect(screen.getByText(/1 of 2 agents respawned/)).toBeDefined()
    expect(screen.getByText(/did not respawn and .* now stopped/)).toBeDefined()
    expect(screen.getByText(/docker unavailable/)).toBeDefined()
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
    expect(screen.getByText(/The roll could not be started/i)).toBeDefined()
    expect(screen.getByText(/already in progress/)).toBeDefined()
  })
})
