/**
 * react-i18next initialization
 *
 * Bridges FSD-local locale files into react-i18next.
 * Language preference is stored in localStorage under 'af:lang'.
 */

import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import { en } from '@app/shared/i18n/locales/en'
import { zh } from '@app/shared/i18n/locales/zh'

const LANG_KEY = 'af:lang'

function detectLanguage(): string {
  if (typeof localStorage !== 'undefined') {
    const stored = localStorage.getItem(LANG_KEY)
    if (stored === 'en' || stored === 'zh') return stored
  }
  if (typeof navigator !== 'undefined') {
    const lang = navigator.language.split('-')[0]
    if (lang === 'zh') return 'zh'
  }
  return 'en'
}

void i18n.use(initReactI18next).init({
  resources: {
    en: { translation: en },
    zh: { translation: zh },
  },
  lng: detectLanguage(),
  fallbackLng: 'en',
  interpolation: {
    // React already escapes values
    escapeValue: false,
  },
})

export { i18n, LANG_KEY }
