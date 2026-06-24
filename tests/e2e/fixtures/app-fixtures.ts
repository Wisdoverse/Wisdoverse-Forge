/**
 * Custom Playwright test for the React app smoke suite.
 *
 * Extending the base `context` fixture so that:
 *   1. The standard API mock set is installed on the browser context BEFORE
 *      any page is created — pre-empting the race condition that made the
 *      first navigation stall on a `/api/v1/orgs` fetch under the old
 *      per-page `page.route` registration (issues #61 / #63).
 *   2. Per-test overrides can still stack on top of the defaults by calling
 *      `context.route` again from inside the test (see `overrideOrgs`).
 */

import { test as base, expect } from '@playwright/test'
import { installStandardMocks } from './mocks'

export const test = base.extend({
  context: async ({ context }, use) => {
    await installStandardMocks(context)
    await use(context)
  },
})

export { expect }
