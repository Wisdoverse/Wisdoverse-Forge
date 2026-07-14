import { describe, expect, test } from 'vitest'
import { createAgentWorkLaneErrorMessage } from '@app/features/agents/model/createAgentWorkLaneErrorMessage'

describe('createAgentWorkLaneErrorMessage', () => {
  test('turns sign-in failures into a New agent retry step', () => {
    const message = createAgentWorkLaneErrorMessage(new Error('HTTP 401: Unauthorized'))

    expect(message).toBe(
      'Sign in again, open New agent again, and set up the place for new tasks again. The place was not created.'
    )
    expect(message).not.toContain('Create Agent')
    expect(message).not.toContain('Unauthorized')
    expect(message).not.toContain('waiting place')
  })

  test('turns permission failures into an owner or admin next step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('HTTP 403: Forbidden'))).toBe(
      'Ask an owner or admin to let you set up the place for new tasks in this project. The place was not created.'
    )
  })

  test('turns structured permission failures into an owner or admin next step', () => {
    const message = createAgentWorkLaneErrorMessage({
      statusCode: '403',
      detail: 'owner role required',
    })

    expect(message).toBe(
      'Ask an owner or admin to let you set up the place for new tasks in this project. The place was not created.'
    )
    expect(message).not.toContain('owner role required')
    expect(message).not.toContain('waiting place')
  })

  test('turns role-required failures into an owner or admin next step', () => {
    const message = createAgentWorkLaneErrorMessage('owner role required')

    expect(message).toBe(
      'Ask an owner or admin to let you set up the place for new tasks in this project. The place was not created.'
    )
    expect(message).not.toContain('owner role required')
    expect(message).not.toContain('waiting place')
  })

  test('maps nested role-required failures into an owner or admin next step', () => {
    const message = createAgentWorkLaneErrorMessage({
      error: { message: 'owner role required' },
    })

    expect(message).toBe(
      'Ask an owner or admin to let you set up the place for new tasks in this project. The place was not created.'
    )
    expect(message).not.toContain('owner role required')
  })

  test('turns duplicate task-queue failures into an existing-queue step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('API 409: duplicate lane'))).toBe(
      'Open the project again, then choose the existing place for new tasks. A starter place may already exist.'
    )
  })

  test('turns structured duplicate failures into an existing-place step', () => {
    const message = createAgentWorkLaneErrorMessage({
      code: '409',
      reason: 'duplicate lane',
    })

    expect(message).toBe(
      'Open the project again, then choose the existing place for new tasks. A starter place may already exist.'
    )
    expect(message).not.toContain('duplicate lane')
    expect(message).not.toContain('waiting place')
    expect(message).not.toContain('Refresh the project')
  })

  test('turns missing projects into a navigable New agent step', () => {
    const message = createAgentWorkLaneErrorMessage({ code: '404' })

    expect(message).toBe(
      'Open New agent, choose the project again, then set up the place for new tasks. The place was not created because the selected project may have changed or been removed.'
    )
    expect(message).not.toContain('Refresh this page')
  })

  test('turns invalid task-queue setup into a project selection step', () => {
    expect(createAgentWorkLaneErrorMessage(new Error('HTTP 422: validation failed'))).toBe(
      'Choose a project first, then set up the place for new tasks again. The place was not created.'
    )
  })

  test('keeps unformatted service failures on the task queue recovery path', () => {
    const message = createAgentWorkLaneErrorMessage(
      new Error('database unavailable during validation while creating task queue')
    )

    expect(message).toBe(
      'Wait a few minutes, then set up the place for new tasks again. Forge could not create the place right now. If it still fails, ask an owner or admin to check places in this project.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('Choose a project first')
  })

  test('turns network failures into a connection retry path', () => {
    const message = createAgentWorkLaneErrorMessage(new TypeError('Failed to fetch'))

    expect(message).toBe(
      'Check your connection, then set up the place for new tasks again. Forge could not connect while creating the place.'
    )
    expect(message).not.toContain('Failed to fetch')
    expect(message).not.toContain('waiting place')
  })

  test('turns service failures into a safe retry and owner check', () => {
    const message = createAgentWorkLaneErrorMessage(new Error('HTTP 500: database unavailable'))

    expect(message).toBe(
      'Wait a few minutes, then set up the place for new tasks again. Forge could not create the place right now. If it still fails, ask an owner or admin to check places in this project.'
    )
    expect(message).not.toContain('HTTP 500')
    expect(message).not.toContain('platform')
    expect(message).not.toContain('waiting place')
    expect(message).not.toContain('task routing setup')
  })

  test('turns structured service failures into a safe retry and owner check', () => {
    const message = createAgentWorkLaneErrorMessage({
      status: '503',
      message: 'database unavailable',
    })

    expect(message).toBe(
      'Wait a few minutes, then set up the place for new tasks again. Forge could not create the place right now. If it still fails, ask an owner or admin to check places in this project.'
    )
    expect(message).not.toContain('database unavailable')
    expect(message).not.toContain('503')
    expect(message).not.toContain('waiting place')
    expect(message).not.toContain('task routing setup')
  })
})
