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
        data-testid="org-switcher"
        onClick={() => setOpen(!open)}
        className={cn(
          'w-full flex items-center gap-2 px-2 py-1.5 rounded-lg text-ui-body',
          'hover:bg-black/[0.04] dark:hover:bg-white/[0.06] transition-colors'
        )}
      >
        <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-md bg-apple-blue text-ui-caption font-semibold text-white">
          {avatar || '?'}
        </span>
        <span className="font-medium truncate flex-1 text-left">
          {selected?.name ?? 'Select org'}
        </span>
        <ChevronsUpDown
          size={14}
          strokeWidth={2}
          className="text-secondary-light dark:text-secondary-dark shrink-0"
        />
      </button>

      {open && (
        <div
          data-testid="org-dropdown"
          className={cn(
            'absolute left-3 right-3 top-full mt-1 z-50',
            'bg-surface dark:bg-surface-dark backdrop-blur-xl rounded-lg',
            'border border-black/[0.08] dark:border-white/[0.1]',
            'py-1'
          )}
        >
          {orgs.map((org) => (
            <button
              key={org.id}
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
                {org.id === selectedOrgId && <Check size={12} strokeWidth={2.5} />}
              </span>
              <span className="truncate">{org.name}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
