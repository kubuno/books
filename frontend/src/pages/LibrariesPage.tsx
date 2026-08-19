import { useTranslation } from 'react-i18next'
import { useQuery } from '@tanstack/react-query'
import { useImageCacheStore } from '@kubuno/sdk'
import { Library as LibraryIcon, Clock, BookOpen } from 'lucide-react'
import { listLibraries, recentBooks, keepReading, bookCoverUrl } from '../api'
import LibraryCard from './LibraryCard'
import CoverCard from '../components/CoverCard'
import { Breadcrumb } from '../components/Breadcrumb'
import { CardGrid, LoadingState, EmptyState, PageHeader } from '../components/shared'

/**
 * Home page: read-only library grid + a "recently added" rail. Library
 * management (create / edit / scan / delete) lives in the admin console
 * (Modules ▸ Books), not here — this page only browses.
 */
export default function LibrariesPage() {
  const { t } = useTranslation('books')
  const cacheVer = useImageCacheStore((s) => s.global)

  const { data: libraries, isLoading } = useQuery({
    queryKey: ['books', 'libraries'],
    queryFn: listLibraries,
  })

  const { data: recent } = useQuery({
    queryKey: ['books', 'recent'],
    queryFn: () => recentBooks(12),
  })

  const { data: keep } = useQuery({
    queryKey: ['books', 'keep-reading'],
    queryFn: () => keepReading(12),
  })

  return (
    <div className="w-full p-6" data-module="books">
      <div className="mb-4">
        <Breadcrumb crumbs={[{ label: t('libraries', { defaultValue: 'Bibliothèques' }) }]} />
      </div>
      <PageHeader title={t('books_title')} subtitle={t('books_subtitle')} />

      {isLoading ? (
        <LoadingState label={t('books_loading')} />
      ) : libraries && libraries.length > 0 ? (
        <div className="grid gap-3 grid-cols-[repeat(auto-fill,minmax(280px,1fr))]">
          {libraries.map((lib) => (
            <LibraryCard key={lib.id} library={lib} />
          ))}
        </div>
      ) : (
        <EmptyState
          icon={<LibraryIcon className="h-10 w-10" />}
          message={t('books_empty_libraries')}
        />
      )}

      {keep && keep.length > 0 && (
        <section className="mt-10">
          <h2 className="mb-3 flex items-center gap-2 text-lg font-semibold text-text-primary">
            <BookOpen className="h-5 w-5 text-text-secondary" />
            {t('books_keep_reading')}
          </h2>
          <CardGrid>
            {keep.map((b) => (
              <CoverCard
                key={b.id}
                to={`/books/read/${b.id}`}
                title={b.title}
                formats={b.formats}
                coverUrl={`${bookCoverUrl(b.id)}?v=${cacheVer}`}
                progress={
                  b.page_count && b.page_count > 1
                    ? b.progress_page / (b.page_count - 1)
                    : null
                }
              />
            ))}
          </CardGrid>
        </section>
      )}

      {recent && recent.length > 0 && (
        <section className="mt-10">
          <h2 className="mb-3 flex items-center gap-2 text-lg font-semibold text-text-primary">
            <Clock className="h-5 w-5 text-text-secondary" />
            {t('books_recent_added')}
          </h2>
          <CardGrid>
            {recent.map((b) => (
              <CoverCard
                key={b.id}
                to={`/books/book/${b.id}`}
                title={b.title}
                formats={b.formats}
                coverUrl={`${bookCoverUrl(b.id)}?v=${cacheVer}`}
              />
            ))}
          </CardGrid>
        </section>
      )}

    </div>
  )
}
