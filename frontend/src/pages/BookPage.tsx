import { useEffect, useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, MenuDropdown, useMenuDropdown, type MenuItem } from '@ui'
import { formatSize, useAuthStore, useImageCacheStore } from '@kubuno/sdk'
import {
  BookOpen,
  FileText,
  Hash,
  Calendar,
  User,
  Tag,
  Check,
  RotateCcw,
  Pencil,
  Download,
  ListPlus,
  Plus,
} from 'lucide-react'
import {
  getBook,
  getProgress,
  markRead,
  markUnread,
  bookCoverUrl,
  bookDownloadUrl,
  listReadLists,
  createReadList,
  addBookToReadList,
  type BookFormat,
} from '../api'
import FormatBadge from '../components/FormatBadge'
import MetadataEditor from '../components/MetadataEditor'
import { Breadcrumb, type Crumb } from '../components/Breadcrumb'
import { LoadingState, EmptyState } from '../components/shared'

function MetaRow({
  icon,
  label,
  value,
}: {
  icon: React.ReactNode
  label: string
  value: React.ReactNode
}) {
  return (
    <div className="flex items-start gap-2 py-1.5 text-sm">
      <span className="mt-0.5 flex-shrink-0 text-text-tertiary">{icon}</span>
      <span className="w-28 flex-shrink-0 text-text-secondary">{label}</span>
      <span className="min-w-0 text-text-primary">{value}</span>
    </div>
  )
}

