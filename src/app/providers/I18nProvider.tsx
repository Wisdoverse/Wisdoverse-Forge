/**
 * I18nProvider
 *
 * Initializes react-i18next and exposes a useI18n hook for language switching.
 * Must be mounted early in the tree (before any component that calls useTranslation).
 */

import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { i18n, LANG_KEY } from '../i18n'
import { I18nContext, type Language } from '@app/shared/model/i18n.context'

export function I18nProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<Language>(
    (i18n.language === 'zh' ? 'zh' : 'en') satisfies Language
  )

  const setLanguage = useCallback((lang: Language) => {
    setLanguageState(lang)
    void i18n.changeLanguage(lang)
    localStorage.setItem(LANG_KEY, lang)
  }, [])

  // Keep state in sync if i18n changes externally
  useEffect(() => {
    const handleChanged = (lng: string) => {
      setLanguageState(lng === 'zh' ? 'zh' : 'en')
    }
    i18n.on('languageChanged', handleChanged)
    return () => {
      i18n.off('languageChanged', handleChanged)
    }
  }, [])

  return <I18nContext.Provider value={{ language, setLanguage }}>{children}</I18nContext.Provider>
}
