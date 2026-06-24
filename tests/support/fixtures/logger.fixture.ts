import { vi } from 'vitest'

export interface MockLogger {
  info: ReturnType<typeof vi.fn>
  warn: ReturnType<typeof vi.fn>
  error: ReturnType<typeof vi.fn>
  debug: ReturnType<typeof vi.fn>
  trace: ReturnType<typeof vi.fn>
  fatal: ReturnType<typeof vi.fn>
  child: ReturnType<typeof vi.fn>
  level: string
}

export function createMockLogger(): MockLogger {
  const logger: MockLogger = {
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
    debug: vi.fn(),
    trace: vi.fn(),
    fatal: vi.fn(),
    child: vi.fn(),
    level: 'silent',
  }
  logger.child.mockReturnValue(logger)
  return logger
}
