import { useCallback, useEffect, useRef, useState } from 'react'
import { ChevronLeft } from 'lucide-react'
import { FitAddon } from '@xterm/addon-fit'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { agentStatusLabel, agentToolLabel } from '@app/entities/agent'
import { cn } from '@app/shared/lib/utils'
import { useWebSocket } from '@app/shared/model/websocket.context'
import { NAV_KEYS, NUM_KEYS, UTIL_KEYS, type KeyDef } from './lib/terminalKeys'
import type { CliTool } from '@shared/types'

interface AgentTerminalTabProps {
  agentId: string
  agentName?: string
  cliTool?: CliTool
  containerId?: string
  agentStatus?: string
}

const KEY_GROUPS: KeyDef[][] = [NAV_KEYS, NUM_KEYS, UTIL_KEYS]
const LIVE_WORK_CONNECTION_NOTICE =
  'Live work notice: Connection dropped. Refresh this page first. If this agent stays Offline, open Overview, use Controls, and choose Restart agent.'

export function liveWorkToolLabel(cliTool?: CliTool): string {
  return agentToolLabel(cliTool)
}

export function liveWorkStatusLabel(status?: string): string {
  return agentStatusLabel(status)
}

export function AgentTerminalTab({
  agentId,
  agentName,
  cliTool,
  containerId,
  agentStatus,
}: AgentTerminalTabProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const terminalRef = useRef<Terminal | null>(null)
  const fitRef = useRef<FitAddon | null>(null)
  const attachedRef = useRef(false)
  const { status, send, subscribe } = useWebSocket()

  // Create + dispose xterm instance (one per agentId)
  useEffect(() => {
    const el = containerRef.current
    if (!el) return

    const term = new Terminal({
      disableStdin: false,
      cursorBlink: true,
      fontSize: 13,
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
      theme: {
        background: '#1a1a2e',
        foreground: '#e0e0e0',
        cursor: '#e0e0e0',
      },
      scrollback: 5000,
      convertEol: true,
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.open(el)

    terminalRef.current = term
    fitRef.current = fit

    const safeFit = () => {
      try {
        fit.fit()
      } catch {
        // Container may not be visible / sized yet
      }
    }

    // Forward keyboard input to server
    term.onData((data) => {
      if (attachedRef.current) {
        send({ type: 'terminal_data', payload: { agentId, data } })
      }
    })

    // Forward pty resize to server
    term.onResize(({ cols, rows }) => {
      if (attachedRef.current) {
        send({ type: 'terminal_resize', payload: { agentId, cols, rows } })
      }
    })

    // First layout pass
    const raf = requestAnimationFrame(safeFit)
    const ro = new ResizeObserver(safeFit)
    ro.observe(el)

    return () => {
      cancelAnimationFrame(raf)
      if (attachedRef.current) {
        send({ type: 'terminal_detach', payload: { agentId } })
        attachedRef.current = false
      }
      ro.disconnect()
      term.dispose()
      terminalRef.current = null
      fitRef.current = null
    }
  }, [agentId, containerId, send])

  // Attach stream once connected; re-attach automatically on reconnect
  useEffect(() => {
    if (status !== 'connected') return
    const term = terminalRef.current
    const fit = fitRef.current
    if (!term || !fit) return

    term.reset()
    try {
      fit.fit()
    } catch {
      // ignore — will retry via ResizeObserver
    }

    send({
      type: 'terminal_attach',
      payload: { agentId, cols: term.cols, rows: term.rows },
    })
    attachedRef.current = true

    return () => {
      if (attachedRef.current) {
        send({ type: 'terminal_detach', payload: { agentId } })
        attachedRef.current = false
      }
    }
  }, [status, agentId, containerId, send])

  // Subscribe to terminal_output frames from the gateway
  useEffect(
    () =>
      subscribe((raw) => {
        if (!raw || typeof raw !== 'object') return
        const msg = raw as {
          type?: string
          payload?: { agentId?: string; data?: string; message?: string }
        }
        if (msg.type !== 'terminal_output' && msg.type !== 'terminal_error') return
        if (msg.payload?.agentId !== agentId) return
        const term = terminalRef.current
        if (!term) return
        if (msg.type === 'terminal_error') {
          term.write(`\r\n${LIVE_WORK_CONNECTION_NOTICE}\r\n`)
          return
        }
        const data = msg.payload?.data
        if (!data) return
        try {
          const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0))
          term.write(bytes)
        } catch {
          // Ignore malformed base64 payloads
        }
      }),
    [subscribe, agentId]
  )

  const sendVirtualKeys = useCallback(
    (keys: string[]) => {
      if (!attachedRef.current) return
      send({ type: 'terminal_data', payload: { agentId, data: keys.join('') } })
    },
    [agentId, send]
  )

  const isLive = status === 'connected'
  const statusLabel = isLive ? 'Live' : status === 'connecting' ? 'Connecting' : 'Disconnected'
  const toolLabel = liveWorkToolLabel(cliTool)

  if (!containerId) {
    return (
      <div
        data-testid="agent-terminal-unavailable"
        className={cn(
          'flex flex-col gap-3 rounded-xl px-4 py-5',
          'shadow-card dark:shadow-card-dark bg-[#1a1a2e] text-white'
        )}
      >
        <div className="flex items-center gap-2 text-[11px] font-mono text-white/50">
          <span className="text-white/30">$</span>
          <span className="truncate">{agentName ?? agentId}</span>
          <span className="rounded bg-white/10 px-1.5 py-0.5 uppercase tracking-wide">
            {toolLabel}
          </span>
        </div>
        <div>
          <h3 className="text-sm font-semibold text-white">Live work is still starting</h3>
          <p className="mt-1 text-xs leading-relaxed text-white/60">
            Wait until this agent shows Ready. If it stays Offline, open Overview, use Controls, and
            start or restart this agent before using Live work.
          </p>
        </div>
        <div className="grid grid-cols-2 gap-2 text-[11px] text-white/55">
          <span>Agent status</span>
          <span className="text-white/80">{liveWorkStatusLabel(agentStatus)}</span>
          <span>Work tool</span>
          <span className="text-white/80">{toolLabel}</span>
          <span>Agent startup</span>
          <span className="text-white/80">Waiting for this agent</span>
        </div>
      </div>
    )
  }

  return (
    <div
      data-testid="agent-terminal-tab"
      className={cn(
        'flex flex-col h-[60vh] min-h-[400px] rounded-xl overflow-hidden',
        'shadow-card dark:shadow-card-dark bg-[#1a1a2e]'
      )}
    >
      {/* Header */}
      <div
        className={cn(
          'flex items-center gap-2 px-3 h-9 border-b border-white/5',
          'text-[11px] text-white/60 font-mono select-none bg-[#161629]'
        )}
      >
        <span className="text-white/40">$</span>
        <span className="truncate text-white/40">Agent: {(agentName ?? agentId).slice(0, 24)}</span>
        <span className="truncate text-white/25">Ready for live work</span>
        <span className="flex-1" />
        <span
          className={cn(
            'flex items-center gap-1.5 px-2 py-0.5 rounded text-[10px] font-semibold',
            'uppercase tracking-wider',
            isLive ? 'text-green-400 bg-green-400/10' : 'text-white/30'
          )}
        >
          <span
            className={cn(
              'w-1.5 h-1.5 rounded-full',
              isLive ? 'bg-green-400 shadow-[0_0_6px_rgba(74,222,128,0.4)]' : 'bg-white/20'
            )}
          />
          {statusLabel}
        </span>
      </div>

      {/* xterm surface */}
      <div ref={containerRef} className="flex-1 min-h-0 overflow-hidden px-2 pt-1 pb-0" />

      {/* Virtual key toolbar */}
      <KeyToolbar onKeys={sendVirtualKeys} disabled={!isLive} />
    </div>
  )
}

