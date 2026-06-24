/**
 * Wisdoverse Forge - Central Configuration Defaults
 *
 * Single source of truth for default values.
 * Environment variables override these defaults.
 */

export const DEFAULTS = {
  /** WebSocket/API server port */
  SERVER_PORT: 4003,

  /** Vite dev server port */
  CLIENT_PORT: 4002,

  /**
   * Events file path (used by hook for local backup).
   * Uses ~/.agentforge/ to ensure consistent location regardless of
   * how agentforge was installed (npx, global npm, local dev).
   * The ~ is expanded by the server at runtime.
   */
  EVENTS_FILE: '~/.agentforge/data/events.jsonl',

  /** Max events to keep in memory (reduced for performance) */
  MAX_EVENTS: 2000,

  /** Git status polling interval in milliseconds (default 15s) */
  GIT_POLL_INTERVAL: 15000,

  /** Allowed origins for CORS/WebSocket (comma-separated, supports wildcards) */
  ALLOWED_ORIGINS: 'localhost,127.0.0.1',

  /** Trust proxy headers (X-Forwarded-For, X-Real-IP, etc.) */
  TRUST_PROXY: false,

  /**
   * Uploads directory for image attachments.
   * Images are saved per-session in subdirectories.
   * Stored inside data directory so it's included in existing volume mounts.
   */
  UPLOADS_DIR: '~/.agentforge/data/uploads',
} as const

export type Defaults = typeof DEFAULTS
