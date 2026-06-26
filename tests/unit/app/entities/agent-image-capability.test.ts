import { describe, expect, it } from 'vitest'
import { isImageCapable, modelSupportsImage } from '@app/entities/agent'

describe('modelSupportsImage (mirrors server agentforge_llm::vision)', () => {
  it('treats anthropic + google (canonical Gemini key) as vision for every model', () => {
    expect(modelSupportsImage('anthropic', 'claude-sonnet-4-6')).toBe(true)
    expect(modelSupportsImage('google', 'gemini-2.5-pro')).toBe(true)
    expect(modelSupportsImage('gemini', 'gemini-2.0-flash')).toBe(true) // defensive alias
  })

  it('allowlists only the vision openai families', () => {
    for (const m of [
      'gpt-4o',
      'gpt-4o-mini',
      'gpt-4-turbo',
      'gpt-4.1-mini',
      'gpt-5',
      'gpt-5-2025-01',
    ]) {
      expect(modelSupportsImage('openai', m)).toBe(true)
    }
    // Text-only openai models must be false (the gap codex flagged).
    for (const m of ['gpt-4', 'gpt-4-0613', 'gpt-3.5-turbo', 'text-davinci-003']) {
      expect(modelSupportsImage('openai', m)).toBe(false)
    }
  })

  it('is conservatively false for unknown providers / blanks', () => {
    expect(modelSupportsImage('mistral', 'mistral-large')).toBe(false)
    expect(modelSupportsImage('', '')).toBe(false)
    expect(modelSupportsImage(null, null)).toBe(false)
  })

  it('normalizes case + surrounding whitespace on BOTH provider and model (server parity)', () => {
    // Server does provider.trim().to_ascii_lowercase() and the same on model
    // before the prefix match (agentforge_llm::vision). The UI must agree so it
    // does not silently hide an upload the backend would accept for an
    // unnormalized slug like "GPT-4o" or " openai ".
    expect(modelSupportsImage('openai', 'GPT-4o')).toBe(true)
    expect(modelSupportsImage('OpenAI', 'GPT-4O')).toBe(true)
    expect(modelSupportsImage('openai', ' gpt-4o ')).toBe(true)
    expect(modelSupportsImage(' openai ', 'gpt-4o')).toBe(true)
    // ...and a normalized text-only model is still correctly false.
    expect(modelSupportsImage('OPENAI', ' GPT-4 ')).toBe(false)
  })
})

describe('isImageCapable (quick-message composer affordance)', () => {
  it('offers upload only for provider/API agents on a vision-capable model', () => {
    expect(isImageCapable({ cliTool: undefined, provider: 'openai', model: 'gpt-4o' })).toBe(true)
    expect(
      isImageCapable({ cliTool: undefined, provider: 'anthropic', model: 'claude-opus-4-8' })
    ).toBe(true)
  })

  it('does NOT offer upload for a text-only model on a vision provider', () => {
    // Provider is vision-capable, but the model is not — must match the server gate.
    expect(isImageCapable({ cliTool: undefined, provider: 'openai', model: 'gpt-4' })).toBe(false)
    expect(isImageCapable({ cliTool: undefined, provider: 'openai', model: 'gpt-3.5-turbo' })).toBe(
      false
    )
  })

  it('never offers upload for a CLI agent (images ride the task path)', () => {
    expect(
      isImageCapable({ cliTool: 'claude', provider: 'anthropic', model: 'claude-opus-4-8' })
    ).toBe(false)
  })
})
