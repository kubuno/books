import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery } from '@tanstack/react-query'
import { Button } from '@ui'
import { useAuthStore, useImageCacheStore } from '@kubuno/sdk'
import { Layers, BookOpen, CheckSquare, Pencil, X } from 'lucide-react'
import { listLibraries, listSeries, listBooks, seriesCoverUrl, bookCoverUrl } from '../api'
import CoverCard from '../components/CoverCard'
import { useBookContextMenu } from '../components/bookMenu'
import { useSeriesContextMenu } from '../components/seriesMenu'
import BulkEditDialog from '../components/BulkEditDialog'
import { Breadcrumb } from '../components/Breadcrumb'
import { CardGrid, LoadingState, EmptyState } from '../components/shared'

/** A single library: its series grid + loose (series-less) books. */
export default function LibraryPage() {
  const { id = '' } = useParams<{ id: string }>()
  const { t } = useTranslation('books')
  const cacheVer = useImageCacheStore((s) => s.global)
  const user = useAuthStore((s) => s.user)
  const isAdmin = user?.role === 'admin'
  const bookMenu = useBookContextMenu()
  const seriesMenu = useSeriesContextMenu()

  // Selection mode for bulk-editing loose books (admin only).
  const [selecting, setSelecting] = useState(false)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [bulkOpen, setBulkOpen] = useState(false)

  function toggle(bookId: string) {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(bookId)) next.delete(bookId)
      else next.add(bookId)
      return next
    })
  }

  function exitSelection() {
    setSelecting(false)
    setSelected(new Set())
  }

  const { data: libraries } = useQuery({
    queryKey: ['books', 'libraries'],
    queryFn: listLibraries,
  })
  const library = libraries?.find((l) => l.id === id)

  const { data: series, isLoading: seriesLoading } = useQuery({
    queryKey: ['books', 'series', id],
    queryFn: () => listSeries(id),
    enabled: !!id,
  })

  // Loose books: anything in this library without a series.
  const { data: books } = useQuery({
    queryKey: ['books', 'library-books', id],
    queryFn: () => listBooks({ library_id: id, limit: 200 }),
    enabled: !!id,
  })
  const looseBooks = books?.filter((b) => !b.series_id) ?? []

  const isEmpty =
    !seriesLoading && (series?.length ?? 0) === 0 && looseBooks.length === 0

  return (
    <div className="w-full p-6" data-module="books">
      <div className="mb-6 flex items-start justify-between gap-4">
        <div className="min-w-0">
          <Breadcrumb
            crumbs={[
              { label: t('libraries', { defaultValue: 'Bibliothèques' }), to: '/books' },
              { label: library?.name ?? t('books_library') },
            ]}
          />
          {library && (
            <p className="mt-1 text-sm text-text-secondary">
              {t('books_item_count', { count: library.item_count })}
            </p>
          )}
        </div>
        {isAdmin && looseBooks.length > 0 && (
          <div className="flex flex-shrink-0 items-center gap-2">
            {selecting ? (
              <>
                <Button
                  variant="primary"
                  icon={<Pencil className="h-4 w-4" />}
                  onClick={() => setBulkOpen(true)}
                  disabled={selected.size === 0}
                >
                  {t('books_bulk_edit_n', { count: selected.size })}
                </Button>
                <Button variant="ghost" icon={<X className="h-4 w-4" />} onClick={exitSelection}>
                  {t('common_cancel')}
                </Button>
              </>
            ) : (
              <Button
                variant="secondary"
                icon={<CheckSquare className="h-4 w-4" />}
                onClick={() => setSelecting(true)}
              >
                {t('books_select')}
              </Button>
            )}
          </div>
        )}
      </div>

      {seriesLoading ? (
        <LoadingState label={t('books_loading')} />
      ) : isEmpty ? (
        <EmptyState icon={<Layers className="h-10 w-10" />} message={t('books_empty_series')} />
      ) : (
        <>
          {series && series.length > 0 && (
            <section>
              <h2 className="mb-3 flex items-center gap-2 text-lg font-semibold text-text-primary">
                <Layers className="h-5 w-5 text-text-secondary" />
                {t('books_section_series')}
              </h2>
              <CardGrid>
                {series.map((s) => (
                  <CoverCard
                    key={s.id}
                    to={`/books/series/${s.id}`}
                    title={s.name}
                    subtitle={t('books_book_count', { count: s.book_count })}
                    count={s.book_count}
                    coverUrl={`${seriesCoverUrl(s.id)}?v=${cacheVer}`}
                    contextMenu={seriesMenu({ id: s.id, name: s.name })}
                  />
                ))}
              </CardGrid>
            </section>
          )}

          {looseBooks.length > 0 && (
            <section className="mt-10">
              <h2 className="mb-3 flex items-center gap-2 text-lg font-semibold text-text-primary">
                <BookOpen className="h-5 w-5 text-text-secondary" />
                {t('books_section_books')}
              </h2>
              <CardGrid>
                {looseBooks.map((b) =>
                  selecting ? (
                    <button
                      key={b.id}
                      type="button"
                      onClick={() => toggle(b.id)}
                      className={`relative flex flex-col overflow-hidden rounded-lg border text-left transition ${
                        selected.has(b.id)
                          ? 'border-primary ring-2 ring-primary/40'
                          : 'border-border hover:border-border-strong'
                      }`}
                    >
                      <span
                        className={`absolute right-1.5 top-1.5 z-10 flex h-5 w-5 items-center justify-center rounded border ${
                          selected.has(b.id)
                            ? 'border-primary bg-primary text-white'
                            : 'border-white bg-black/30 text-transparent'
                        }`}
                      >
                        <CheckSquare className="h-3.5 w-3.5" />
                      </span>
                      <span
                        className="flex aspect-[2/3] w-full items-center justify-center bg-surface-2"
                        style={{ backgroundImage: `url(${bookCoverUrl(b.id)}?v=${cacheVer})`, backgroundSize: 'cover', backgroundPosition: 'center' }}
                      />
                      <span className="truncate px-2.5 py-2 text-sm font-medium text-text-primary">
                        {b.title}
                      </span>
                    </button>
                  ) : (
                    <CoverCard
                      key={b.id}
                      to={`/books/book/${b.id}`}
                      title={b.title}
                      formats={b.formats}
                      coverUrl={`${bookCoverUrl(b.id)}?v=${cacheVer}`}
                      contextMenu={bookMenu({ id: b.id, title: b.title })}
                    />
                  ),
                )}
              </CardGrid>
            </section>
          )}
        </>
      )}

      {bulkOpen && (
        <BulkEditDialog
          ids={[...selected]}
          onClose={() => setBulkOpen(false)}
          onDone={exitSelection}
        />
      )}
    </div>
  )
}
