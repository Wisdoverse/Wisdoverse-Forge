import { beforeAll, afterAll } from 'vitest'
import fs from 'node:fs'
import path from 'node:path'

const TEST_TMP_ROOT = path.join(process.cwd(), 'coverage', '.tmp')
let testTmpDir = ''

const installLocalStoragePolyfill = () => {
  if (typeof window === 'undefined') return

  let storage: Storage | undefined
  try {
    storage = globalThis.localStorage
  } catch {
    storage = undefined
  }

  if (
    typeof storage?.getItem === 'function' &&
    typeof storage?.setItem === 'function' &&
    typeof storage?.removeItem === 'function' &&
    typeof storage?.clear === 'function'
  ) {
    return
  }

  const store: Record<string, string> = {}
  const polyfill = {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => {
      store[key] = String(value)
    },
    removeItem: (key: string) => {
      delete store[key]
    },
    clear: () => {
      for (const key of Object.keys(store)) delete store[key]
    },
    get length() {
      return Object.keys(store).length
    },
    key: (index: number) => Object.keys(store)[index] ?? null,
  } as Storage

  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: polyfill,
  })
  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    value: polyfill,
  })
}

installLocalStoragePolyfill()

beforeAll(() => {
  fs.mkdirSync(TEST_TMP_ROOT, { recursive: true })
  testTmpDir = fs.mkdtempSync(path.join(TEST_TMP_ROOT, 'af-test-'))

  process.env.NODE_ENV = 'test'
  process.env.JWT_SECRET = 'test-jwt-secret-that-is-at-least-43-characters-long-for-validation'
  process.env.JWT_SECRET_PREVIOUS = ''
  process.env.API_KEY_SALT = 'test-api-key-salt-value'
  process.env.LLM_ENCRYPTION_KEY = 'test-llm-encryption-key-32chars!'
  process.env.DATABASE_URL = 'postgresql://test:test@localhost:5432/agentforge_test'
  process.env.REDIS_URL = ''
  process.env.STORAGE_PROVIDER = 'local'
  process.env.STORAGE_LOCAL_PATH = testTmpDir
  process.env.CONTAINER_ENABLED = 'false'
  process.env.LLM_GATEWAY_ENABLED = 'false'
  process.env.PLATFORM_GRPC_ENABLED = 'false'
  process.env.NATS_URL = ''
})

afterAll(() => {
  if (testTmpDir) {
    fs.rmSync(testTmpDir, { recursive: true, force: true })
  }
})
