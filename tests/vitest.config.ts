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
      // Keep the global baseline close to the current full-surface reality,
      // and let scripts/check-critical-coverage.cjs enforce higher-signal paths.
      thresholds: {
        lines: 11,
        branches: 9,
        functions: 11,
        statements: 11,
      },
      reporter: isCi ? ['text', 'cobertura'] : ['text', 'cobertura', 'html'],
      reportsDirectory: './coverage',
    },
  },
})
