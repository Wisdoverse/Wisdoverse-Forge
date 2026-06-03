import { test, expect } from '@playwright/test'

test.describe('password reset links', () => {
  test.use({ storageState: { cookies: [], origins: [] } })

  const resetToken = 'e2e-reset-token-1234567890abcdef'
  const newPassword = 'ValidReset123!'

  test('opens the reset form from a legacy root reset-token link without login', async ({
    page,
  }) => {
    await page.goto(`/?reset_token=${resetToken}`)

    await expect(page.getByRole('heading', { name: 'Choose a new password' })).toBeVisible()
    await expect(page.locator('#reset-password')).toBeVisible()
    await expect(page.locator('#reset-confirm')).toBeVisible()
    await expect(page).toHaveURL(/\/login$/)
  })

  test('submits the preserved reset token with the new password', async ({ page }) => {
    let resetRequest: unknown = null
    await page.route('**/api/v1/auth/reset-password', async (route) => {
      resetRequest = JSON.parse(route.request().postData() ?? '{}')
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({ ok: true }),
      })
    })

    await page.goto(`/?reset_token=${resetToken}`)
    await page.locator('#reset-password').fill(newPassword)
    await page.locator('#reset-confirm').fill(newPassword)
    await page.getByRole('button', { name: 'Save new password' }).click()

    await expect(page.getByRole('heading', { name: 'Password updated' })).toBeVisible()
    expect(resetRequest).toEqual({ token: resetToken, newPassword })
  })

  test('shows a clear error for invalid reset tokens', async ({ page }) => {
    await page.route('**/api/v1/auth/reset-password', async (route) => {
      await route.fulfill({
        status: 400,
        contentType: 'application/json',
        body: JSON.stringify({
          ok: false,
          error: 'VALIDATION_ERROR',
          message: 'invalid or expired reset token',
        }),
      })
    })

    await page.goto(`/?reset_token=${resetToken}`)
    await page.locator('#reset-password').fill(newPassword)
    await page.locator('#reset-confirm').fill(newPassword)
    await page.getByRole('button', { name: 'Save new password' }).click()

    await expect(page.locator('#reset-error')).toHaveText('invalid or expired reset token')
  })
})
