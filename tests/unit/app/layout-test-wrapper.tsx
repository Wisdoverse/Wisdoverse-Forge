import { ThemeProvider } from '@app/providers/ThemeProvider'
import { AppLayout } from '@app/layouts/AppLayout'

export function MemoryRouter() {
  return (
    <ThemeProvider>
      <AppLayout>
        <div>Test Content</div>
      </AppLayout>
    </ThemeProvider>
  )
}
