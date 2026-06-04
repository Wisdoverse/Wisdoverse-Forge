import { render, screen, fireEvent, cleanup } from '@testing-library/react'
import { describe, it, expect, vi, afterEach } from 'vitest'
import { ChatComposer } from '@app/features/chat/ChatComposer'

afterEach(cleanup)

describe('ChatComposer', () => {
  it('calls onSend with trimmed content on Cmd+Enter', () => {
    const onSend = vi.fn()
    render(<ChatComposer onSend={onSend} onAbort={() => {}} streaming={false} disabled={false} />)
    const ta = screen.getByRole('textbox')
    fireEvent.change(ta, { target: { value: '  hello  ' } })
    fireEvent.keyDown(ta, { key: 'Enter', metaKey: true })
    expect(onSend).toHaveBeenCalledWith('hello')
  })

  it('calls onSend with Ctrl+Enter', () => {
    const onSend = vi.fn()
    render(<ChatComposer onSend={onSend} onAbort={() => {}} streaming={false} disabled={false} />)
    const ta = screen.getByRole('textbox')
    fireEvent.change(ta, { target: { value: 'hi' } })
    fireEvent.keyDown(ta, { key: 'Enter', ctrlKey: true })
    expect(onSend).toHaveBeenCalledWith('hi')
  })

  it('does not send on bare Enter', () => {
    const onSend = vi.fn()
    render(<ChatComposer onSend={onSend} onAbort={() => {}} streaming={false} disabled={false} />)
    const ta = screen.getByRole('textbox')
    fireEvent.change(ta, { target: { value: 'hi' } })
    fireEvent.keyDown(ta, { key: 'Enter' })
    expect(onSend).not.toHaveBeenCalled()
  })

  it('shows plain-language starting points for a first message', () => {
    render(<ChatComposer onSend={() => {}} onAbort={() => {}} streaming={false} disabled={false} />)

    expect(
      screen.getByText(/ask for a short summary, what is blocked, or the next safe step/i)
    ).toBeVisible()
    expect(screen.getByRole('textbox')).toHaveAccessibleDescription(
      /write one clear instruction or question.*ask for a short summary/i
    )
  })

  it('guides the user when Send is clicked without a message', () => {
    const onSend = vi.fn()
    render(<ChatComposer onSend={onSend} onAbort={() => {}} streaming={false} disabled={false} />)

    const textarea = screen.getByRole('textbox')
    fireEvent.click(screen.getByRole('button', { name: /send/i }))

    expect(onSend).not.toHaveBeenCalled()
    expect(screen.getByRole('alert')).toHaveTextContent(
      'Write a message before sending it to this agent. Try asking for a summary, what is blocked, or the next safe step.'
    )
    expect(textarea).toHaveFocus()

    fireEvent.change(textarea, { target: { value: 'review the latest task' } })
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('shows Stop button when streaming and triggers onAbort', () => {
    const onAbort = vi.fn()
    render(<ChatComposer onSend={() => {}} onAbort={onAbort} streaming={true} disabled={false} />)
    fireEvent.click(screen.getByRole('button', { name: /stop/i }))
    expect(onAbort).toHaveBeenCalled()
  })

  it('disables textarea + Send button when disabled', () => {
    render(
      <ChatComposer
        onSend={() => {}}
        onAbort={() => {}}
        streaming={false}
        disabled={true}
        disabledReason="This agent is offline. Start it before sending a message."
      />
    )
    expect(screen.getByRole('textbox')).toBeDisabled()
    expect(screen.getByRole('button', { name: /send/i })).toBeDisabled()
    expect(
      screen.getByText('This agent is offline. Start it before sending a message.')
    ).toBeVisible()
  })

  it('textarea is disabled while streaming', () => {
    render(<ChatComposer onSend={() => {}} onAbort={() => {}} streaming={true} disabled={false} />)
    expect(screen.getByRole('textbox')).toBeDisabled()
  })
})
