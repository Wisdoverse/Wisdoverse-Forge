import { create } from 'zustand'
import { orchestrationApi, type ContextFeatureSnapshot } from '@app/shared/api/orchestration'

const DISABLED_CONTEXT_FEATURES: ContextFeatureSnapshot = {
  governance: false,
  preview: false,
  injection: false,
  analytics: false,
}

interface ContextFeaturesState extends ContextFeatureSnapshot {
  loaded: boolean
  loading: boolean
  load: () => Promise<void>
  reset: () => void
}

export const useContextFeaturesStore = create<ContextFeaturesState>((set, get) => ({
  ...DISABLED_CONTEXT_FEATURES,
  loaded: false,
  loading: false,
  load: async () => {
    if (get().loading) return
    set({ loading: true })
    try {
      const features = await orchestrationApi.fetchContextFeatures()
      set({ ...features, loaded: true, loading: false })
    } catch {
      set({ ...DISABLED_CONTEXT_FEATURES, loaded: true, loading: false })
    }
  },
  reset: () => set({ ...DISABLED_CONTEXT_FEATURES, loaded: false, loading: false }),
}))
