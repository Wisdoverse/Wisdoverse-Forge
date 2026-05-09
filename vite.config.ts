import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { resolve } from 'path'
import { readFileSync } from 'fs'
import { DEFAULTS } from './shared/defaults'

const clientPort = parseInt(process.env.AGENTFORGE_CLIENT_PORT ?? String(DEFAULTS.CLIENT_PORT), 10)
const serverPort = parseInt(process.env.AGENTFORGE_PORT ?? String(DEFAULTS.SERVER_PORT), 10)
const pkgVersion = (JSON.parse(readFileSync(resolve(__dirname, 'package.json'), 'utf8')) as {
  version: string
}).version

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
    proxy: {
      '/ws': {
        target: `ws://localhost:${serverPort}`,
        ws: true,
      },
      '/api': {
        target: `http://localhost:${serverPort}`,
      },
    },
  },
  build: {
    target: 'esnext',
    sourcemap: process.env.VITE_SOURCEMAP === 'true',
    rollupOptions: {
      output: {
        manualChunks: {
          'vendor-react': ['react', 'react-dom'],
          'vendor-three': ['three'],
          'vendor-xterm': ['@xterm/xterm', '@xterm/addon-fit'],
        },
      },
    },
  },
})
