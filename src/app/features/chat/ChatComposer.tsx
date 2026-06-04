import type React from 'react'
import { useId, useRef, useState } from 'react'
import { cn } from '@app/shared/lib/utils'

export interface ChatComposerProps {
  onSend: (content: string) => void
  onAbort: () => void
  streaming: boolean
  disabled: boolean
  disabledReason?: string
}

export function ChatComposer({
  onSend,
  onAbort,
  streaming,
  disabled,
  disabledReason,
}: ChatComposerProps) {
  const [value, setValue] = useState('')
  const [error, setError] = useState<string | null>(null)
  const textareaRef = useRef<HTMLTextAreaElement>(null)
  const inputId = useId()
  const helpId = `${inputId}-help`
  const examplesId = `${inputId}-examples`
  const errorId = `${inputId}-error`
  const describedBy = error ? `${helpId} ${examplesId} ${errorId}` : `${helpId} ${examplesId}`

  const statusText = streaming
    ? 'Agent is responding. Stop it if you need to change the message.'
    : disabled
      ? (disabledReason ?? 'Chat is not ready yet. Try again when this agent is online.')
      : 'Write one clear instruction or question, then send it to this agent.'

  function handleKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault()
      submitMessage()
    }
  }

  function handleClickSend() {
    submitMessage()
  }

  function submitMessage() {
    if (disabled || streaming) return

    const trimmed = value.trim()
    if (!trimmed) {
      setError(
        'Write a message before sending it to this agent. Try asking for a summary, what is blocked, or the next safe step.'
      )
      textareaRef.current?.focus()
      return
    }

    onSend(trimmed)
    setValue('')
    setError(null)
  }

  const sendDisabled = disabled || streaming

  return (
    <div
      className={cn(
        'bg-white dark:bg-[#2c2c2e] rounded-xl p-3 flex flex-col gap-2',
        'shadow-card dark:shadow-card-dark'
      )}
    >
      <div className="flex items-end gap-2">
        <label htmlFor={inputId} className="sr-only">
          Message this agent
        </label>
        <textarea
          id={inputId}
          ref={textareaRef}
          className={cn(
            'max-h-40 flex-1 resize-none bg-transparent text-sm outline-none',
            (disabled || streaming) && 'cursor-not-allowed opacity-50'
          )}
          placeholder="Ask this agent for the next step"
          value={value}
          rows={2}
          disabled={disabled || streaming}
          aria-invalid={error != null}
          aria-describedby={describedBy}
          onChange={(e) => {
            setValue(e.target.value)
            if (error) setError(null)
          }}
          onKeyDown={handleKeyDown}
        />
        {streaming ? (
          <button
            type="button"
            onClick={onAbort}
            className={cn('rounded-lg bg-apple-red px-3 py-2 text-xs font-medium text-white')}
          >
            Stop
          </button>
        ) : (
          <button
            type="button"
            disabled={sendDisabled}
            onClick={handleClickSend}
            className={cn(
              'rounded-lg px-3 py-2 text-xs font-medium text-white',
              'bg-gradient-to-br from-apple-blue to-apple-purple',
              sendDisabled && 'cursor-not-allowed opacity-50'
            )}
          >
            Send
          </button>
        )}
      </div>
      <p id={helpId} className="text-ui-caption text-secondary-light dark:text-secondary-dark">
        {statusText}
      </p>
      <p id={examplesId} className="text-ui-caption text-secondary-light dark:text-secondary-dark">
        Need a starting point? Ask for a short summary, what is blocked, or the next safe step.
      </p>
      {error && (
        <p id={errorId} role="alert" className="text-ui-caption font-medium text-apple-red">
          {error}
        </p>
      )}
    </div>
  )
}
