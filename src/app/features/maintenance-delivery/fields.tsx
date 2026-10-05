import { useId, type ReactNode } from 'react'
import { uiStyles } from '@app/shared/lib/uiStyles'

export function TextField({
  label,
  value,
  onChange,
  max = 4000,
  required = true,
  children,
}: {
  label: string
  value: string
  onChange: (value: string) => void
  max?: number
  required?: boolean
  children?: ReactNode
}) {
  const id = useId()
  return (
    <div className="block">
      <label htmlFor={id} className={uiStyles.label}>
        {label}
      </label>
      <textarea
        id={id}
        aria-describedby={children ? `${id}-help` : undefined}
        className={`${uiStyles.input} h-auto min-h-16 py-2`}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        required={required}
        maxLength={max}
        rows={2}
      />
      {children && (
        <p id={`${id}-help`} className={uiStyles.sectionDescription}>
          {children}
        </p>
      )}
    </div>
  )
}
export function Timestamp({ value }: { value: string }) {
  return <time dateTime={value}>{new Date(value).toLocaleString()}</time>
}
export function Feedback({ message }: { message: string }) {
  return message ? (
    <p role="alert" aria-live="polite" className={uiStyles.error}>
      {message}
    </p>
  ) : null
}
