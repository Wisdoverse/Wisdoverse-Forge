import { defineConfig } from 'vitest/config'
import path from 'node:path'

const isCi = process.env.CI === 'true'

const aliases = {
  '@shared': path.resolve(__dirname, '../shared'),
  '@app': path.resolve(__dirname, '../src/app'),
  '@support': path.resolve(__dirname, 'support'),
}

export default defineConfig({
  resolve: {
    alias: aliases,
  },
  test: {
    root: path.resolve(__dirname, '..'),
    projects: [
      {
        resolve: { alias: aliases },
        test: {
          name: 'unit',
          include: ['tests/unit/**/*.test.ts'],
          exclude: [
            'tests/unit/app/**/*.test.ts',
            'tests/legacy/**/*.test.ts',
          ],
          environment: 'node',
          setupFiles: ['tests/support/setup/test-env.ts'],
          testTimeout: 10_000,
          hookTimeout: 10_000,
          mockReset: true,
          restoreMocks: true,
        },
      },
      {
        resolve: { alias: aliases },
        test: {
          name: 'integration',
          include: ['tests/integration/**/*.test.ts'],
          environment: 'node',
          setupFiles: ['tests/support/setup/global-setup.ts'],
          pool: 'forks',
          testTimeout: 30_000,
          hookTimeout: 15_000,
          mockReset: true,
          restoreMocks: true,
        },
      },
      {
        resolve: { alias: aliases },
        test: {
          name: 'unit-app',
          include: [
            'tests/unit/app/**/*.test.tsx',
            'tests/unit/app/**/*.test.ts',
            'tests/unit/components/**/*.test.tsx',
          ],
          environment: 'jsdom',
          setupFiles: ['tests/support/setup/jest-dom.ts'],
          pool: 'threads',
          testTimeout: 10_000,
          mockReset: true,
          restoreMocks: true,
        },
      },
    ],
    coverage: {
      provider: 'v8',
      include: ['src/app/**/*.ts', 'shared/**/*.ts'],
      exclude: [
        'node_modules',
        'dist',
        '**/*.test.ts',
        '**/*.spec.ts',
        '**/*.d.ts',
        'tests/**',
        'shared/generated/platform/**',
      ],
      // Default root coverage tracks all active frontend/shared code.
      // Thresholds sit a few points below the current full-surface reality
      // (~41/40/48/41 stmts/branch/funcs/lines) so a regression on a busy
      // code path fails fast without flagging incidental drift on a slow PR.
      // scripts/check-critical-coverage.cjs enforces higher-signal paths
      // (turn-builder, legacy API client) at 90/75 lines/branches.
      // Operators raising the bar should bump this block plus the targets
      // in `scripts/check-critical-coverage.cjs` together so the global
      // floor and the per-path ceilings move in lockstep.
      thresholds: {
        lines: 35,
        branches: 30,
        functions: 40,
        statements: 35,
      },
      reporter: isCi ? ['text', 'cobertura'] : ['text', 'cobertura', 'html'],
      reportsDirectory: './coverage',
    },
  },
})
