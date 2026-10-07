import { useParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ConfirmDialog } from '@ui'
import { useConfirm, useImageCacheStore } from '@kubuno/sdk'
import { FolderPlus } from 'lucide-react'
import { getCollection, removeSeriesFromCollection, seriesCoverUrl } from '../api'
import CoverCard from '../components/CoverCard'
import { useSeriesContextMenu } from '../components/seriesMenu'
import { Breadcrumb } from '../components/Breadcrumb'
import { CardGrid, LoadingState, EmptyState } from '../components/shared'

/** P6 — a single collection: grid of its series, each removable. */
export default function CollectionPage() {
  const { id = '' } = useParams<{ id: string }>()
  const { t } = useTranslation('books')
  const qc = useQueryClient()
  const cacheVer = useImageCacheStore((s) => s.global)
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const seriesMenu = useSeriesContextMenu()

  const { data, isLoading } = useQuery({
    queryKey: ['books', 'collection', id],
    queryFn: () => getCollection(id),
    enabled: !!id,
  })

  async function remove(seriesId: string, name: string) {
    const ok = await confirm({
      title: t('books_collection_remove_title'),
      message: t('books_collection_remove_message', { name }),
      variant: 'warning',
      confirmLabel: t('common_remove'),
    })
    if (!ok) return
    await removeSeriesFromCollection(id, seriesId)
    void qc.invalidateQueries({ queryKey: ['books', 'collection', id] })
    void qc.invalidateQueries({ queryKey: ['books', 'collections'] })
  }

  const collection = data?.collection
  const series = data?.series ?? []

  return (
    <div className="w-full p-6" data-module="books">
      <div className="mb-6">
        <Breadcrumb
          crumbs={[
            { label: t('books_nav_collections'), to: '/books/collections' },
            { label: collection?.name ?? t('books_nav_collections') },
          ]}
        />
        {collection?.description && (
          <p className="mt-1 text-sm text-text-secondary">{collection.description}</p>
        )}
      </div>

      {isLoading ? (
        <LoadingState label={t('books_loading')} />
      ) : series.length > 0 ? (
        <CardGrid>
          {series.map((s) => (
            <div key={s.id} className="group relative">
              <CoverCard
                to={`/books/series/${s.id}`}
                title={s.name}
                subtitle={t('books_book_count', { count: s.book_count })}
                count={s.book_count}
                coverUrl={`${seriesCoverUrl(s.id)}?v=${cacheVer}`}
                contextMenu={seriesMenu({ id: s.id, name: s.name })}
              />
              <button
                type="button"
                onClick={() => void remove(s.id, s.name)}
                className="absolute right-1.5 top-1.5 z-10 rounded-full bg-black/55 p-1 text-white opacity-0 transition hover:bg-danger group-hover:opacity-100"
                title={t('common_remove')}
              >
                <FolderPlus className="h-3.5 w-3.5 rotate-45" />
              </button>
            </div>
          ))}
        </CardGrid>
      ) : (
        <EmptyState
          icon={<FolderPlus className="h-10 w-10" />}
          message={t('books_empty_collection_series')}
        />
      )}

      {confirmState && (
        <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
      )}
    </div>
  )
}
