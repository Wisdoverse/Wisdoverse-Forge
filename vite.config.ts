import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { resolve } from 'path'
import { readFileSync } from 'fs'
import { DEFAULTS } from './shared/defaults'

const clientPort = parseInt(process.env.AGENTFORGE_CLIENT_PORT ?? String(DEFAULTS.CLIENT_PORT), 10)
const serverPort = parseInt(process.env.AGENTFORGE_PORT ?? String(DEFAULTS.SERVER_PORT), 10)
const pkgVersion = (
  JSON.parse(readFileSync(resolve(__dirname, 'package.json'), 'utf8')) as {
    version: string
  }
).version

// Shared dev/preview proxy: forward API + WebSocket traffic to the Rust backend.
// Used by both `vite dev` (server) and `vite preview` so a built bundle served
// by preview (e.g. the CI E2E smoke job, or prod-like local serving) reaches the
// backend the same way the dev server does.
const apiProxy = {
  '/ws': {
    target: `ws://localhost:${serverPort}`,
    ws: true,
  },
  '/api': {
    target: `http://localhost:${serverPort}`,
  },
}

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@app': resolve(__dirname, 'src/app'),
      '@shared': resolve(__dirname, 'shared'),
    },
  },
  define: {
    __AGENTFORGE_DEFAULT_PORT__: serverPort,
    __APP_VERSION__: JSON.stringify(pkgVersion),
  },
  server: {
    port: clientPort,
    proxy: apiProxy,
  },
  preview: {
    port: clientPort,
    proxy: apiProxy,
  },
  build: {
    target: 'esnext',
    sourcemap: process.env.VITE_SOURCEMAP === 'true',
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes('/node_modules/')) return undefined
          if (id.includes('/node_modules/react/') || id.includes('/node_modules/react-dom/')) {
            return 'vendor-react'
          }
          if (id.includes('/node_modules/three/')) {
            return 'vendor-three'
          }
          if (id.includes('/node_modules/@xterm/')) {
            return 'vendor-xterm'
          }
          return undefined
        },
      },
    },
  },
})
