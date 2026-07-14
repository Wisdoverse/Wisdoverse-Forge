import type React from 'react'
import { useId, useRef, useState } from 'react'
import { cn } from '@app/shared/lib/utils'
import { uiStyles } from '@app/shared/lib/uiStyles'

export interface ChatComposerProps {
  onSend: (content: string) => void
  onAbort: () => void
  streaming: boolean
  disabled: boolean
  disabledReason?: string
  disabledPlaceholder?: string
  helperText?: string
}

export function ChatComposer({
  onSend,
  onAbort,
  streaming,
  disabled,
  disabledReason,
  disabledPlaceholder,
  helperText,
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
      ? (disabledReason ??
        'Wait until this agent shows Ready, then send the message again from this chat.')
      : 'Write one clear message or question, then send it to this agent.'
  const placeholderText = disabled
    ? (disabledPlaceholder ?? 'Wait until this agent is Ready before sending.')
    : 'Ask this agent for the next step'
  const exampleText =
    helperText ??
    'Need a starting point? Ask for a short summary, what needs help, or the next safe step.'

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
        'Write a message before sending it to this agent. Try asking for a summary, what needs help, or the next safe step.'
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
    <div className={cn(uiStyles.card, 'flex flex-col gap-2 p-3')}>
      <div className="flex items-end gap-2">
        <label htmlFor={inputId} className="sr-only">
          Message this agent
        </label>
        <textarea
          id={inputId}
          ref={textareaRef}
          className={cn(
            uiStyles.input,
            'h-auto min-h-16 max-h-40 flex-1 resize-none py-2',
            (disabled || streaming) && 'cursor-not-allowed opacity-50'
          )}
          placeholder={placeholderText}
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
          <button type="button" onClick={onAbort} className={uiStyles.dangerConfirmButton}>
            Stop
          </button>
        ) : (
          <button
            type="button"
            disabled={sendDisabled}
            onClick={handleClickSend}
            className={uiStyles.primaryButton}
          >
            Send
          </button>
        )}
      </div>
      <p id={helpId} className="text-ui-caption text-secondary-light dark:text-secondary-dark">
        {statusText}
      </p>
      <p id={examplesId} className="text-ui-caption text-secondary-light dark:text-secondary-dark">
        {exampleText}
      </p>
      {error && (
        <p
          id={errorId}
          role="alert"
          aria-live="polite"
          className="text-ui-caption font-medium text-apple-red"
        >
          {error}
        </p>
      )}
    </div>
  )
}
