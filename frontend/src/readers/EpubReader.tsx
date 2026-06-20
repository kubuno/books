import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Spinner } from '@ui'
import { ChevronLeft, ChevronRight, Minus, Plus } from 'lucide-react'
import ePub, { type Rendition } from 'epubjs'
import { formatRawUrl } from '../api'
import { ReaderToolbar, ToolbarButton, ToolbarSelect } from './ReaderShell'
import {
  useReaderPrefs,
  useAutoHideToolbar,
  useFullscreen,
  type EpubTheme,
} from './prefs'

interface Props {
  formatId: string
  title: string
  startLocation: string | null
  onBack: () => void
  /** Debounced progress writer (EPUB tracks a CFI location string). */
  onProgress: (location: string) => void
}

// Reflowable colour themes registered on the rendition.
const THEMES: Record<EpubTheme, Record<string, Record<string, string>>> = {
  light: { body: { color: '#1a1a1a', background: '#ffffff' } },
  dark: { body: { color: '#dadada', background: '#1b1b1b' } },
  sepia: { body: { color: '#5b4636', background: '#f4ecd8' } },
}

/**
 * EPUB reader powered by epub.js (epubjs bundles JSZip). Reflowable rendition
 * with light/dark/sepia themes, font-size control, prev/next + keyboard, and
 * CFI-based progress so the reader resumes at the exact spot.
 */
export default function EpubReader({ formatId, title, startLocation, onBack, onProgress }: Props) {
  const { t } = useTranslation('books')
  const [prefs, setPrefs] = useReaderPrefs()

  const rootRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<HTMLDivElement>(null)
  const renditionRef = useRef<Rendition | null>(null)
  const [isFullscreen, toggleFullscreen] = useFullscreen(rootRef.current)
  const [visible, wake] = useAutoHideToolbar()
  const [loading, setLoading] = useState(true)

  // Keep the latest progress callback without re-creating the rendition.
  const onProgressRef = useRef(onProgress)
  onProgressRef.current = onProgress

  // Build the book + rendition once.
  useEffect(() => {
    const el = viewRef.current
    if (!el) return
    const book = ePub(formatRawUrl(formatId))
    const rendition = book.renderTo(el, { width: '100%', height: '100%', spread: 'auto' })
    renditionRef.current = rendition

    // Register & apply themes / initial font size.
    for (const [name, rules] of Object.entries(THEMES)) {
      rendition.themes.register(name, rules)
    }
    rendition.themes.select(prefs.epubTheme)
    rendition.themes.fontSize(`${prefs.epubFontSize}%`)

    // Debounced CFI progress persistence.
    let timer: number | undefined
    rendition.on('relocated', (loc: { start?: { cfi?: string } }) => {
      const cfi = loc?.start?.cfi
      if (!cfi) return
      window.clearTimeout(timer)
      timer = window.setTimeout(() => onProgressRef.current(cfi), 800)
    })

    void rendition.display(startLocation || undefined).then(() => setLoading(false))

    return () => {
      window.clearTimeout(timer)
      rendition.destroy()
      void book.destroy()
      renditionRef.current = null
    }
    // Intentionally only re-create when the source file changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [formatId])

  const goPrev = useCallback(() => void renditionRef.current?.prev(), [])
  const goNext = useCallback(() => void renditionRef.current?.next(), [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      wake()
      if (e.key === 'ArrowRight' || e.key === ' ' || e.key === 'PageDown') {
        e.preventDefault()
        goNext()
      } else if (e.key === 'ArrowLeft' || e.key === 'PageUp') {
        e.preventDefault()
        goPrev()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [goNext, goPrev, wake])

  const setTheme = (theme: EpubTheme) => {
    setPrefs({ epubTheme: theme })
    renditionRef.current?.themes.select(theme)
  }
  const setFontSize = (size: number) => {
    const clamped = Math.min(Math.max(size, 60), 250)
    setPrefs({ epubFontSize: clamped })
    renditionRef.current?.themes.fontSize(`${clamped}%`)
  }

  const themeOptions: { value: EpubTheme; label: string }[] = [
    { value: 'light', label: t('reader_theme_light') },
    { value: 'dark', label: t('reader_theme_dark') },
    { value: 'sepia', label: t('reader_theme_sepia') },
  ]

  const bg =
    prefs.epubTheme === 'dark' ? '#1b1b1b' : prefs.epubTheme === 'sepia' ? '#f4ecd8' : '#ffffff'

  return (
    <div ref={rootRef} className="absolute inset-0" style={{ background: bg }} onMouseMove={wake}>
      <ReaderToolbar
        title={title}
        onBack={onBack}
        isFullscreen={isFullscreen}
        onToggleFullscreen={toggleFullscreen}
        visible={visible}
        controls={
          <>
            <ToolbarSelect value={prefs.epubTheme} options={themeOptions} onChange={setTheme} label={t('reader_theme')} />
            <ToolbarButton onClick={() => setFontSize(prefs.epubFontSize - 10)} title={t('reader_font_smaller')}>
              <Minus className="h-4 w-4" />
            </ToolbarButton>
            <ToolbarButton onClick={() => setFontSize(prefs.epubFontSize + 10)} title={t('reader_font_larger')}>
              <Plus className="h-4 w-4" />
            </ToolbarButton>
          </>
        }
      />

      {loading && (
        <div className="pointer-events-none absolute inset-0 z-10 flex items-center justify-center">
          <Spinner size="md" label={t('books_loading')} />
        </div>
      )}

      <button
        type="button"
        aria-label={t('reader_prev')}
        onClick={goPrev}
        className="absolute left-0 top-1/2 z-10 flex h-12 w-12 -translate-y-1/2 items-center justify-center text-text-tertiary hover:text-text-primary"
      >
        <ChevronLeft className="h-7 w-7" />
      </button>
      <div ref={viewRef} className="mx-auto h-full max-w-4xl px-12" />
      <button
        type="button"
        aria-label={t('reader_next')}
        onClick={goNext}
        className="absolute right-0 top-1/2 z-10 flex h-12 w-12 -translate-y-1/2 items-center justify-center text-text-tertiary hover:text-text-primary"
      >
        <ChevronRight className="h-7 w-7" />
      </button>
    </div>
  )
}