interface KeyToolbarProps {
  onKeys: (keys: string[]) => void
  disabled: boolean
}

function KeyToolbar({ onKeys, disabled }: KeyToolbarProps) {
  const [collapsed, setCollapsed] = useState(false)
  const toggleLabel = collapsed ? 'Show virtual keyboard' : 'Hide virtual keyboard'
  const keyboardHint = disabled
    ? 'Wait for live work before using keys'
    : 'Shortcut keys send to live work'

  return (
    <div
      className={cn(
        'flex items-center gap-1.5 px-2 py-1.5 min-h-10',
        'border-t border-white/5 bg-[#161629]'
      )}
    >
      <button
        type="button"
        onClick={() => setCollapsed((c) => !c)}
        className={cn(
          'h-7 min-w-[92px] flex items-center justify-center gap-1.5 rounded px-2 shrink-0',
          'text-white/30 hover:text-white/60 hover:bg-white/5 transition-colors'
        )}
        aria-label={toggleLabel}
        title={toggleLabel}
      >
        <ChevronLeft
          size={12}
          aria-hidden="true"
          className={cn('transition-transform', collapsed && 'rotate-180')}
        />
        <span className="text-[11px] font-medium">Keyboard</span>
      </button>

      {!collapsed && (
        <>
          <span className="hidden shrink-0 text-[10px] text-white/35 lg:inline">
            {keyboardHint}
          </span>
          <div className="flex items-center gap-1 overflow-x-auto flex-1">
            {KEY_GROUPS.map((group, gi) => (
              <div key={gi} className="flex items-center gap-0.5 shrink-0">
                {group.map((key, ki) => {
                  const wide = key.className?.includes('key-wide')
                  const danger = key.className?.includes('key-danger')
                  return (
                    <button
                      key={`${gi}-${ki}`}
                      type="button"
                      disabled={disabled}
                      onClick={() => onKeys(key.keys)}
                      title={key.label}
                      className={cn(
                        'h-[26px] px-1.5 rounded border font-mono text-[11px]',
                        'whitespace-nowrap transition-colors select-none',
                        wide ? 'min-w-[52px]' : 'min-w-[32px]',
                        danger
                          ? 'text-red-400 border-red-400/20 hover:bg-red-400/10 hover:border-red-400/30'
                          : 'text-white/60 border-white/10 bg-white/[0.04] hover:bg-white/10 hover:border-white/15 hover:text-white/85',
                        'disabled:opacity-40 disabled:cursor-not-allowed'
                      )}
                    >
                      {key.label}
                    </button>
                  )
                })}
                {gi < KEY_GROUPS.length - 1 && (
                  <div className="w-px h-4 bg-white/10 mx-1 shrink-0" />
                )}
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  )
}
