/**
 * BillingAPI - Pure API layer for billing operations
 *
 * All functions are pure HTTP calls with no DOM/state dependencies.
 * Mirrors the server billing controller endpoints at /billing/*.
 */

import type { AuthHeaderProvider } from './AgentAPI'

// ============================================================================
// Types
// ============================================================================

export interface BillingPlan {
  id: string
  name: string
  description: string
  features: Record<string, boolean>
  limits: Record<string, number>
  price: { monthly: number; yearly: number; currency: string }
  popular?: boolean
}

export type SubscriptionStatus = 'active' | 'trialing' | 'past_due' | 'canceled' | 'unpaid'

export interface BillingSubscription {
  id: string
  planId: string
  status: SubscriptionStatus
  currentPeriodStart: string
  currentPeriodEnd: string
  cancelAtPeriodEnd: boolean
  canceledAt?: string
  trialStart?: string
  trialEnd?: string
}

export interface SubscriptionResult {
  subscription: BillingSubscription | null
  plan: BillingPlan | null
}

export interface CheckoutInput {
  planId: string
  billingCycle: 'monthly' | 'yearly'
  successUrl: string
  cancelUrl: string
  couponCode?: string
}

export interface CheckoutResult {
  agentId: string
  url: string
}

export interface UsageMetric {
  metric: string
  current: number
  limit: number
  percentUsed: number
}

export type InvoiceStatus = 'paid' | 'open' | 'void' | 'draft' | 'uncollectible'

export interface BillingInvoice {
  id: string
  number?: string
  status: InvoiceStatus
  amountDue: number
  amountPaid: number
  total: number
  currency: string
  periodStart?: string
  periodEnd?: string
  dueDate?: string
  paidAt?: string
  hostedInvoiceUrl?: string
  pdfUrl?: string
  createdAt: string
}

export type CancelResult = Pick<
  BillingSubscription,
  'id' | 'status' | 'cancelAtPeriodEnd' | 'canceledAt' | 'currentPeriodEnd'
>

// ============================================================================
// Error Class
// ============================================================================

export class BillingApiError extends Error {
  constructor(
    message: string,
    public readonly statusCode?: number,
    public readonly serverError?: string
  ) {
    super(message)
    this.name = 'BillingApiError'
  }
}

// ============================================================================
// API Factory
// ============================================================================

export function createBillingAPI(
  apiUrl: string,
  getAuthHeaders?: AuthHeaderProvider,
  fetchFn: typeof fetch = fetch
) {
  function headers(extra?: Record<string, string>): Record<string, string> {
    return {
      'Content-Type': 'application/json',
      ...(getAuthHeaders?.() ?? {}),
      ...extra,
    }
  }

  function headersNoBody(): Record<string, string> {
    return getAuthHeaders?.() ?? {}
  }

  async function parseResponse<T>(
    response: Response,
    extractData: (data: Record<string, unknown>) => T
  ): Promise<T> {
    if (!response.ok) {
      let serverError: string | undefined
      try {
        const errorData = await response.json()
        if (errorData) {
          const error = errorData.error
          serverError =
            typeof error === 'string'
              ? error
              : typeof error?.message === 'string'
                ? error.message
                : typeof errorData.message === 'string'
                  ? errorData.message
                  : undefined
        }
      } catch {
        /* not JSON */
      }
      throw new BillingApiError(
        serverError || `HTTP ${response.status}: ${response.statusText}`,
        response.status,
        serverError
      )
    }
    const data = await response.json()
    if (!data.ok) {
      throw new BillingApiError(
        data.error || data.message || 'Server returned error',
        response.status,
        data.error
      )
    }
    return extractData(data)
  }

  return {
    async getPlans(): Promise<BillingPlan[]> {
      const response = await fetchFn(`${apiUrl}/billing/plans`, { headers: headersNoBody() })
      return parseResponse(response, (data) => data.plans as BillingPlan[])
    },

    async getSubscription(): Promise<SubscriptionResult> {
      const response = await fetchFn(`${apiUrl}/billing/subscription`, { headers: headersNoBody() })
      return parseResponse(response, (data) => ({
        subscription: (data.subscription as BillingSubscription) ?? null,
        plan: (data.plan as BillingPlan) ?? null,
      }))
    },

    async createCheckout(input: CheckoutInput): Promise<CheckoutResult> {
      const response = await fetchFn(`${apiUrl}/billing/checkout`, {
        method: 'POST',
        headers: headers(),
        body: JSON.stringify(input),
      })
      return parseResponse(response, (data) => ({
        agentId: data.agentId as string,
        url: data.url as string,
      }))
    },

    async createPortalAgent(returnUrl: string): Promise<{ url: string }> {
      const response = await fetchFn(`${apiUrl}/billing/portal`, {
        method: 'POST',
        headers: headers(),
        body: JSON.stringify({ returnUrl }),
      })
      return parseResponse(response, (data) => ({ url: data.url as string }))
    },

    async getUsage(metric?: string): Promise<UsageMetric[]> {
      const params = metric ? `?metric=${encodeURIComponent(metric)}` : ''
      const response = await fetchFn(`${apiUrl}/billing/usage${params}`, {
        headers: headersNoBody(),
      })
      return parseResponse(response, (data) => data.usage as UsageMetric[])
    },

    async getInvoices(limit = 10): Promise<BillingInvoice[]> {
      const response = await fetchFn(`${apiUrl}/billing/invoices?limit=${limit}`, {
        headers: headersNoBody(),
      })
      return parseResponse(response, (data) => data.invoices as BillingInvoice[])
    },

    async cancelSubscription(immediately = false): Promise<CancelResult> {
      const response = await fetchFn(`${apiUrl}/billing/subscription/cancel`, {
        method: 'POST',
        headers: headers(),
        body: JSON.stringify({ immediately }),
      })
      return parseResponse(response, (data) => data.subscription as CancelResult)
    },

    async resumeSubscription(): Promise<CancelResult> {
      const response = await fetchFn(`${apiUrl}/billing/subscription/resume`, {
        method: 'POST',
        headers: headers(),
      })
      return parseResponse(response, (data) => data.subscription as CancelResult)
    },
  }
}

export type BillingAPI = ReturnType<typeof createBillingAPI>
