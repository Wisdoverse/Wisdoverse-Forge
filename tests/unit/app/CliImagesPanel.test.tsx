import { afterEach, describe, expect, test, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { CliImagesPanel } from '@app/features/admin/CliImagesPanel'
import {
  useAdminStore,
  type CliImageStatus,
  type CliImageTool,
} from '@app/shared/model/admin.store'

const originalAdminState = useAdminStore.getState()

afterEach(() => {
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
        total: 3,
        succeeded: 1,
        failed: 2,
        skippedBusy: 0,
        results: [
          { agentId: 'a1', ok: true, stopped: false },
          { agentId: 'a2', ok: false, stopped: true, error: 'respawn failed' },
          { agentId: 'a3', ok: false, stopped: false, error: 'stop failed' },
        ],
      },
    })

    render(<CliImagesPanel />)
    expect(screen.getByText('Last roll: codex')).toBeDefined()
    expect(screen.getByText(/1 of 3 agents respawned/)).toBeDefined()
    // start-fail → "now stopped"; stop-fail → unconfirmed post-condition (may be
    // running on the old image OR already down after a partial stop).
    expect(screen.getByText(/did not respawn and .* now stopped/)).toBeDefined()
    expect(
      screen.getByText(/could not be stopped cleanly.*may still be running on.*the previous image/s)
    ).toBeDefined()
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

  // --------------------------------------------------------------------
  // claude local build
  // --------------------------------------------------------------------

  test('claude row shows the local-build chip, versions, and a one-click build', () => {
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

    expect(screen.getByText('Local build')).toBeDefined()
    expect(screen.getByText('Update available')).toBeDefined()
    // installed vs latest, in plain versions (claude has no registry digests).
    expect(screen.getByText(/installed: v2\.1\.100/)).toBeDefined()
    expect(screen.getByText(/latest on npm: v2\.1\.173/)).toBeDefined()

    // ONE click builds — no confirm step (image-level, agents untouched).
    const build = screen.getByRole('button', { name: 'Build v2.1.173' })
    fireEvent.click(build)
    expect(buildClaudeImage).toHaveBeenCalledOnce()
  })

  test('claude row disables the build button with a progress label while building', () => {
    useAdminStore.setState({
      ...originalAdminState,
      cliImages: sampleStatus({ tools: [claudeTool({ building: true })] }),
      cliImagesLoading: false,
      cliImagesError: null,
      loadCliImages: vi.fn(),
    })

    render(<CliImagesPanel />)

    expect(screen.getByRole('button', { name: 'Building…' })).toBeDisabled()
    expect(screen.queryByRole('button', { name: /Build v/ })).toBeNull()
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

    expect(screen.getByText(/Auto-build on/)).toBeDefined()
    expect(screen.queryByRole('button', { name: 'Roll onto new image' })).toBeNull()
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
    expect(screen.queryByRole('button', { name: /Build/ })).toBeNull()
  })

  test('claude never-checked still offers Build latest (works with checks off)', () => {
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

    const build = screen.getByRole('button', { name: 'Build latest' })
    fireEvent.click(build)
    expect(buildClaudeImage).toHaveBeenCalledOnce()
  })

  test('claude failed check offers a Build latest retry', () => {
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

    expect(screen.getByText(/npm registry timeout/)).toBeDefined()
    expect(screen.getByRole('button', { name: 'Build latest' })).toBeDefined()
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
    expect(screen.getByText(/The build could not be started/i)).toBeDefined()
    expect(screen.getByText(/already in progress/)).toBeDefined()
    // the row still renders for retry.
    expect(screen.getByRole('button', { name: 'Build v2.1.173' })).toBeDefined()
  })
})
