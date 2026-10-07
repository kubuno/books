import { useCallback, useMemo } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery } from '@tanstack/react-query'
import { Spinner } from '@ui'
import { getBook, getProgress, getPageCount, putProgress, type BookFormat } from '../api'
import ImageReader from '../readers/ImageReader'
import PdfReader from '../readers/PdfReader'
import EpubReader from '../readers/EpubReader'

// Format priority: image archives first, then PDF, then EPUB.
const IMAGE_FORMATS = new Set(['cbz', 'cb7', 'cbr'])

/** Picks the primary readable format for a book. */
function pickFormat(formats: BookFormat[]): BookFormat | null {
  const lower = (f: BookFormat) => f.format.toLowerCase()
  return (
    formats.find((f) => IMAGE_FORMATS.has(lower(f))) ||
    formats.find((f) => lower(f) === 'pdf') ||
    formats.find((f) => lower(f) === 'epub') ||
    null
  )
}

/**
 * Full-screen reader overlay. Loads the book + saved progress, selects the
 * primary format and mounts the matching reader (image / PDF / EPUB). Rendered
 * above the host shell; "back" returns to the book detail page.
 */
export default function ReaderView() {
  const { id = '' } = useParams<{ id: string }>()
  const { t } = useTranslation('books')
  const navigate = useNavigate()

  const onBack = useCallback(() => navigate(`/books/book/${id}`), [navigate, id])

  const { data: bookData, isLoading: bookLoading } = useQuery({
    queryKey: ['books', 'book', id],
    queryFn: () => getBook(id),
    enabled: !!id,
  })

  const { data: progress, isLoading: progressLoading } = useQuery({
    queryKey: ['books', 'progress', id],
    queryFn: () => getProgress(id),
    enabled: !!id,
  })

  const format = useMemo(
    () => (bookData ? pickFormat(bookData.formats) : null),
    [bookData],
  )
  const isImage = !!format && IMAGE_FORMATS.has(format.format.toLowerCase())

  // For image archives, fetch the page count (this also triggers server indexing).
  const { data: imagePages } = useQuery({
    queryKey: ['books', 'pages', id],
    queryFn: () => getPageCount(id),
    enabled: isImage,
  })

  // Progress writers (debounced inside each reader).
  const writePage = useCallback(
    (page: number, completed: boolean) => {
      void putProgress(id, { page, completed })
    },
    [id],
  )
  const writeLocation = useCallback(
    (location: string) => {
      void putProgress(id, { location })
    },
    [id],
  )

  const title = bookData?.book.title ?? ''

  // Overlay container: fixed, above the shell.
  const overlay = (children: React.ReactNode) => (
    <div className="fixed inset-0 z-[1000] bg-black" data-module="books">
      {children}
    </div>
  )

  if (bookLoading || progressLoading || (isImage && imagePages == null)) {
    return overlay(
      <div className="flex h-full items-center justify-center">
        <Spinner size="md" label={t('books_loading')} />
      </div>,
    )
  }

  if (!bookData || !format) {
    return overlay(
      <div className="flex h-full flex-col items-center justify-center gap-4 text-white">
        <p>{t('reader_no_format')}</p>
        <button
          type="button"
          onClick={onBack}
          className="rounded bg-white/10 px-4 py-2 text-sm hover:bg-white/20"
        >
          {t('common_back')}
        </button>
      </div>,
    )
  }

  const startPage = progress?.page ?? 0
  const startLocation = progress?.location ?? null
  const fmt = format.format.toLowerCase()

  if (isImage) {
    return overlay(
      <ImageReader
        bookId={id}
        title={title}
        pageCount={imagePages ?? 0}
        startPage={startPage}
        onBack={onBack}
        onProgress={writePage}
      />,
    )
  }

  if (fmt === 'pdf') {
    return overlay(
      <PdfReader
        formatId={format.id}
        title={title}
        startPage={startPage}
        onBack={onBack}
        onProgress={writePage}
      />,
    )
  }

  return overlay(
    <EpubReader
      formatId={format.id}
      title={title}
      startLocation={startLocation}
      onBack={onBack}
      onProgress={writeLocation}
    />,
  )
}