/** Book detail: cover placeholder, metadata, format list (reader lands in P3). */
export default function BookPage() {
  const { id = '' } = useParams<{ id: string }>()
  const { t } = useTranslation('books')
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const cacheVer = useImageCacheStore((s) => s.global)
  const user = useAuthStore((s) => s.user)
  const isAdmin = user?.role === 'admin'
  // Falls back to the placeholder when the cover image fails to load.
  const [coverFailed, setCoverFailed] = useState(false)
  const [editing, setEditing] = useState(false)
  const readListMenu = useMenuDropdown()

  const { data, isLoading } = useQuery({
    queryKey: ['books', 'book', id],
    queryFn: () => getBook(id),
    enabled: !!id,
  })

  // Reset the broken-cover flag when navigating to another book or refreshing the cover.
  useEffect(() => setCoverFailed(false), [id, cacheVer])

  const { data: readLists } = useQuery({
    queryKey: ['books', 'readlists'],
    queryFn: listReadLists,
  })

  async function addToReadList(listId: string) {
    await addBookToReadList(listId, id)
    void queryClient.invalidateQueries({ queryKey: ['books', 'readlists'] })
    void queryClient.invalidateQueries({ queryKey: ['books', 'readlist', listId] })
  }

  async function addToNewReadList() {
    const created = await createReadList({ name: data?.book.title ?? 'Liste' })
    await addToReadList(created.id)
  }

  const { data: progress } = useQuery({
    queryKey: ['books', 'progress', id],
    queryFn: () => getProgress(id),
    enabled: !!id,
  })

  const invalidateProgress = () => {
    void queryClient.invalidateQueries({ queryKey: ['books', 'progress', id] })
    void queryClient.invalidateQueries({ queryKey: ['books', 'keep-reading'] })
  }

  if (isLoading) {
    return (
      <div className="mx-auto max-w-5xl p-6" data-module="books">
        <LoadingState label={t('books_loading')} />
      </div>
    )
  }

  if (!data) {
    return (
      <div className="mx-auto max-w-5xl p-6" data-module="books">
        <EmptyState icon={<BookOpen className="h-10 w-10" />} message={t('books_empty_books')} />
      </div>
    )
  }

  const { book, formats, library, series } = data

  const crumbs: Crumb[] = [
    { label: t('libraries', { defaultValue: 'Bibliothèques' }), to: '/books' },
    { label: library.name, to: `/books/library/${library.id}` },
    ...(series ? [{ label: series.name, to: `/books/series/${series.id}` }] : []),
    { label: book.title },
  ]

  const coverSrc = `${bookCoverUrl(book.id)}?v=${cacheVer}`

  const readListItems: MenuItem[] = [
    ...(readLists ?? []).map(
      (l): MenuItem => ({
        type: 'action',
        label: l.name,
        icon: <ListPlus className="h-4 w-4" />,
        onClick: () => void addToReadList(l.id),
      }),
    ),
    ...(readLists && readLists.length > 0 ? [{ type: 'separator' as const }] : []),
    {
      type: 'action',
      label: t('books_readlist_new'),
      icon: <Plus className="h-4 w-4" />,
      onClick: () => void addToNewReadList(),
    },
  ]

  return (
    <div className="mx-auto max-w-5xl p-6" data-module="books">
      <div className="mb-6 flex items-start justify-between gap-4">
        <Breadcrumb crumbs={crumbs} />
        {isAdmin && (
          <Button
            variant="secondary"
            icon={<Pencil className="h-4 w-4" />}
            onClick={() => setEditing(true)}
          >
            {t('books_edit')}
          </Button>
        )}
      </div>

      <div className="flex flex-col gap-6 md:flex-row">
        {/* Cover image (falls back to a placeholder for non-CBZ books). */}
        <div className="flex w-44 flex-shrink-0 flex-col gap-3">
          <div className="relative flex aspect-[2/3] w-full items-center justify-center overflow-hidden rounded-lg bg-primary-light">
            {coverFailed ? (
              <BookOpen className="h-12 w-12 text-primary/60" />
            ) : (
              <img
                src={coverSrc}
                alt={book.title}
                className="h-full w-full object-cover"
                loading="lazy"
                onError={() => setCoverFailed(true)}
              />
            )}
          </div>
          <Button
            variant="primary"
            onClick={() => navigate(`/books/read/${book.id}`)}
            className="w-full"
          >
            {progress && progress.page > 0 && !progress.completed
              ? t('books_continue', { page: progress.page + 1 })
              : t('books_read')}
          </Button>
          {progress?.completed ? (
            <Button
              variant="secondary"
              icon={<RotateCcw className="h-4 w-4" />}
              onClick={() => markUnread(book.id).then(invalidateProgress)}
              className="w-full"
            >
              {t('books_mark_unread')}
            </Button>
          ) : (
            <Button
              variant="secondary"
              icon={<Check className="h-4 w-4" />}
              onClick={() => markRead(book.id).then(invalidateProgress)}
              className="w-full"
            >
              {t('books_mark_read')}
            </Button>
          )}

          <a
            href={bookDownloadUrl(book.id)}
            download
            className="inline-flex w-full items-center justify-center gap-2 rounded-md border border-border bg-surface-0 px-3 py-1.5 text-sm font-medium text-text-primary transition hover:bg-surface-1"
          >
            <Download className="h-4 w-4" />
            {t('books_download')}
          </a>

          <Button
            variant="ghost"
            icon={<ListPlus className="h-4 w-4" />}
            onClick={(e) => readListMenu.open(e)}
            className="w-full"
          >
            {t('books_add_to_readlist')}
          </Button>
        </div>

        <div className="min-w-0 flex-1">
          <h1 className="text-2xl font-semibold text-text-primary">{book.title}</h1>
          {book.authors.length > 0 && (
            <p className="mt-1 text-sm text-text-secondary">{book.authors.join(', ')}</p>
          )}

          {book.description && (
            <p className="mt-4 max-w-2xl whitespace-pre-line text-sm text-text-secondary">
              {book.description}
            </p>
          )}

          <div className="mt-6 divide-y divide-border rounded-lg border border-border bg-surface-0 px-4 py-2">
            {book.publisher && (
              <MetaRow
                icon={<User className="h-4 w-4" />}
                label={t('books_meta_publisher')}
                value={book.publisher}
              />
            )}
            {book.published_date && (
              <MetaRow
                icon={<Calendar className="h-4 w-4" />}
                label={t('books_meta_published')}
                value={book.published_date}
              />
            )}
            {book.language && (
              <MetaRow
                icon={<FileText className="h-4 w-4" />}
                label={t('books_meta_language')}
                value={book.language}
              />
            )}
            {book.isbn && (
              <MetaRow
                icon={<Hash className="h-4 w-4" />}
                label={t('books_meta_isbn')}
                value={book.isbn}
              />
            )}
            {book.page_count != null && (
              <MetaRow
                icon={<FileText className="h-4 w-4" />}
                label={t('books_meta_pages')}
                value={book.page_count}
              />
            )}
            {book.tags.length > 0 && (
              <MetaRow
                icon={<Tag className="h-4 w-4" />}
                label={t('books_meta_tags')}
                value={
                  <span className="flex flex-wrap gap-1">
                    {book.tags.map((tag) => (
                      <span
                        key={tag}
                        className="rounded bg-surface-2 px-1.5 py-0.5 text-xs text-text-secondary"
                      >
                        {tag}
                      </span>
                    ))}
                  </span>
                }
              />
            )}
          </div>

          {/* Available formats. */}
          <div className="mt-6">
            <h2 className="mb-2 text-sm font-semibold text-text-primary">
              {t('books_section_formats')}
            </h2>
            <ul className="space-y-1.5">
              {formats.map((f: BookFormat) => (
                <li
                  key={f.id}
                  className="flex items-center gap-3 rounded-lg border border-border bg-surface-0 px-3 py-2"
                >
                  <FormatBadge format={f.format} />
                  <span className="min-w-0 flex-1 truncate text-sm text-text-primary" title={f.file_name}>
                    {f.file_name}
                  </span>
                  {f.page_count != null && (
                    <span className="text-xs text-text-tertiary">
                      {t('books_pages_n', { count: f.page_count })}
                    </span>
                  )}
                  <span className="text-xs text-text-tertiary">{formatSize(f.size_bytes)}</span>
                </li>
              ))}
            </ul>
          </div>
        </div>
      </div>

      {editing && <MetadataEditor book={book} onClose={() => setEditing(false)} />}
      {readListMenu.isOpen && readListMenu.pos && (
        <MenuDropdown
          items={readListItems}
          pos={readListMenu.pos}
          onClose={readListMenu.close}
        />
      )}
    </div>
  )
}
