import { describe, expect, it } from 'vitest'
import {
  isApiAgent,
  isContainerAgent,
  isHostCliAgent,
  isImageCapable,
  isTaskImageCapable,
  modelSupportsImage,
  runtimeKindLabel,
  runtimeKindShortLabel,
} from '@app/entities/agent'

describe('runtime-kind specifications', () => {
  it('isHostCliAgent matches only runtimeKind="cli"', () => {
    expect(isHostCliAgent({ runtimeKind: 'cli' })).toBe(true)
    expect(isHostCliAgent({ runtimeKind: 'container' })).toBe(false)
    expect(isHostCliAgent({ runtimeKind: 'api' })).toBe(false)
  })

  it('no prefix fallback: runtimeKind="container" returns false regardless of runtimeId', () => {
    expect(isHostCliAgent({ runtimeKind: 'container' as const })).toBe(false)
  })

  it('undefined runtimeKind during rolling-deploy returns false for all three', () => {
    const u = { runtimeKind: undefined }
    expect(isHostCliAgent(u)).toBe(false)
    expect(isContainerAgent(u)).toBe(false)
    expect(isApiAgent(u)).toBe(false)
  })

  it('isContainerAgent exhaustive', () => {
    expect(isContainerAgent({ runtimeKind: 'container' })).toBe(true)
    expect(isContainerAgent({ runtimeKind: 'cli' })).toBe(false)
    expect(isContainerAgent({ runtimeKind: 'api' })).toBe(false)
  })

  it('isApiAgent exhaustive', () => {
    expect(isApiAgent({ runtimeKind: 'api' })).toBe(true)
    expect(isApiAgent({ runtimeKind: 'cli' })).toBe(false)
    expect(isApiAgent({ runtimeKind: 'container' })).toBe(false)
  })

  it('uses beginner-facing labels for chat-only agents', () => {
    expect(runtimeKindLabel('api')).toBe('Simple chat agent')
    expect(runtimeKindShortLabel('api')).toBe('Questions only')
  })

  it('uses result-facing labels for agents that can edit project files', () => {
    expect(runtimeKindLabel('container')).toBe('Project files')
    expect(runtimeKindShortLabel('container')).toBe('Project files')
  })

  it('uses beginner-facing labels when runtime kind is missing', () => {
    expect(runtimeKindLabel(undefined)).toBe('Check where it works')
    expect(runtimeKindShortLabel(undefined)).toBe('Check location')
  })

  it('isTaskImageCapable requires container runtime, a vision CLI, AND the image_input token', () => {
    const cap = (runtimeKind: 'container' | 'cli' | 'api' | undefined, capabilities?: string[]) =>
      isTaskImageCapable({ runtimeKind, capabilities })
    // Container + vision CLI + sidecar image_input token → allowed (case-insensitive).
    expect(cap('container', ['claude', 'image_input'])).toBe(true)
    expect(cap('container', ['codex', 'image_input'])).toBe(true)
    expect(cap('container', ['gemini', 'image_input'])).toBe(true)
    expect(cap('container', ['Claude', 'IMAGE_INPUT'])).toBe(true)
    // Vision CLI but NO image_input token (old/stale sidecar) → excluded, matching
    // the server dispatch gate so the user isn't offered an upload that fails.
    expect(cap('container', ['claude'])).toBe(false)
    // Host CLI reports the same tools but is off-host — excluded.
    expect(cap('cli', ['claude', 'image_input'])).toBe(false)
    // opencode has no vision; API/chat agents report no CLI tool.
    expect(cap('container', ['opencode', 'image_input'])).toBe(false)
    expect(cap('api', ['image_input'])).toBe(false)
    // Missing runtime kind (older server) degrades safely to hidden.
    expect(cap(undefined, ['claude', 'image_input'])).toBe(false)
    expect(isTaskImageCapable(undefined)).toBe(false)
  })

  it('modelSupportsImage normalizes provider and model casing/whitespace like the server gate', () => {
    // The server `agentforge_llm::vision::model_supports_image` trims + lowercases
    // BOTH provider and model. The UI must match so a vision model stored with
    // odd casing/whitespace (e.g. "GPT-4O", " gpt-4o ") is not wrongly hidden.
    expect(modelSupportsImage('openai', 'GPT-4O')).toBe(true)
    expect(modelSupportsImage('openai', '  gpt-4o ')).toBe(true)
    expect(modelSupportsImage(' OpenAI ', 'gpt-4o')).toBe(true)
    expect(modelSupportsImage('OPENAI', 'GPT-4.1-MINI')).toBe(true)
    // Vision families and all-vision providers (mirrors vision.rs cases).
    expect(modelSupportsImage('openai', 'gpt-4-turbo-2024-04-09')).toBe(true)
    expect(modelSupportsImage('openai', 'gpt-5-2025-08-01')).toBe(true)
    expect(modelSupportsImage('anthropic', 'Claude-Opus-4-8')).toBe(true)
    expect(modelSupportsImage('google', 'gemini-2.5-pro')).toBe(true)
    expect(modelSupportsImage('gemini', 'gemini-2.5-pro')).toBe(true) // defensive alias
    // Text-only / unknown / non-vision provider conservatively fall through to false.
    expect(modelSupportsImage('openai', 'gpt-3.5-turbo')).toBe(false)
    expect(modelSupportsImage('openai', 'gpt-4')).toBe(false)
    expect(modelSupportsImage('ollama', 'llava')).toBe(false)
    expect(modelSupportsImage('', '')).toBe(false)
    expect(modelSupportsImage(undefined, undefined)).toBe(false)
  })

  it('isImageCapable offers upload for a vision (provider, model) regardless of stored casing, hides it for CLI/text-only', () => {
    // Stored casing must not hide the affordance the server would accept.
    expect(isImageCapable({ cliTool: undefined, provider: 'openai', model: 'GPT-4O' })).toBe(true)
    expect(
      isImageCapable({ cliTool: undefined, provider: 'anthropic', model: 'claude-opus-4-8' })
    ).toBe(true)
    // Text-only model on a vision provider → hidden (server would reject the upload).
    expect(isImageCapable({ cliTool: undefined, provider: 'openai', model: 'gpt-3.5-turbo' })).toBe(
      false
    )
    // Container CLI agents never use the quick-message image path.
    expect(
      isImageCapable({ cliTool: 'claude', provider: 'anthropic', model: 'claude-opus-4-8' })
    ).toBe(false)
  })

  it('does not expose unknown runtime kind slugs', () => {
    expect(runtimeKindLabel('future_runtime' as never)).toBe('Check work location')
    expect(runtimeKindShortLabel('future_runtime' as never)).toBe('Check location')
    expect(runtimeKindLabel('future_runtime' as never)).not.toContain('future_runtime')
    expect(runtimeKindShortLabel('future_runtime' as never)).not.toContain('future_runtime')
  })
})
