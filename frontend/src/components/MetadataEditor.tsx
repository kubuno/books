import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useQueryClient } from '@tanstack/react-query'
import { FloatingWindow, Button, Input, Textarea, Dropdown, NumberInput } from '@ui'
import { useImageCacheStore } from '@kubuno/sdk'
import { Pencil, AlertTriangle, FileDown, Globe } from 'lucide-react'
import {
  updateBook,
  refreshBookMetadata,
  applyOnlineMetadata,
  type Book,
  type AuthorEntry,
  type OnlineMetadataResult,
} from '../api'
import { ChipsField, AuthorsField, StarRating } from './fields'
import OnlineMetadataDialog from './OnlineMetadataDialog'

interface Props {
  book: Book
  onClose: () => void
}

const DIRECTIONS = ['ltr', 'rtl', 'vertical', 'webtoon'] as const
type Direction = (typeof DIRECTIONS)[number]

/** P4 — full metadata editor for a single book (admin only). */
export default function MetadataEditor({ book, onClose }: Props) {
  const { t } = useTranslation('books')
  const qc = useQueryClient()
  const bumpCache = useImageCacheStore((s) => s.bumpAll)

  const [title, setTitle] = useState(book.title)
  const [sortTitle, setSortTitle] = useState(book.sort_title)
  const [seriesIndex, setSeriesIndex] = useState<string>(
    book.series_index != null ? String(book.series_index) : '',
  )
  const [description, setDescription] = useState(book.description ?? '')
  const [publisher, setPublisher] = useState(book.publisher ?? '')
  const [publishedDate, setPublishedDate] = useState(book.published_date ?? '')
  const [isbn, setIsbn] = useState(book.isbn ?? '')
  const [language, setLanguage] = useState(book.language ?? '')
  const [rating, setRating] = useState<number | null>(book.rating)
  const [ageRating, setAgeRating] = useState<number>(
    book.age_rating != null ? Number(book.age_rating) : 0,
  )
  const [direction, setDirection] = useState<Direction>(
    (book.metadata?.reading_direction as Direction) ?? 'ltr',
  )
  const [authors, setAuthors] = useState<AuthorEntry[]>(
    book.authors.map((name) => ({ name })),
  )
  const [tags, setTags] = useState<string[]>(book.tags)

  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [onlineOpen, setOnlineOpen] = useState(false)

  const directionOptions = DIRECTIONS.map((d) => ({
    value: d,
    label: t(`books_direction_${d}`),
  }))

  function invalidate() {
    void qc.invalidateQueries({ queryKey: ['books', 'book', book.id] })
    if (book.series_id) {
      void qc.invalidateQueries({ queryKey: ['books', 'series-books', book.series_id] })
    }
  }

  async function save() {
    setSaving(true)
    setError(null)
    try {
      await updateBook(book.id, {
        title: title.trim(),
        sort_title: sortTitle.trim(),
        series_index: seriesIndex.trim() ? Number(seriesIndex) : null,
        description: description.trim() || null,
        publisher: publisher.trim() || null,
        published_date: publishedDate.trim() || null,
        isbn: isbn.trim() || null,
        language: language.trim() || null,
        rating,
        age_rating: ageRating || null,
        reading_direction: direction,
        authors: authors.filter((a) => a.name.trim()),
        tags,
      })
      invalidate()
      onClose()
    } catch {
      setError(t('books_save_error'))
      setSaving(false)
    }
  }

  async function importFromFile() {
    setSaving(true)
    setError(null)
    try {
      await refreshBookMetadata(book.id)
      invalidate()
      onClose()
    } catch {
      setError(t('books_save_error'))
      setSaving(false)
    }
  }

  /** Pre-fill editor fields from an online match (P5). */
  function prefillFromOnline(r: OnlineMetadataResult, downloadCover: boolean) {
    if (r.title) setTitle(r.title)
    if (r.authors.length > 0) setAuthors(r.authors.map((name) => ({ name })))
    if (r.publisher) setPublisher(r.publisher)
    if (r.date) setPublishedDate(r.date)
    if (r.isbn) setIsbn(r.isbn)
    if (r.description) setDescription(r.description)
    if (r.language) setLanguage(r.language)
    if (r.tags.length > 0) setTags(Array.from(new Set([...tags, ...r.tags])))
    setOnlineOpen(false)
    // If the cover should be downloaded, apply it server-side immediately.
    if (downloadCover && r.cover_url) {
      void applyOnlineMetadata(book.id, { cover_url: r.cover_url, download_cover: true })
        .then(() => {
          bumpCache()
          invalidate()
        })
        .catch(() => {
          /* cover download is best-effort */
        })
    }
  }

  return (
    <FloatingWindow
      title={t('books_edit_title')}
      icon={<Pencil className="h-4 w-4" />}
      onClose={onClose}
      defaultWidth={620}
      defaultHeight={640}
      backdrop
    >
      <div className="flex h-full flex-col" data-module="books">
        <div className="flex flex-wrap gap-2 border-b border-border p-4">
          <Button
            variant="secondary"
            icon={<Globe className="h-4 w-4" />}
            onClick={() => setOnlineOpen(true)}
          >
            {t('books_online_button')}
          </Button>
          <Button
            variant="secondary"
            icon={<FileDown className="h-4 w-4" />}
            onClick={importFromFile}
            disabled={saving}
          >
            {t('books_import_file')}
          </Button>
        </div>

        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-5">
          <Input
            label={t('books_field_title')}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
          <Input
            label={t('books_field_sort_title')}
            value={sortTitle}
            onChange={(e) => setSortTitle(e.target.value)}
          />

          <AuthorsField label={t('books_field_authors')} value={authors} onChange={setAuthors} />

          <Textarea
            label={t('books_field_description')}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={4}
          />

          <div className="grid grid-cols-2 gap-4">
            <Input
              label={t('books_field_publisher')}
              value={publisher}
              onChange={(e) => setPublisher(e.target.value)}
            />
            <Input
              label={t('books_field_published_date')}
              value={publishedDate}
              onChange={(e) => setPublishedDate(e.target.value)}
              placeholder="YYYY-MM-DD"
            />
            <Input
              label={t('books_field_isbn')}
              value={isbn}
              onChange={(e) => setIsbn(e.target.value)}
            />
            <Input
              label={t('books_field_language')}
              value={language}
              onChange={(e) => setLanguage(e.target.value)}
              placeholder="fr, en…"
            />
            <Input
              label={t('books_field_series_index')}
              type="number"
              value={seriesIndex}
              onChange={(e) => setSeriesIndex(e.target.value)}
            />
            <NumberInput
              label={t('books_field_age_rating')}
              value={ageRating}
              onChange={setAgeRating}
              min={0}
              max={21}
            />
          </div>

          <div className="grid grid-cols-2 items-start gap-4">
            <div>
              <label className="mb-1.5 block text-sm font-medium text-text-secondary">
                {t('books_field_direction')}
              </label>
              <Dropdown
                value={direction}
                onChange={(v) => setDirection(v as Direction)}
                options={directionOptions}
                width="100%"
                height={36}
              />
            </div>
            <StarRating label={t('books_field_rating')} value={rating} onChange={setRating} />
          </div>

          <ChipsField
            label={t('books_field_tags')}
            value={tags}
            onChange={setTags}
            placeholder={t('books_field_tags_placeholder')}
          />

          {error && (
            <p className="flex items-center gap-1.5 text-sm text-danger">
              <AlertTriangle className="h-4 w-4 flex-shrink-0" />
              {error}
            </p>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-border p-4">
          <Button variant="ghost" onClick={onClose} disabled={saving}>
            {t('common_cancel')}
          </Button>
          <Button variant="primary" onClick={save} loading={saving} disabled={!title.trim()}>
            {t('common_save')}
          </Button>
        </div>
      </div>

      {onlineOpen && (
        <OnlineMetadataDialog
          initialQuery={[book.title, book.authors[0]].filter(Boolean).join(' ')}
          onClose={() => setOnlineOpen(false)}
          onApply={prefillFromOnline}
        />
      )}
    </FloatingWindow>
  )
}
