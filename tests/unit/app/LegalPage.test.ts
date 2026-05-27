import { afterEach, describe, expect, test } from 'vitest'
import { LegalPage } from '@app/shared/ui/legal/LegalPage'

let page: LegalPage | null = null

afterEach(() => {
  page?.destroy()
  page = null
  document.body.innerHTML = ''
  history.replaceState(null, '', '/')
})

describe('LegalPage', () => {
  test('explains why first-time users are reviewing legal pages', () => {
    page = new LegalPage()
    page.show('privacy')

    expect(document.querySelector('.legal-summary')?.textContent).toContain(
      'Review what you agree to and how your workspace data is handled'
    )
    expect(document.querySelector('.legal-tab.active')?.textContent).toContain('Privacy Policy')
  })
})
