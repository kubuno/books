import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Spinner } from '@ui'
import { ChevronLeft, ChevronRight, ZoomIn, ZoomOut } from 'lucide-react'
import * as pdfjs from 'pdfjs-dist'
import type { PDFDocumentProxy } from 'pdfjs-dist'
import { formatRawUrl } from '../api'
import { ReaderToolbar, ToolbarButton } from './ReaderShell'
import { useReaderPrefs, useAutoHideToolbar, useFullscreen } from './prefs'

// Self-hosted pdf.js worker — bundled by Vite, NO external CDN (sovereignty).
pdfjs.GlobalWorkerOptions.workerSrc = new URL(
  'pdfjs-dist/build/pdf.worker.min.mjs',
  import.meta.url,
).toString()

interface Props {
  formatId: string
  title: string
  startPage: number
  onBack: () => void
  /** Debounced progress writer (page is 0-based). */
  onProgress: (page: number, completed: boolean) => void
}

/**
 * PDF reader powered by pdfjs-dist. Renders the current page to a <canvas> with
 * prev/next navigation and ±zoom. The raw file is streamed from the module with
 * cookie credentials; the worker is self-hosted (see workerSrc above).
 */
export default function PdfReader({ formatId, title, startPage, onBack, onProgress }: Props) {
  const { t } = useTranslation('books')
  const [prefs, setPrefs] = useReaderPrefs()
  const scale = prefs.pdfScale

  const rootRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const docRef = useRef<PDFDocumentProxy | null>(null)
  const [isFullscreen, toggleFullscreen] = useFullscreen(rootRef.current)
  const [visible, wake] = useAutoHideToolbar()

  const [numPages, setNumPages] = useState(0)
  const [page, setPage] = useState(startPage)
  const [loading, setLoading] = useState(true)

  // Load the document once.
  useEffect(() => {
    let cancelled = false
    const task = pdfjs.getDocument({ url: formatRawUrl(formatId), withCredentials: true })
    task.promise
      .then((doc) => {
        if (cancelled) {
          void doc.cleanup()
          return
        }
        docRef.current = doc
        setNumPages(doc.numPages)
        setPage((p) => Math.min(Math.max(p, 0), doc.numPages - 1))
        setLoading(false)
      })
      .catch(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
      // Destroying the loading task tears down the worker & document.
      void task.destroy()
      docRef.current = null
    }
  }, [formatId])

  // Render the current page whenever page or scale changes.
  useEffect(() => {
    const doc = docRef.current
    const canvas = canvasRef.current
    if (!doc || !canvas || numPages === 0) return
    let cancelled = false
    let task: { cancel: () => void } | null = null

    void doc.getPage(page + 1).then((pdfPage) => {
      if (cancelled) return
      const dpr = window.devicePixelRatio || 1
      const viewport = pdfPage.getViewport({ scale: scale * dpr })
      const ctx = canvas.getContext('2d')
      if (!ctx) return
      canvas.width = viewport.width
      canvas.height = viewport.height
      canvas.style.width = `${viewport.width / dpr}px`
      canvas.style.height = `${viewport.height / dpr}px`
      const renderTask = pdfPage.render({ canvasContext: ctx, canvas, viewport })
      task = renderTask
      renderTask.promise.catch(() => {})
    })

    return () => {
      cancelled = true
      task?.cancel()
    }
  }, [page, scale, numPages])

  const goPrev = useCallback(() => setPage((p) => Math.max(p - 1, 0)), [])
  const goNext = useCallback(() => setPage((p) => Math.min(p + 1, numPages - 1)), [numPages])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      wake()
      if (e.key === 'ArrowRight' || e.key === ' ' || e.key === 'PageDown') {
        e.preventDefault()
        goNext()
      } else if (e.key === 'ArrowLeft' || e.key === 'PageUp') {
        e.preventDefault()
        goPrev()
      } else if (e.key === 'Home') {
        setPage(0)
      } else if (e.key === 'End') {
        setPage(numPages - 1)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [goNext, goPrev, numPages, wake])

  // Debounced progress reporting.
  useEffect(() => {
    if (numPages === 0) return
    const completed = page >= numPages - 1
    const id = window.setTimeout(() => onProgress(page, completed), 800)
    return () => window.clearTimeout(id)
  }, [page, numPages, onProgress])

  const setScale = (s: number) => setPrefs({ pdfScale: Math.min(Math.max(s, 0.4), 4) })

  return (
    <div ref={rootRef} className="absolute inset-0 bg-neutral-800" onMouseMove={wake}>
      <ReaderToolbar
        title={title}
        onBack={onBack}
        indicator={numPages ? `${page + 1} / ${numPages}` : undefined}
        isFullscreen={isFullscreen}
        onToggleFullscreen={toggleFullscreen}
        visible={visible}
        controls={
          <>
            <ToolbarButton onClick={() => setScale(scale - 0.2)} title={t('reader_zoom_out')}>
              <ZoomOut className="h-4 w-4" />
            </ToolbarButton>
            <ToolbarButton onClick={() => setScale(scale + 0.2)} title={t('reader_zoom_in')}>
              <ZoomIn className="h-4 w-4" />
            </ToolbarButton>
          </>
        }
      />

      {loading ? (
        <div className="flex h-full items-center justify-center">
          <Spinner size="md" label={t('books_loading')} />
        </div>
      ) : (
        <div className="flex h-full items-center">
          <button
            type="button"
            aria-label={t('reader_prev')}
            onClick={goPrev}
            disabled={page <= 0}
            className="absolute left-0 z-10 flex h-full w-16 items-center justify-center text-white/50 hover:text-white disabled:opacity-0"
          >
            <ChevronLeft className="h-8 w-8" />
          </button>
          <div className="h-full w-full overflow-auto py-6">
            <div className="flex justify-center">
              <canvas ref={canvasRef} className="shadow-lg" />
            </div>
          </div>
          <button
            type="button"
            aria-label={t('reader_next')}
            onClick={goNext}
            disabled={page >= numPages - 1}
            className="absolute right-0 z-10 flex h-full w-16 items-center justify-center text-white/50 hover:text-white disabled:opacity-0"
          >
            <ChevronRight className="h-8 w-8" />
          </button>
        </div>
      )}
    </div>
  )
}
