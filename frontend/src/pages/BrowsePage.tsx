import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useQuery } from '@tanstack/react-query'
import { Button, Dropdown } from '@ui'
import { useImageCacheStore } from '@kubuno/sdk'
import { BookOpen, X } from 'lucide-react'
import { listBooks, getFacets, bookCoverUrl } from '../api'
import CoverCard from '../components/CoverCard'
import { useBookContextMenu } from '../components/bookMenu'
import FilterPanel, { type BrowseFilters } from '../components/FilterPanel'
import { Breadcrumb } from '../components/Breadcrumb'
import { CardGrid, LoadingState, EmptyState, PageHeader } from '../components/shared'

const FORMATS = ['', 'cbz', 'cbr', 'cb7', 'pdf', 'epub', 'mobi', 'azw3']
const SORTS = ['added_desc', 'added_asc', 'title_asc', 'title_desc']

/** P6 — browse all books with faceted filters, a format filter and sort. */
export default function BrowsePage() {
  const { t } = useTranslation('books')
  const cacheVer = useImageCacheStore((s) => s.global)
  const bookMenu = useBookContextMenu()
  const [filters, setFilters] = useState<BrowseFilters>({})
  const [format, setFormat] = useState('')
  const [sort, setSort] = useState('added_desc')

  const { data: facets } = useQuery({
    queryKey: ['books', 'facets'],
    queryFn: getFacets,
  })

  const params = {
    ...filters,
    ...(format ? { format } : {}),
    sort,
    limit: 100,
  }

  const { data: books, isLoading } = useQuery({
    queryKey: ['books', 'browse', params],
    queryFn: () => listBooks(params),
  })

  const formatOptions = FORMATS.map((f) => ({
    value: f,
    label: f ? f.toUpperCase() : t('books_filter_all_formats'),
  }))
  const sortOptions = SORTS.map((s) => ({ value: s, label: t(`books_sort_${s}`) }))

  const activeChips = (
    [
      ['tag', filters.tag],
      ['author', filters.author],
      ['publisher', filters.publisher],
      ['language', filters.language],
    ] as const
  ).filter(([, v]) => !!v)

  return (
    <div className="w-full p-6" data-module="books">
      <div className="mb-4">
        <Breadcrumb crumbs={[{ label: t('books_nav_browse') }]} />
      </div>
      <PageHeader
        title={t('books_nav_browse')}
        subtitle={t('books_browse_subtitle')}
        actions={
          <div className="flex items-center gap-2">
            <Dropdown value={format} onChange={setFormat} options={formatOptions} height={34} />
            <Dropdown value={sort} onChange={setSort} options={sortOptions} height={34} />
          </div>
        }
      />

      {activeChips.length > 0 && (
        <div className="mb-4 flex flex-wrap items-center gap-1.5">
          {activeChips.map(([key, value]) => (
            <button
              key={key}
              type="button"
              onClick={() => setFilters({ ...filters, [key]: undefined })}
              className="inline-flex items-center gap-1 rounded-full bg-primary-light px-2.5 py-1 text-xs font-medium text-primary hover:bg-primary/20"
            >
              {value}
              <X className="h-3 w-3" />
            </button>
          ))}
          <Button variant="ghost" onClick={() => setFilters({})}>
            {t('books_filter_clear')}
          </Button>
        </div>
      )}

      <div className="flex gap-6">
        <FilterPanel facets={facets} filters={filters} onChange={setFilters} />
        <div className="min-w-0 flex-1">
          {isLoading ? (
            <LoadingState label={t('books_loading')} />
          ) : books && books.length > 0 ? (
            <CardGrid>
              {books.map((b) => (
                <CoverCard
                  key={b.id}
                  to={`/books/book/${b.id}`}
                  title={b.title}
                  formats={b.formats}
                  coverUrl={`${bookCoverUrl(b.id)}?v=${cacheVer}`}
                  contextMenu={bookMenu({ id: b.id, title: b.title })}
                />
              ))}
            </CardGrid>
          ) : (
            <EmptyState icon={<BookOpen className="h-10 w-10" />} message={t('books_empty_books')} />
          )}
        </div>
      </div>
    </div>
  )
}
