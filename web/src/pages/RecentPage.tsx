import { useTranslation } from 'react-i18next'
import { useQuery } from '@tanstack/react-query'
import { useImageCacheStore } from '@kubuno/sdk'
import { Clock } from 'lucide-react'
import { recentBooks, bookCoverUrl } from '../api'
import CoverCard from '../components/CoverCard'
import { useBookContextMenu } from '../components/bookMenu'
import { Breadcrumb } from '../components/Breadcrumb'
import { CardGrid, LoadingState, EmptyState } from '../components/shared'

/** Flat grid of the most recently added books across all libraries. */
export default function RecentPage() {
  const { t } = useTranslation('books')
  const cacheVer = useImageCacheStore((s) => s.global)
  const bookMenu = useBookContextMenu()

  const { data: books, isLoading } = useQuery({
    queryKey: ['books', 'recent-page'],
    queryFn: () => recentBooks(60),
  })

  return (
    <div className="w-full p-6" data-module="books">
      <div className="mb-6">
        <Breadcrumb crumbs={[{ label: t('recent_added', { defaultValue: 'Ajoutés récemment' }) }]} />
        <p className="mt-1 text-sm text-text-secondary">{t('books_recent_subtitle')}</p>
      </div>

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
        <EmptyState icon={<Clock className="h-10 w-10" />} message={t('books_empty_books')} />
      )}
    </div>
  )
}
