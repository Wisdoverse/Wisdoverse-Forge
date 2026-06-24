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
type BillingActionArea = 'checkout' | 'portal'

const BILLING_AREA_LABEL: Record<BillingErrorArea, string> = {
  subscription: 'Plan and payment',
  usage: 'Usage',
  invoices: 'Invoices',
}

const BILLING_ACTION_LABEL: Record<BillingActionArea, string> = {
  checkout: 'secure payment page',
  portal: 'billing management page',
}

const BILLING_ACTION_OWNER_CHECK: Record<BillingActionArea, string> = {
  checkout: 'billing',
  portal: 'billing access',
}
const RAW_SERVICE_DETAIL =
  /\b(database|sql|stack trace|traceback|exception|panic|internal server error)\b/i
const ERROR_TEXT_KEYS = ['serverError', 'detail', 'error', 'message', 'reason'] as const
const STATUS_CODE_KEYS = ['statusCode', 'status', 'code'] as const

function isBillingNotConfigured(err: unknown): boolean {
  const msg = structuredErrorText(err).toLowerCase()
  if (RAW_SERVICE_DETAIL.test(msg)) return false

  const code = statusCode(err)
  if (code === 404 || code === 501 || code === 503) {
    return true
  }

  if (msg.includes('404') || msg.includes('not configured') || msg.includes('not found')) {
    return true
  }
  return false
}

function errorText(err: unknown): string {
  if (err instanceof Error) return err.message
  return typeof err === 'string' ? err : ''
}

function payloadText(err: unknown, depth = 0): string {
  if (depth > 2) return ''
  const text = errorText(err)
  if (text.trim()) return text
  if (!err || typeof err !== 'object') return ''

  for (const key of ERROR_TEXT_KEYS) {
    const nested = payloadText((err as Record<string, unknown>)[key], depth + 1)
    if (nested.trim()) return nested
  }

  return ''
}

function structuredErrorText(err: unknown): string {
  return payloadText(err)
}

function payloadStatusCode(err: unknown, depth = 0): number | null {
  if (depth > 2 || !err || typeof err !== 'object') return null

  for (const key of STATUS_CODE_KEYS) {
    const value = (err as Record<string, unknown>)[key]
    if (typeof value === 'number' && Number.isFinite(value)) return value
    if (typeof value === 'string' && /^\d{3}$/.test(value.trim())) {
      return Number.parseInt(value, 10)
    }
  }

  for (const key of ERROR_TEXT_KEYS) {
    const nested = payloadStatusCode((err as Record<string, unknown>)[key], depth + 1)
    if (nested != null) return nested
  }

  return null
}

function statusCode(err: unknown): number | null {
  const structuredCode = payloadStatusCode(err)
  if (structuredCode != null) return structuredCode

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
  const target = BILLING_AREA_LABEL[area].toLowerCase()
  const base = `Choose Check billing again to load ${target}.`
  const text = structuredErrorText(err).toLowerCase()
  const code = statusCode(err)

  if (code === 401 || text.includes('sign in again') || text.includes('unauthorized')) {
    return `Sign in again, then open Billing and choose Check billing again to load ${target}.`
  }
  if (
    code === 403 ||
    text.includes('permission') ||
    text.includes('forbidden') ||
    text.includes('role required')
  ) {
    return `Ask an owner or admin to give you billing access, then choose Check billing again to load ${target}.`
  }
  if (code === 429 || text.includes('busy') || text.includes('too many')) {
    return `Wait a minute, then choose Check billing again to load ${target}. Billing is busy.`
  }
  if (code != null && code >= 500) {
    return `${base} If it still fails, ask an owner or admin to check billing.`
  }
  if (isNetworkError(err)) {
    return `Check your connection, then choose Check billing again to load ${target}. Forge could not connect while loading billing.`
  }

  return `${base} If it still fails, ask an owner or admin to check billing.`
}

export function billingActionErrorMessage(err: unknown, action: BillingActionArea): string {
  const target = BILLING_ACTION_LABEL[action]
  const retry = `try opening the ${target} again`
  const text = structuredErrorText(err).toLowerCase()
  const code = statusCode(err)

  if (code === 401 || text.includes('sign in again') || text.includes('unauthorized')) {
    return `Sign in again, then open Billing and ${retry}.`
  }
  if (
    code === 403 ||
    text.includes('permission') ||
    text.includes('forbidden') ||
    text.includes('role required')
  ) {
    return `Ask an owner or admin to give you billing access, then ${retry}.`
  }
  if (code === 429 || text.includes('busy') || text.includes('too many')) {
    return `Wait a minute, then ${retry}. Billing is busy.`
  }
  if (code != null && code >= 500) {
    return `Wait a few minutes, then ${retry}. If it still fails, ask an owner or admin to check billing.`
  }
  if (isNetworkError(err)) {
    return `Check your connection, then ${retry}. Forge could not connect while opening billing.`
  }

  return `Try opening the ${target} again. If it still fails, ask an owner or admin to check ${BILLING_ACTION_OWNER_CHECK[action]}.`
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
    const result = await getBillingApi().createCheckout(input)
    return result.url
  },

  // ---------------------------------------------------------------------------
  // Portal
  // ---------------------------------------------------------------------------

  openPortal: async () => {
    const result = await getBillingApi().createPortalAgent(window.location.href)
    return result.url
  },
}))
