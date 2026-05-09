import './i18n' // initialize i18next before rendering
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { I18nProvider } from './providers/I18nProvider'
import { ThemeProvider } from './providers/ThemeProvider'
import { AuthProvider } from './providers/AuthProvider'
import { WebSocketProvider } from './providers/WebSocketProvider'
import App from './App'
import './styles/globals.css'
import './styles/tokens/primitives.css'
import './styles/tokens/semantic.css'
import './styles/tokens/light.css'
import './styles/auth.css'
import './styles/legal.css'

const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
const wsUrl = `${wsProtocol}//${window.location.host}/ws`

const root = document.getElementById('root')
if (!root) throw new Error('Root element not found')

createRoot(root).render(
  <StrictMode>
    <I18nProvider>
      <ThemeProvider>
        <AuthProvider>
          <WebSocketProvider url={wsUrl}>
            <App />
          </WebSocketProvider>
        </AuthProvider>
      </ThemeProvider>
    </I18nProvider>
  </StrictMode>
)
