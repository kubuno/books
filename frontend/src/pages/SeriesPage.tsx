import { useState } from 'react'
import { useParams } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useAuthStore, useImageCacheStore } from '@kubuno/sdk'
import { Button, MenuDropdown, useMenuDropdown, type MenuItem } from '@ui'
import { BookOpen, Pencil, FolderPlus, Plus, Globe, Library as LibraryIcon } from 'lucide-react'
import {
  getSeries,
  getSeriesBooks,
  bookCoverUrl,
  seriesCoverUrl,
  listCollections,
  createCollection,
  addSeriesToCollection,
  applyOnlineSeriesMetadata,
  type SeriesOnlineResult,
} from '../api'
import CoverCard from '../components/CoverCard'
import { useBookContextMenu } from '../components/bookMenu'
import SeriesMetadataEditor from '../components/SeriesMetadataEditor'
import SeriesOnlineDialog from '../components/SeriesOnlineDialog'
import { Breadcrumb } from '../components/Breadcrumb'
import { CardGrid, LoadingState, EmptyState } from '../components/shared'

/** Small labelled fact in the info panel. */
function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[11px] font-medium uppercase tracking-wide text-text-tertiary">{label}</span>
      <span className="text-sm text-text-primary">{children}</span>
    </div>
  )
}

