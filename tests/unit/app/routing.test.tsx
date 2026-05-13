import { describe, test, expect, afterEach } from 'vitest'
import { render, screen, cleanup } from '@testing-library/react'
import { RouterProvider, createMemoryHistory } from '@tanstack/react-router'
import { createTestRouter } from './test-helpers'

afterEach(cleanup)

describe('Routing', () => {
  test('renders start page at /start', async () => {
    const router = createTestRouter(createMemoryHistory({ initialEntries: ['/start'] }))
    render(<RouterProvider router={router} />)
    expect(await screen.findByTestId('page-start')).toBeDefined()
  })

  test('renders tasks page at /tasks', async () => {
    const router = createTestRouter(createMemoryHistory({ initialEntries: ['/tasks'] }))
    render(<RouterProvider router={router} />)
    expect(await screen.findByTestId('page-tasks')).toBeDefined()
  })

  test('renders inbox page at /inbox', async () => {
    const router = createTestRouter(createMemoryHistory({ initialEntries: ['/inbox'] }))
    render(<RouterProvider router={router} />)
    expect(await screen.findByTestId('page-inbox')).toBeDefined()
  })

  test('renders agents page at /agents', async () => {
    const router = createTestRouter(createMemoryHistory({ initialEntries: ['/agents'] }))
    render(<RouterProvider router={router} />)
    expect(await screen.findByTestId('page-agents')).toBeDefined()
  })

  test('renders skills page at /skills', async () => {
    const router = createTestRouter(createMemoryHistory({ initialEntries: ['/skills'] }))
    render(<RouterProvider router={router} />)
    expect(await screen.findByTestId('page-skills')).toBeDefined()
  })

  test('renders settings page at /settings', async () => {
    const router = createTestRouter(createMemoryHistory({ initialEntries: ['/settings'] }))
    render(<RouterProvider router={router} />)
    expect(await screen.findByTestId('page-settings')).toBeDefined()
  })

  test('renders settings section pages without falling through to 404', async () => {
    const router = createTestRouter(createMemoryHistory({ initialEntries: ['/settings/projects'] }))
    render(<RouterProvider router={router} />)
    expect(await screen.findByTestId('page-settings')).toBeDefined()
  })

  test('redirects / to /tasks', async () => {
    const router = createTestRouter(createMemoryHistory({ initialEntries: ['/'] }))
    render(<RouterProvider router={router} />)
    expect(await screen.findByTestId('page-tasks')).toBeDefined()
  })
})
