import { expect } from 'vitest'

interface ApiLikeResponse {
  statusCode: number
  body: string
  json: () => Record<string, unknown>
}

expect.extend({
  toBeApiSuccess(received: ApiLikeResponse) {
    let json: Record<string, unknown>
    try {
      json = received.json()
    } catch {
      return {
        pass: false,
        message: () => `expected API success response but body is not JSON: ${received.body}`,
      }
    }
    const pass = received.statusCode >= 200 && received.statusCode < 300 && json.ok === true
    return {
      pass,
      message: () =>
        pass
          ? `expected response NOT to be API success`
          : `expected API success (2xx + ok:true), got ${received.statusCode}: ${JSON.stringify(json)}`,
    }
  },

  toBeApiError(received: ApiLikeResponse, expectedStatus: number, expectedCode?: string) {
    let json: Record<string, unknown>
    try {
      json = received.json()
    } catch {
      return {
        pass: false,
        message: () => `expected API error response but body is not JSON: ${received.body}`,
      }
    }
    const statusMatch = received.statusCode === expectedStatus
    const okFalse = json.ok === false
    const codeMatch = !expectedCode || json.error === expectedCode
    const pass = statusMatch && okFalse && codeMatch
    return {
      pass,
      message: () =>
        pass
          ? `expected response NOT to be API error ${expectedStatus} ${expectedCode}`
          : `expected ${expectedStatus} ${expectedCode ?? ''} (ok:false), got ${received.statusCode}: ${JSON.stringify(json)}`,
    }
  },
})

declare module 'vitest' {
  interface Assertion<T> {
    toBeApiSuccess(): T
    toBeApiError(statusCode: number, errorCode?: string): T
  }
  interface AsymmetricMatchersContaining {
    toBeApiSuccess(): unknown
    toBeApiError(statusCode: number, errorCode?: string): unknown
  }
}
