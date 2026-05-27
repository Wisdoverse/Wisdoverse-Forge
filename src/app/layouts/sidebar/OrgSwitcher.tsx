import { useState, useRef, useEffect } from 'react'
import { ChevronsUpDown, Check } from 'lucide-react'
import { cn } from '@app/shared/lib/utils'
import type { NavOrg } from '@app/entities/organization'

interface OrgSwitcherProps {
  orgs: NavOrg[]
  selectedOrgId: string | null
  onSelect: (orgId: string) => void
}

export function OrgSwitcher({ orgs, selectedOrgId, onSelect }: OrgSwitcherProps) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    if (open) document.addEventListener('mousedown', handleClickOutside)
    return () => document.removeEventListener('mousedown', handleClickOutside)
  }, [open])

  const selected = orgs.find((o) => o.id === selectedOrgId)
  const selectedName = selected?.name ?? 'Select organization'

  // Derive a 2-letter avatar from the org name
  const avatar = (selected?.name ?? '?')
    .split(/\s+/)
    .map((w) => w[0])
    .filter(Boolean)
    .slice(0, 2)
    .join('')
    .toUpperCase()

  return (
    <div ref={ref} className="relative px-3 mb-2">
      <button
        type="button"
        data-testid="org-switcher"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={`Organization selector: ${selectedName}`}
        title="Choose organization"
        className={cn(
          'w-full flex items-center gap-2 px-2 py-1.5 rounded-lg text-ui-body',
          'hover:bg-black/[0.04] dark:hover:bg-white/[0.06] transition-colors'
        )}
      >
        <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-apple-blue text-ui-caption font-semibold text-white">
          {avatar || '?'}
        </span>
        <span className="font-medium truncate flex-1 text-left">{selectedName}</span>
        <ChevronsUpDown
          size={14}
          strokeWidth={2}
          className="text-secondary-light dark:text-secondary-dark shrink-0"
        />
      </button>

      {open && (
        <div
          data-testid="org-dropdown"
          role="menu"
          aria-label="Choose organization"
          className={cn(
            'absolute left-3 right-3 top-full mt-1 z-50',
            'bg-surface dark:bg-surface-dark backdrop-blur-xl rounded-lg',
            'border border-black/[0.08] dark:border-white/[0.1]',
            'py-1 shadow-lg'
          )}
        >
          <div className="border-b border-black/[0.06] px-3 pb-2 pt-1.5 dark:border-white/[0.08]">
            <p className="text-ui-body font-medium text-foreground-light dark:text-foreground-dark">
              Organization
            </p>
            <p className="mt-0.5 text-ui-caption text-secondary-light dark:text-secondary-dark">
              Switching changes which teams, projects, and Agents you can see.
            </p>
          </div>
          {orgs.map((org) => (
            <button
              key={org.id}
              type="button"
              role="menuitemradio"
              aria-checked={org.id === selectedOrgId}
              aria-label={`Switch to ${org.name}`}
              title={`Switch to ${org.name}`}
              onClick={() => {
                onSelect(org.id)
                setOpen(false)
              }}
              className={cn(
                'w-full flex items-center gap-2 px-3 py-1.5 text-ui-body text-left',
                'hover:bg-black/[0.04] dark:hover:bg-white/[0.06]',
                org.id === selectedOrgId && 'text-apple-blue font-medium'
              )}
            >
              <span className="w-4 shrink-0 flex items-center justify-center">
                {org.id === selectedOrgId && (
                  <Check size={12} strokeWidth={2.5} aria-hidden="true" />
                )}
              </span>
              <span className="truncate">{org.name}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
