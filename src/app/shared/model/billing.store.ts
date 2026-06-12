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

type BillingErrorArea = 'subscription' | 'usage' | 'invoices'

const BILLING_AREA_LABEL: Record<BillingErrorArea, string> = {
  subscription: 'Plan and payment',
  usage: 'Usage',
  invoices: 'Invoices',
}

function isBillingNotConfigured(err: unknown): boolean {
  const code = statusCode(err)
  if (code === 404 || code === 501 || code === 503) {
    return true
  }

  const msg = structuredErrorText(err).toLowerCase()
  if (msg.includes('404') || msg.includes('not configured') || msg.includes('not found')) {
    return true
  }
  return false
}

function errorText(err: unknown): string {
  if (err instanceof Error) return err.message
  return typeof err === 'string' ? err : ''
}

function structuredErrorText(err: unknown): string {
  if (!err || typeof err !== 'object') return errorText(err)
  for (const key of ['serverError', 'detail', 'error', 'message', 'reason'] as const) {
    const value = (err as Record<string, unknown>)[key]
    if (typeof value === 'string' && value.trim()) return value
  }
  return errorText(err)
}

function statusCode(err: unknown): number | null {
  if (err && typeof err === 'object') {
    for (const key of ['statusCode', 'status', 'code'] as const) {
      const value = (err as Record<string, unknown>)[key]
      if (typeof value === 'number' && Number.isFinite(value)) return value
      if (typeof value === 'string' && /^\d{3}$/.test(value.trim())) {
        return Number.parseInt(value, 10)
      }
    }
  }

  const match = structuredErrorText(err).match(/\b(?:HTTP|API|Server error|Code:)\s*\(?(\d{3})\b/i)
  if (!match) return null
  const code = Number.parseInt(match[1] ?? '', 10)
  return Number.isFinite(code) ? code : null
}

function isNetworkError(err: unknown): boolean {
  const text = structuredErrorText(err).toLowerCase()
  return (
    err instanceof TypeError ||
    text.includes('failed to fetch') ||
    text.includes('network') ||
    text.includes('browser could not reach') ||
    text.includes('load failed')
  )
}

export function billingErrorMessage(err: unknown, area: BillingErrorArea): string {
  const base = `${BILLING_AREA_LABEL[area]} could not be loaded.`
  const text = structuredErrorText(err).toLowerCase()
  const code = statusCode(err)

  if (code === 401 || text.includes('sign in again') || text.includes('unauthorized')) {
    return `${base} Sign in again, then open Billing.`
  }
  if (code === 403 || text.includes('permission') || text.includes('forbidden')) {
    return `${base} Ask an owner or admin to give you billing access.`
  }
  if (code === 429 || text.includes('busy') || text.includes('too many')) {
    return `${base} Billing is busy. Wait a minute, then refresh this page.`
  }
  if (code != null && code >= 500) {
    return `${base} Forge could not load billing right now. Ask an owner or admin to check billing, then refresh this page.`
  }
  if (isNetworkError(err)) {
    return `${base} Forge could not connect while loading billing. Check your connection, then refresh this page.`
  }

  return `${base} Refresh this page. If it still fails, ask an owner or admin to check billing.`
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
          subscriptionError: billingErrorMessage(err, 'subscription'),
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
          usageError: billingErrorMessage(err, 'usage'),
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
          invoicesError: billingErrorMessage(err, 'invoices'),
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
