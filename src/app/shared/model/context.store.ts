import { create } from 'zustand'

interface ContextState {
  pendingCandidateCount: number
  setPendingCandidateCount: (count: number) => void
  incrementPendingCandidateCount: () => void
  decrementPendingCandidateCount: () => void
  reset: () => void
}

const initialState = {
  pendingCandidateCount: 0,
}

export const useContextStore = create<ContextState>((set) => ({
  ...initialState,
  setPendingCandidateCount: (count) => set({ pendingCandidateCount: Math.max(0, count) }),
  incrementPendingCandidateCount: () =>
    set((state) => ({ pendingCandidateCount: state.pendingCandidateCount + 1 })),
  decrementPendingCandidateCount: () =>
    set((state) => ({ pendingCandidateCount: Math.max(0, state.pendingCandidateCount - 1) })),
  reset: () => set(initialState),
}))
