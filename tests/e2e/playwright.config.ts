/**
 * Wisdoverse Forge Playwright E2E configuration — canonical.
 *
 * One file, env-driven target. Real login via globalSetup; every test
 * inherits `storageState`. Never mocks auth.
 *
 * Usage:
 *   # local (Vite on 4002)
 *   npm run test:e2e
 *   # staging
 *   BASE_URL=https://forge.example.com npm run test:e2e
 *   # prod canary
 *   BASE_URL=https://forge.example.com npm run test:e2e
 */

import { defineConfig, devices } from '@playwright/test'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const here = path.dirname(fileURLToPath(import.meta.url))
const BASE_URL = process.env.BASE_URL ?? 'http://localhost:4002'
const STORAGE_STATE = path.resolve(here, '.auth/user.json')
const SKIP_AUTH_SETUP = process.env.E2E_SKIP_AUTH_SETUP === '1'

export default defineConfig({
  testDir: './specs',
  testMatch: [
    'react-app-smoke.spec.ts',
    'legacy-nav-golden.spec.ts',
    'auth-reset-password.spec.ts',
    'inbox-owner-notifications.spec.ts',
    'agent-terminal-cli-runtime.spec.ts',
    'provider-prompt.spec.ts',
    'orchestration-real-task.spec.ts',
    'webui-mobile.spec.ts',
    'context-features-disabled.spec.ts',
    'context-injection-codex.spec.ts',
    'context-injection-gemini.spec.ts',
    'context-injection-opencode.spec.ts',
    'context-tab.spec.ts',
    'context-preview.spec.ts',
    'approval-queue.spec.ts',
    'task-context-badges.spec.ts',
    'analytics-dashboard.spec.ts',
    'governance-audit.spec.ts',
  ],
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1,

  globalSetup: SKIP_AUTH_SETUP ? undefined : path.resolve(here, 'global-setup.ts'),

  reporter: [
    ['list'],
    ['html', { outputFolder: '../../playwright-report/e2e', open: 'never' }],
    ['json', { outputFile: '../../test-results/e2e/results.json' }],
  ],

  use: {
    baseURL: BASE_URL,
    storageState: SKIP_AUTH_SETUP ? undefined : STORAGE_STATE,
    trace: 'on-first-retry',
    screenshot: 'on',
    video: 'on-first-retry',
    actionTimeout: 15_000,
    navigationTimeout: 30_000,
  },

  timeout: 120_000,

  expect: {
    timeout: 15_000,
  },

  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 1920, height: 1080 },
        launchOptions: {
          executablePath:
            process.env.PLAYWRIGHT_CHROMIUM_PATH ??
            '/opt/pw-browsers/chromium-1208/chrome-linux64/chrome',
          args: ['--use-gl=swiftshader', '--enable-webgl', '--no-sandbox'],
        },
      },
    },
  ],

  outputDir: '../../test-results',
})
