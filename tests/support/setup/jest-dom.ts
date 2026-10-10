import '@testing-library/jest-dom/vitest'

// Polyfill localStorage for jsdom threads (vitest pool: 'threads' may not provide working localStorage)
if (
  typeof globalThis.localStorage === 'undefined' ||
  typeof globalThis.localStorage?.getItem !== 'function'
) {
  const store: Record<string, string> = {}
  globalThis.localStorage = {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => {
      store[key] = String(value)
    },
    removeItem: (key: string) => {
      delete store[key]
    },
    clear: () => {
      for (const key of Object.keys(store)) delete store[key]
    },
    get length() {
      return Object.keys(store).length
    },
    key: (index: number) => Object.keys(store)[index] ?? null,
  } as Storage
}

// Polyfill ResizeObserver for jsdom (used by cmdk and other UI libraries)
if (typeof globalThis.ResizeObserver === 'undefined') {
  globalThis.ResizeObserver = class ResizeObserver {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
}

// Polyfill scrollIntoView for jsdom (used by cmdk for keyboard navigation)
if (typeof Element.prototype.scrollIntoView === 'undefined') {
  Element.prototype.scrollIntoView = function () {}
}

// jsdom cannot scroll. Use browser tests to verify viewport movement.
window.scrollTo = function () {}

// jsdom has no modal-dialog API. Browser tests cover native focus and inertness.
if (typeof HTMLDialogElement.prototype.showModal !== 'function') {
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute('open', '')
  }
  HTMLDialogElement.prototype.close = function () {
    if (!this.open) return
    this.removeAttribute('open')
    this.dispatchEvent(new Event('close'))
  }
}
