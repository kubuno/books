import type { ReactNode } from 'react'
import { Spinner } from '@ui'

/** Responsive grid wrapper for cover cards. */
export function CardGrid({ children }: { children: ReactNode }) {
  return (
    <div className="grid gap-4 grid-cols-[repeat(auto-fill,minmax(140px,1fr))]">{children}</div>
  )
}

/** Centered loading state. */
export function LoadingState({ label }: { label?: string }) {
  return (
    <div className="flex items-center justify-center py-20">
      <Spinner size="md" label={label} />
    </div>
  )
}

/** Empty placeholder with icon + message and optional action. */
export function EmptyState({
  icon,
  message,
  action,
}: {
  icon: ReactNode
  message: string
  action?: ReactNode
}) {
  return (
    <div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-border py-16 text-center">
      <div className="mb-3 text-text-tertiary">{icon}</div>
      <p className="text-sm text-text-secondary">{message}</p>
      {action && <div className="mt-4">{action}</div>}
    </div>
  )
}

/** Page-level header (title + optional subtitle + right-aligned actions). */
export function PageHeader({
  title,
  subtitle,
  actions,
}: {
  title: ReactNode
  subtitle?: ReactNode
  actions?: ReactNode
}) {
  return (
    <header className="mb-6 flex items-start justify-between gap-4">
      <div>
        <h1 className="text-2xl font-semibold text-text-primary">{title}</h1>
        {subtitle && <p className="mt-1 text-sm text-text-secondary">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-shrink-0 items-center gap-2">{actions}</div>}
    </header>
  )
}
