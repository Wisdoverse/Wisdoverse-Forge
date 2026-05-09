import { create } from 'zustand'
import type {
  BillingSubscription,
  BillingPlan,
  UsageMetric,
  BillingInvoice,
  CheckoutInput,
} from '@app/shared/api/legacy/billingApi'
import { getBillingApi } from '@app/shared/api/legacy'

// ============================================================================
// Types
// ============================================================================

interface BillingState {
  // Subscription
  subscription: BillingSubscription | null
  plan: BillingPlan | null
  subscriptionLoading: boolean
  subscriptionError: string | null

  // Usage
  usage: UsageMetric[]
  usageLoading: boolean
  usageError: string | null

  // Invoices
  invoices: BillingInvoice[]
  invoicesLoading: boolean
  invoicesError: string | null

  // Billing not configured flag
  billingNotConfigured: boolean

  // Actions
  loadSubscription: () => Promise<void>
  loadUsage: () => Promise<void>
  loadInvoices: () => Promise<void>
  loadAll: () => Promise<void>
  createCheckout: (input: CheckoutInput) => Promise<string | null>
  openPortal: () => Promise<string | null>
}

// ============================================================================
// Helpers
// ============================================================================

function isBillingNotConfigured(err: unknown): boolean {
  if (err instanceof Error) {
    const msg = err.message.toLowerCase()
    // 404 or "not configured" or "billing" + "disabled"
    if (msg.includes('404') || msg.includes('not configured') || msg.includes('not found')) {
      return true
    }
    // Check statusCode on BillingApiError
    const asAny = err as { statusCode?: number }
    if (asAny.statusCode === 404 || asAny.statusCode === 501 || asAny.statusCode === 503) {
      return true
    }
  }
  return false
}

function extractMessage(err: unknown, fallback: string): string {
  return err instanceof Error ? err.message : fallback
}

// ============================================================================
// Store
// ============================================================================

export const useBillingStore = create<BillingState>((set) => ({
  subscription: null,
  plan: null,
  subscriptionLoading: false,
  subscriptionError: null,

  usage: [],
  usageLoading: false,
  usageError: null,

  invoices: [],
  invoicesLoading: false,
  invoicesError: null,

  billingNotConfigured: false,

  // ---------------------------------------------------------------------------
  // Subscription
  // ---------------------------------------------------------------------------

  loadSubscription: async () => {
    set({ subscriptionLoading: true, subscriptionError: null })
    try {
      const result = await getBillingApi().getSubscription()
      set({
        subscription: result.subscription,
        plan: result.plan,
        subscriptionLoading: false,
      })
    } catch (err) {
      if (isBillingNotConfigured(err)) {
        set({ subscriptionLoading: false, billingNotConfigured: true })
      } else {
        set({
          subscriptionLoading: false,
          subscriptionError: extractMessage(err, 'Failed to load subscription'),
        })
      }
    }
  },

  // ---------------------------------------------------------------------------
  // Usage
  // ---------------------------------------------------------------------------

  loadUsage: async () => {
    set({ usageLoading: true, usageError: null })
    try {
      const usage = await getBillingApi().getUsage()
      set({ usage, usageLoading: false })
    } catch (err) {
      if (isBillingNotConfigured(err)) {
        set({ usageLoading: false, billingNotConfigured: true })
      } else {
        set({
          usageLoading: false,
          usageError: extractMessage(err, 'Failed to load usage'),
        })
      }
    }
  },

  // ---------------------------------------------------------------------------
  // Invoices
  // ---------------------------------------------------------------------------

  loadInvoices: async () => {
    set({ invoicesLoading: true, invoicesError: null })
    try {
      const invoices = await getBillingApi().getInvoices(20)
      set({ invoices, invoicesLoading: false })
    } catch (err) {
      if (isBillingNotConfigured(err)) {
        set({ invoicesLoading: false, billingNotConfigured: true })
      } else {
        set({
          invoicesLoading: false,
          invoicesError: extractMessage(err, 'Failed to load invoices'),
        })
      }
    }
  },

  // ---------------------------------------------------------------------------
  // Load all
  // ---------------------------------------------------------------------------

  loadAll: async () => {
    const store = useBillingStore.getState()
    await Promise.all([store.loadSubscription(), store.loadUsage(), store.loadInvoices()])
  },

  // ---------------------------------------------------------------------------
  // Checkout
  // ---------------------------------------------------------------------------

  createCheckout: async (input) => {
    try {
      const result = await getBillingApi().createCheckout(input)
      return result.url
    } catch {
      return null
    }
  },

  // ---------------------------------------------------------------------------
  // Portal
  // ---------------------------------------------------------------------------

  openPortal: async () => {
    try {
      const result = await getBillingApi().createPortalAgent(window.location.href)
      return result.url
    } catch {
      return null
    }
  },
}))
