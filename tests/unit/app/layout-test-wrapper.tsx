import type { ComponentProps, ReactNode } from 'react'
import { ThemeProvider } from '@app/providers/ThemeProvider'
import { AppLayout } from '@app/layouts/AppLayout'

type MemoryRouterProps = Partial<
  Pick<ComponentProps<typeof AppLayout>, 'activePath' | 'onNavigate'>
> & {
  children?: ReactNode
}

export function MemoryRouter({
  children = <div>Test Content</div>,
  ...layoutProps
}: MemoryRouterProps = {}) {
  return (
    <ThemeProvider>
      <AppLayout {...layoutProps}>{children}</AppLayout>
    </ThemeProvider>
  )
}
