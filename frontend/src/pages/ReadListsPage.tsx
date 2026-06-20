import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, ConfirmDialog } from '@ui'
import { useConfirm } from '@kubuno/sdk'
import { ListChecks, Plus, Trash2 } from 'lucide-react'
import { listReadLists, createReadList, deleteReadList } from '../api'
import CreateNamedDialog from '../components/CreateNamedDialog'
import { Breadcrumb } from '../components/Breadcrumb'
import { LoadingState, EmptyState, PageHeader } from '../components/shared'

/** P6 — grid of reading lists (ordered sets of books). */
export default function ReadListsPage() {
  const { t } = useTranslation('books')
  const navigate = useNavigate()
  const qc = useQueryClient()
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const [creating, setCreating] = useState(false)

  const { data: lists, isLoading } = useQuery({
    queryKey: ['books', 'readlists'],
    queryFn: listReadLists,
  })

  async function remove(id: string, name: string) {
    const ok = await confirm({
      title: t('books_readlist_delete_title'),
      message: t('books_readlist_delete_message', { name }),
      variant: 'danger',
      confirmLabel: t('common_delete'),
    })
    if (!ok) return
    await deleteReadList(id)
    void qc.invalidateQueries({ queryKey: ['books', 'readlists'] })
  }

  return (
    <div className="w-full p-6" data-module="books">
      <div className="mb-4">
        <Breadcrumb crumbs={[{ label: t('books_nav_readlists') }]} />
      </div>
      <PageHeader
        title={t('books_nav_readlists')}
        subtitle={t('books_readlists_subtitle')}
        actions={
          <Button variant="primary" icon={<Plus className="h-4 w-4" />} onClick={() => setCreating(true)}>
            {t('books_readlist_new')}
          </Button>
        }
      />

      {isLoading ? (
        <LoadingState label={t('books_loading')} />
      ) : lists && lists.length > 0 ? (
        <div className="grid gap-3 grid-cols-[repeat(auto-fill,minmax(280px,1fr))]">
          {lists.map((l) => (
            <div
              key={l.id}
              className="group flex items-start gap-3 rounded-lg border border-border bg-surface-0 p-4 transition hover:border-border-strong hover:shadow-sm"
            >
              <button
                type="button"
                onClick={() => navigate(`/books/readlist/${l.id}`)}
                className="flex min-w-0 flex-1 items-start gap-3 text-left"
              >
                <span className="mt-0.5 flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-md bg-primary-light text-primary">
                  <ListChecks className="h-5 w-5" />
                </span>
                <span className="min-w-0">
                  <span className="block truncate font-medium text-text-primary">{l.name}</span>
                  <span className="block text-xs text-text-tertiary">
                    {t('books_book_count', { count: l.book_count })}
                  </span>
                </span>
              </button>
              <button
                type="button"
                onClick={() => void remove(l.id, l.name)}
                className="flex-shrink-0 text-text-tertiary opacity-0 transition hover:text-danger group-hover:opacity-100"
                title={t('common_delete')}
              >
                <Trash2 className="h-4 w-4" />
              </button>
            </div>
          ))}
        </div>
      ) : (
        <EmptyState icon={<ListChecks className="h-10 w-10" />} message={t('books_empty_readlists')} />
      )}

      {creating && (
        <CreateNamedDialog
          title={t('books_readlist_new')}
          onClose={() => setCreating(false)}
          onCreate={async (dto) => {
            await createReadList(dto)
            void qc.invalidateQueries({ queryKey: ['books', 'readlists'] })
          }}
        />
      )}
      {confirmState && (
        <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
      )}
    </div>
  )
}
