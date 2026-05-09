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

  it('shows Stop button when streaming and triggers onAbort', () => {
    const onAbort = vi.fn()
    render(<ChatComposer onSend={() => {}} onAbort={onAbort} streaming={true} disabled={false} />)
    fireEvent.click(screen.getByRole('button', { name: /stop/i }))
    expect(onAbort).toHaveBeenCalled()
  })

  it('disables textarea + Send button when disabled', () => {
    render(<ChatComposer onSend={() => {}} onAbort={() => {}} streaming={false} disabled={true} />)
    expect(screen.getByRole('textbox')).toBeDisabled()
    expect(screen.getByRole('button', { name: /send/i })).toBeDisabled()
  })

  it('textarea is disabled while streaming', () => {
    render(<ChatComposer onSend={() => {}} onAbort={() => {}} streaming={true} disabled={false} />)
    expect(screen.getByRole('textbox')).toBeDisabled()
  })
})
