import { useParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ConfirmDialog } from '@ui'
import { useConfirm, useImageCacheStore } from '@kubuno/sdk'
import { ListChecks, X } from 'lucide-react'
import { getReadList, removeBookFromReadList, bookCoverUrl } from '../api'
import CoverCard from '../components/CoverCard'
import { Breadcrumb } from '../components/Breadcrumb'
import { CardGrid, LoadingState, EmptyState } from '../components/shared'

/** P6 — a single reading list: its books in order, each removable. */
export default function ReadListPage() {
  const { id = '' } = useParams<{ id: string }>()
  const { t } = useTranslation('books')
  const qc = useQueryClient()
  const cacheVer = useImageCacheStore((s) => s.global)
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()

  const { data, isLoading } = useQuery({
    queryKey: ['books', 'readlist', id],
    queryFn: () => getReadList(id),
    enabled: !!id,
  })

  async function remove(bookId: string, title: string) {
    const ok = await confirm({
      title: t('books_readlist_remove_title'),
      message: t('books_readlist_remove_message', { name: title }),
      variant: 'warning',
      confirmLabel: t('common_remove'),
    })
    if (!ok) return
    await removeBookFromReadList(id, bookId)
    void qc.invalidateQueries({ queryKey: ['books', 'readlist', id] })
    void qc.invalidateQueries({ queryKey: ['books', 'readlists'] })
  }

  const readList = data?.read_list
  const books = data?.books ?? []

  return (
    <div className="w-full p-6" data-module="books">
      <div className="mb-6">
        <Breadcrumb
          crumbs={[
            { label: t('books_nav_readlists'), to: '/books/readlists' },
            { label: readList?.name ?? t('books_nav_readlists') },
          ]}
        />
        {readList?.description && (
          <p className="mt-1 text-sm text-text-secondary">{readList.description}</p>
        )}
      </div>

      {isLoading ? (
        <LoadingState label={t('books_loading')} />
      ) : books.length > 0 ? (
        <CardGrid>
          {books.map((b) => (
            <div key={b.id} className="group relative">
              <CoverCard
                to={`/books/book/${b.id}`}
                title={b.title}
                formats={b.formats}
                coverUrl={`${bookCoverUrl(b.id)}?v=${cacheVer}`}
              />
              <button
                type="button"
                onClick={() => void remove(b.id, b.title)}
                className="absolute right-1.5 top-1.5 z-10 rounded-full bg-black/55 p-1 text-white opacity-0 transition hover:bg-danger group-hover:opacity-100"
                title={t('common_remove')}
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </div>
          ))}
        </CardGrid>
      ) : (
        <EmptyState
          icon={<ListChecks className="h-10 w-10" />}
          message={t('books_empty_readlist_books')}
        />
      )}

      {confirmState && (
        <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
      )}
    </div>
  )
}
