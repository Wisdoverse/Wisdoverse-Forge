import type React from 'react'
import { useState } from 'react'
import { cn } from '@app/shared/lib/utils'

export interface ChatComposerProps {
  onSend: (content: string) => void
  onAbort: () => void
  streaming: boolean
  disabled: boolean
}

export function ChatComposer({ onSend, onAbort, streaming, disabled }: ChatComposerProps) {
  const [value, setValue] = useState('')

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      const trimmed = value.trim()
      if (!trimmed || streaming || disabled) return
      onSend(trimmed)
      setValue('')
    }
  }

  function handleClickSend() {
    const trimmed = value.trim()
    if (!trimmed || disabled || streaming) return
    onSend(trimmed)
    setValue('')
  }

  const sendDisabled = !value.trim() || disabled

  return (
    <div
      className={cn(
        'bg-white dark:bg-[#2c2c2e] rounded-xl p-3 flex items-end gap-2',
        'shadow-card dark:shadow-card-dark'
      )}
    >
      <textarea
        className={cn(
          'flex-1 resize-none bg-transparent text-sm outline-none max-h-40',
          (disabled || streaming) && 'opacity-50 cursor-not-allowed'
        )}
        placeholder="Type a message… (Cmd/Ctrl+Enter to send)"
        value={value}
        rows={2}
        disabled={disabled || streaming}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={handleKeyDown}
      />
      {streaming ? (
        <button
          type="button"
          onClick={onAbort}
          className={cn('px-3 py-2 rounded-lg text-xs font-medium bg-apple-red text-white')}
        >
          Stop
        </button>
      ) : (
        <button
          type="button"
          disabled={sendDisabled}
          onClick={handleClickSend}
          className={cn(
            'px-3 py-2 rounded-lg text-xs font-medium text-white',
            'bg-gradient-to-br from-apple-blue to-apple-purple',
            sendDisabled && 'opacity-50 cursor-not-allowed'
          )}
        >
          Send
        </button>
      )}
    </div>
  )
}
