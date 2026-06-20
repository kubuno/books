import { useTranslation } from 'react-i18next'
import { Tag, User, Building2, Languages } from 'lucide-react'
import type { Facets, FacetValue } from '../api'

export interface BrowseFilters {
  tag?: string
  author?: string
  publisher?: string
  language?: string
}

interface Props {
  facets: Facets | undefined
  filters: BrowseFilters
  onChange: (next: BrowseFilters) => void
}

function FacetGroup({
  icon,
  title,
  values,
  active,
  onPick,
}: {
  icon: React.ReactNode
  title: string
  values: FacetValue[]
  active?: string
  onPick: (value?: string) => void
}) {
  if (values.length === 0) return null
  return (
    <div className="mb-5">
      <h3 className="mb-1.5 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-text-tertiary">
        {icon}
        {title}
      </h3>
      <ul className="space-y-0.5">
        {values.slice(0, 12).map((v) => {
          const isActive = active === v.value
          return (
            <li key={v.value}>
              <button
                type="button"
                onClick={() => onPick(isActive ? undefined : v.value)}
                className={`flex w-full items-center justify-between gap-2 rounded px-2 py-1 text-left text-sm transition ${
                  isActive
                    ? 'bg-primary-light font-medium text-primary'
                    : 'text-text-secondary hover:bg-surface-1'
                }`}
              >
                <span className="min-w-0 truncate">{v.value}</span>
                <span className="flex-shrink-0 text-xs text-text-tertiary">{v.count}</span>
              </button>
            </li>
          )
        })}
      </ul>
    </div>
  )
}

/** P6 — clickable facet navigator (tags / authors / publishers / languages). */
export default function FilterPanel({ facets, filters, onChange }: Props) {
  const { t } = useTranslation('books')

  return (
    <aside className="w-56 flex-shrink-0">
      <FacetGroup
        icon={<Tag className="h-3.5 w-3.5" />}
        title={t('books_facet_tags')}
        values={facets?.tags ?? []}
        active={filters.tag}
        onPick={(tag) => onChange({ ...filters, tag })}
      />
      <FacetGroup
        icon={<User className="h-3.5 w-3.5" />}
        title={t('books_facet_authors')}
        values={facets?.authors ?? []}
        active={filters.author}
        onPick={(author) => onChange({ ...filters, author })}
      />
      <FacetGroup
        icon={<Building2 className="h-3.5 w-3.5" />}
        title={t('books_facet_publishers')}
        values={facets?.publishers ?? []}
        active={filters.publisher}
        onPick={(publisher) => onChange({ ...filters, publisher })}
      />
      <FacetGroup
        icon={<Languages className="h-3.5 w-3.5" />}
        title={t('books_facet_languages')}
        values={facets?.languages ?? []}
        active={filters.language}
        onPick={(language) => onChange({ ...filters, language })}
      />
    </aside>
  )
}
