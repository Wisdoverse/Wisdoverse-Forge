import { describe, expect, it } from 'vitest'

import { buildResetPasswordLoginHref, getResetTokenFromLocation } from '@app/routes/public-auth'

describe('public auth route helpers', () => {
  it('extracts reset tokens from raw search strings', () => {
    expect(getResetTokenFromLocation({ searchStr: '?reset_token=token-123&next=%2Ftasks' })).toBe(
      'token-123'
    )
  })

  it('falls back to parsed route search objects', () => {
    expect(getResetTokenFromLocation({ search: { reset_token: 'token-from-router' } })).toBe(
      'token-from-router'
    )
  })

  it('extracts reset tokens from full location hrefs', () => {
    expect(getResetTokenFromLocation({ href: '/?reset_token=token-from-href' })).toBe(
      'token-from-href'
    )
  })

  it('ignores missing or empty reset tokens', () => {
    expect(getResetTokenFromLocation({ searchStr: '?next=%2Ftasks' })).toBeNull()
    expect(getResetTokenFromLocation({ search: { reset_token: '   ' } })).toBeNull()
  })

  it('builds login hrefs that preserve the reset token', () => {
    expect(buildResetPasswordLoginHref('token with spaces')).toBe(
      '/login?reset_token=token%20with%20spaces'
    )
  })
})
