import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, ConfirmDialog } from '@ui'
import { useConfirm } from '@kubuno/sdk'
import { FolderPlus, Plus, Trash2 } from 'lucide-react'
import { listCollections, createCollection, deleteCollection } from '../api'
import CreateNamedDialog from '../components/CreateNamedDialog'
import { Breadcrumb } from '../components/Breadcrumb'
import { LoadingState, EmptyState, PageHeader } from '../components/shared'

/** P6 — grid of collections (groups of series). */
export default function CollectionsPage() {
  const { t } = useTranslation('books')
  const navigate = useNavigate()
  const qc = useQueryClient()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const [creating, setCreating] = useState(false)

  const { data: collections, isLoading } = useQuery({
    queryKey: ['books', 'collections'],
    queryFn: listCollections,
  })

  async function remove(id: string, name: string) {
    const ok = await confirm({
      title: t('books_collection_delete_title'),
      message: t('books_collection_delete_message', { name }),
      variant: 'danger',
      confirmLabel: t('common_delete'),
    })
    if (!ok) return
    await deleteCollection(id)
    void qc.invalidateQueries({ queryKey: ['books', 'collections'] })
  }

  return (
    <div className="w-full p-6" data-module="books">
      <div className="mb-4">
        <Breadcrumb crumbs={[{ label: t('books_nav_collections') }]} />
      </div>
      <PageHeader
        title={t('books_nav_collections')}
        subtitle={t('books_collections_subtitle')}
        actions={
          <Button variant="primary" icon={<Plus className="h-4 w-4" />} onClick={() => setCreating(true)}>
            {t('books_collection_new')}
          </Button>
        }
      />

      {isLoading ? (
        <LoadingState label={t('books_loading')} />
      ) : collections && collections.length > 0 ? (
        <div className="grid gap-3 grid-cols-[repeat(auto-fill,minmax(280px,1fr))]">
          {collections.map((c) => (
            <div
              key={c.id}
              className="group flex items-start gap-3 rounded-lg border border-border bg-surface-0 p-4 transition hover:border-border-strong hover:shadow-sm"
            >
              <button
                type="button"
                onClick={() => navigate(`/books/collection/${c.id}`)}
                className="flex min-w-0 flex-1 items-start gap-3 text-left"
              >
                <span className="mt-0.5 flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-md bg-primary-light text-primary">
                  <FolderPlus className="h-5 w-5" />
                </span>
                <span className="min-w-0">
                  <span className="block truncate font-medium text-text-primary">{c.name}</span>
                  <span className="block text-xs text-text-tertiary">
                    {t('books_series_count', { count: c.series_count })}
                  </span>
                </span>
              </button>
              <button
                type="button"
                onClick={() => void remove(c.id, c.name)}
                className="flex-shrink-0 text-text-tertiary opacity-0 transition hover:text-danger group-hover:opacity-100"
                title={t('common_delete')}
              >
                <Trash2 className="h-4 w-4" />
              </button>
            </div>
          ))}
        </div>
      ) : (
        <EmptyState
          icon={<FolderPlus className="h-10 w-10" />}
          message={t('books_empty_collections')}
        />
      )}

      {creating && (
        <CreateNamedDialog
          title={t('books_collection_new')}
          onClose={() => setCreating(false)}
          onCreate={async (dto) => {
            await createCollection(dto)
            void qc.invalidateQueries({ queryKey: ['books', 'collections'] })
          }}
        />
      )}
      {confirmState && (
        <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
      )}
    </div>
  )
}
