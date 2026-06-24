import { beforeAll, afterAll } from 'vitest'
import './test-env.js'

beforeAll(async () => {
  console.log('[test-setup] Integration test environment ready')
})

afterAll(async () => {
  console.log('[test-setup] Integration test environment cleanup')
})
