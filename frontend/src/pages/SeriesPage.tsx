import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useAuthStore, useImageCacheStore } from '@kubuno/sdk'
import { Button, MenuDropdown, useMenuDropdown, type MenuItem } from '@ui'
import { BookOpen, Pencil, FolderPlus, Plus } from 'lucide-react'
import {
  getSeries,
  getSeriesBooks,
  bookCoverUrl,
  listCollections,
  createCollection,
  addSeriesToCollection,
} from '../api'
import CoverCard from '../components/CoverCard'
import { useBookContextMenu } from '../components/bookMenu'
import SeriesMetadataEditor from '../components/SeriesMetadataEditor'
import { Breadcrumb } from '../components/Breadcrumb'
import { CardGrid, LoadingState, EmptyState } from '../components/shared'

/** A single series: header + its books, ordered by series index. */
export default function SeriesPage() {
  const { id = '' } = useParams<{ id: string }>()
  const { t } = useTranslation('books')
  const cacheVer = useImageCacheStore((s) => s.global)
  const user = useAuthStore((s) => s.user)
  const isAdmin = user?.role === 'admin'
  const qc = useQueryClient()
  const [editing, setEditing] = useState(false)
  const collectionMenu = useMenuDropdown()
  const bookMenu = useBookContextMenu()

  const { data: detail } = useQuery({
    queryKey: ['books', 'series-detail', id],
    queryFn: () => getSeries(id),
    enabled: !!id,
  })
  const series = detail?.series
  const library = detail?.library

  const { data: collections } = useQuery({
    queryKey: ['books', 'collections'],
    queryFn: listCollections,
  })

  async function addToCollection(collectionId: string) {
    await addSeriesToCollection(collectionId, id)
    void qc.invalidateQueries({ queryKey: ['books', 'collections'] })
    void qc.invalidateQueries({ queryKey: ['books', 'collection', collectionId] })
  }

  async function addToNewCollection() {
    const created = await createCollection({ name: series?.name ?? 'Collection' })
    await addToCollection(created.id)
  }

  const collectionItems: MenuItem[] = [
    ...(collections ?? []).map(
      (c): MenuItem => ({
        type: 'action',
        label: c.name,
        icon: <FolderPlus className="h-4 w-4" />,
        onClick: () => void addToCollection(c.id),
      }),
    ),
    ...(collections && collections.length > 0 ? [{ type: 'separator' as const }] : []),
    {
      type: 'action',
      label: t('books_collection_new'),
      icon: <Plus className="h-4 w-4" />,
      onClick: () => void addToNewCollection(),
    },
  ]

  const { data: books, isLoading } = useQuery({
    queryKey: ['books', 'series-books', id],
    queryFn: () => getSeriesBooks(id),
    enabled: !!id,
  })

  const subtitleParts = [
    series?.publisher,
    series ? t('books_book_count', { count: series.book_count }) : null,
  ].filter(Boolean)

  return (
    <div className="w-full p-6" data-module="books">
      <div className="mb-6 flex items-start justify-between gap-4">
        <div className="min-w-0">
          <Breadcrumb
            crumbs={[
              { label: t('libraries', { defaultValue: 'Bibliothèques' }), to: '/books' },
              ...(library
                ? [{ label: library.name, to: `/books/library/${library.id}` }]
                : []),
              { label: series?.name ?? t('books_series') },
            ]}
          />
          {subtitleParts.length > 0 && (
            <p className="mt-1 text-sm text-text-secondary">{subtitleParts.join(' · ')}</p>
          )}
        </div>
        <div className="flex flex-shrink-0 items-center gap-2">
          <Button
            variant="secondary"
            icon={<FolderPlus className="h-4 w-4" />}
            onClick={(e) => collectionMenu.open(e)}
          >
            {t('books_add_to_collection')}
          </Button>
          {isAdmin && series && (
            <Button
              variant="secondary"
              icon={<Pencil className="h-4 w-4" />}
              onClick={() => setEditing(true)}
            >
              {t('books_edit')}
            </Button>
          )}
        </div>
      </div>

      {series?.description && (
        <p className="mb-6 max-w-3xl text-sm text-text-secondary">{series.description}</p>
      )}

      {isLoading ? (
        <LoadingState label={t('books_loading')} />
      ) : books && books.length > 0 ? (
        <CardGrid>
          {books.map((b) => (
            <CoverCard
              key={b.id}
              to={`/books/book/${b.id}`}
              title={b.title}
              subtitle={
                b.series_index != null ? t('books_volume', { n: b.series_index }) : undefined
              }
              formats={b.formats}
              coverUrl={`${bookCoverUrl(b.id)}?v=${cacheVer}`}
              contextMenu={bookMenu({ id: b.id, title: b.title })}
            />
          ))}
        </CardGrid>
      ) : (
        <EmptyState icon={<BookOpen className="h-10 w-10" />} message={t('books_empty_books')} />
      )}

      {editing && series && (
        <SeriesMetadataEditor series={series} onClose={() => setEditing(false)} />
      )}
      {collectionMenu.isOpen && collectionMenu.pos && (
        <MenuDropdown
          items={collectionItems}
          pos={collectionMenu.pos}
          onClose={collectionMenu.close}
        />
      )}
    </div>
  )
}
