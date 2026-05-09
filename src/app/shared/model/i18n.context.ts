import { createContext, useContext } from 'react'

export type Language = 'en' | 'zh'

export interface I18nContextValue {
  language: Language
  setLanguage: (lang: Language) => void
}

export const I18nContext = createContext<I18nContextValue | null>(null)

export function useI18n(): I18nContextValue {
  const ctx = useContext(I18nContext)
  if (!ctx) throw new Error('useI18n must be used within I18nProvider')
  return ctx
}
