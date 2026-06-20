import type { ReactElement } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { ChevronRight } from 'lucide-react'

/** A single breadcrumb segment. When `to` is omitted, it is the current (non-clickable) item. */
export interface Crumb {
  label: string
  to?: string
}

/**
 * Breadcrumb trail, styled to exactly match the Drive module's breadcrumb.
 * The first crumb renders as the clickable root; the last (no `to`) is the
 * current segment and is not clickable.
 */
export function Breadcrumb({ crumbs }: { crumbs: Crumb[] }): ReactElement {
  const { t } = useTranslation('books')

  return (
    <nav
      className="flex items-center gap-0.5 flex-wrap"
      aria-label={t('breadcrumb', { defaultValue: "Fil d'Ariane" })}
    >
      {crumbs.map((crumb, i) => {
        const content =
          crumb.to !== undefined ? (
            <Link
              to={crumb.to}
              className="text-xl font-medium text-text-secondary hover:text-primary transition-colors leading-tight"
            >
              {crumb.label}
            </Link>
          ) : (
            <span className="text-xl font-medium text-text-primary leading-tight">
              {crumb.label}
            </span>
          )

        // The first crumb has no leading chevron; every subsequent one does.
        if (i === 0) return <span key={i}>{content}</span>

        return (
          <span key={i} className="flex items-center gap-0.5">
            <ChevronRight size={16} className="text-text-tertiary flex-shrink-0" />
            {content}
          </span>
        )
      })}
    </nav>
  )
}

export default Breadcrumb