/** A series presentation page: artwork + synopsis + metadata, then its books. */
export default function SeriesPage() {
  const { id = '' } = useParams<{ id: string }>()
  const { t } = useTranslation('books')
  const cacheVer = useImageCacheStore((s) => s.global)
  const user = useAuthStore((s) => s.user)
  const isAdmin = user?.role === 'admin'
  const qc = useQueryClient()
  const [editing, setEditing] = useState(false)
  const [onlineOpen, setOnlineOpen] = useState(false)
  const [coverFailed, setCoverFailed] = useState(false)
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

  const { data: books, isLoading } = useQuery({
    queryKey: ['books', 'series-books', id],
    queryFn: () => getSeriesBooks(id),
    enabled: !!id,
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

  async function applyOnline(r: SeriesOnlineResult, downloadCover: boolean) {
    await applyOnlineSeriesMetadata(id, {
      description: r.description ?? undefined,
      publisher: r.publisher ?? undefined,
      genres: r.genres.length ? r.genres : undefined,
      cover_url: r.cover_url ?? undefined,
      download_cover: downloadCover && !!r.cover_url,
    })
    setOnlineOpen(false)
    setCoverFailed(false)
    void qc.invalidateQueries({ queryKey: ['books', 'series-detail', id] })
    useImageCacheStore.getState().bumpAll()
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

  const genres = series?.genres ?? []
  const tags = series?.tags ?? []
  // Always attempt the cover URL (it serves the downloaded series artwork OR a
  // book-derived cover); fall back to an icon only once the image actually fails.
  const showCover = !!series && !coverFailed

  return (
    <div className="w-full p-6" data-module="books">
      <Breadcrumb
        crumbs={[
          { label: t('libraries', { defaultValue: 'Bibliothèques' }), to: '/books' },
          ...(library ? [{ label: library.name, to: `/books/library/${library.id}` }] : []),
          { label: series?.name ?? t('books_series') },
        ]}
      />

      {/* ── Presentation header ── */}
      <div className="mt-4 mb-8 flex flex-col gap-6 sm:flex-row">
        {/* Artwork */}
        <div className="mx-auto sm:mx-0 h-72 w-48 flex-shrink-0 overflow-hidden rounded-xl bg-surface-2 shadow-md ring-1 ring-border">
          {showCover ? (
            <img
              src={`${seriesCoverUrl(id)}?v=${cacheVer}`}
              alt={series?.name ?? ''}
              className="h-full w-full object-cover"
              onError={() => setCoverFailed(true)}
            />
          ) : (
            <div className="flex h-full w-full items-center justify-center">
              <LibraryIcon className="h-12 w-12 text-text-tertiary" strokeWidth={1.2} />
            </div>
          )}
        </div>

        {/* Title + facts + synopsis */}
        <div className="min-w-0 flex-1">
          <div className="flex items-start justify-between gap-4">
            <h1 className="text-2xl font-semibold text-text-primary">{series?.name}</h1>
            <div className="flex flex-shrink-0 items-center gap-2">
              <Button
                variant="secondary"
                icon={<FolderPlus className="h-4 w-4" />}
                onClick={(e) => collectionMenu.open(e)}
              >
                {t('books_add_to_collection')}
              </Button>
              {isAdmin && series && (
                <>
                  <Button
                    variant="secondary"
                    icon={<Globe className="h-4 w-4" />}
                    onClick={() => setOnlineOpen(true)}
                  >
                    {t('books_series_online_action', { defaultValue: 'Infos en ligne' })}
                  </Button>
                  <Button variant="secondary" icon={<Pencil className="h-4 w-4" />} onClick={() => setEditing(true)}>
                    {t('books_edit')}
                  </Button>
                </>
              )}
            </div>
          </div>

          {/* Facts grid */}
          <div className="mt-4 grid grid-cols-2 gap-4 sm:grid-cols-3">
            <Fact label={t('books_book_count_label', { defaultValue: 'Livres' })}>
              {series ? (series.total_book_count || series.book_count) : '—'}
            </Fact>
            {series?.publisher && (
              <Fact label={t('books_field_publisher', { defaultValue: 'Éditeur' })}>{series.publisher}</Fact>
            )}
            {series?.language && (
              <Fact label={t('books_md_language', { defaultValue: 'Langue' })}>{series.language}</Fact>
            )}
          </div>

          {/* Genres */}
          {genres.length > 0 && (
            <div className="mt-4 flex flex-wrap gap-1.5">
              {genres.map((g) => (
                <span key={g} className="rounded-full bg-primary-light px-2.5 py-0.5 text-xs text-primary">{g}</span>
              ))}
            </div>
          )}

          {/* Synopsis */}
          {series?.description ? (
            <p className="mt-4 max-w-3xl whitespace-pre-line text-sm leading-relaxed text-text-secondary">
              {series.description}
            </p>
          ) : (
            <p className="mt-4 text-sm italic text-text-tertiary">
              {t('books_series_no_description', { defaultValue: 'Aucune présentation pour cette série.' })}
              {isAdmin && (
                <>
                  {' '}
                  <button className="text-primary hover:underline" onClick={() => setOnlineOpen(true)}>
                    {t('books_series_online_action', { defaultValue: 'Infos en ligne' })}
                  </button>
                </>
              )}
            </p>
          )}

          {/* Tags */}
          {tags.length > 0 && (
            <div className="mt-3 flex flex-wrap gap-1.5">
              {tags.map((tg) => (
                <span key={tg} className="rounded bg-surface-2 px-2 py-0.5 text-[11px] text-text-secondary">#{tg}</span>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* ── Books grid ── */}
      <h2 className="mb-3 text-sm font-semibold uppercase tracking-wide text-text-tertiary">
        {t('books_in_series', { defaultValue: 'Dans cette série' })}
      </h2>
      {isLoading ? (
        <LoadingState label={t('books_loading')} />
      ) : books && books.length > 0 ? (
        <CardGrid>
          {books.map((b) => (
            <CoverCard
              key={b.id}
              to={`/books/book/${b.id}`}
              title={b.title}
              subtitle={b.series_index != null ? t('books_volume', { n: b.series_index }) : undefined}
              formats={b.formats}
              coverUrl={`${bookCoverUrl(b.id)}?v=${cacheVer}`}
              contextMenu={bookMenu({ id: b.id, title: b.title })}
            />
          ))}
        </CardGrid>
      ) : (
        <EmptyState icon={<BookOpen className="h-10 w-10" />} message={t('books_empty_books')} />
      )}

      {editing && series && <SeriesMetadataEditor series={series} onClose={() => setEditing(false)} />}
      {onlineOpen && series && (
        <SeriesOnlineDialog
          initialQuery={series.name}
          onClose={() => setOnlineOpen(false)}
          onApply={(r, dl) => void applyOnline(r, dl)}
        />
      )}
      {collectionMenu.isOpen && collectionMenu.pos && (
        <MenuDropdown items={collectionItems} pos={collectionMenu.pos} onClose={collectionMenu.close} />
      )}
    </div>
  )
}
