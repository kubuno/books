import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Spinner, Checkbox } from '@ui'
import { pageImageUrl } from '../api'
import { ReaderToolbar, ToolbarSelect } from './ReaderShell'
import {
  useReaderPrefs,
  useAutoHideToolbar,
  useFullscreen,
  type ImageMode,
  type ImageFit,
} from './prefs'

interface Props {
  bookId: string
  title: string
  pageCount: number
  startPage: number
  onBack: () => void
  /** Debounced progress writer (page is 0-based; pass completed on the last page). */
  onProgress: (page: number, completed: boolean) => void
}

/**
 * Reader for image archives (CBZ/CB7/CBR). Pages are server-rendered images
 * fetched by 0-based index. Supports single/double/continuous/webtoon layouts,
 * width/height/original fit, RTL (manga) navigation, click zones, keyboard
 * shortcuts, wheel-zoom in original mode and next-page prefetching.
 */
export default function ImageReader({
  bookId,
  title,
  pageCount,
  startPage,
  onBack,
  onProgress,
}: Props) {
  const { t } = useTranslation('books')
  const [prefs, setPrefs] = useReaderPrefs()
  const { imageMode: mode, imageFit: fit, rtl } = prefs

  const rootRef = useRef<HTMLDivElement>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const [isFullscreen, toggleFullscreen] = useFullscreen(rootRef.current)
  const [visible, wake] = useAutoHideToolbar()

  const [page, setPage] = useState(() => Math.min(Math.max(startPage, 0), pageCount - 1))
  // Wheel zoom factor, only meaningful in `original` fit.
  const [zoom, setZoom] = useState(1)

  const isPaged = mode === 'single' || mode === 'double'
  const isScroll = mode === 'continuous' || mode === 'webtoon'

  const url = useCallback((n: number) => pageImageUrl(bookId, n), [bookId])

  // Prefetch the next two pages so paging feels instant.
  useEffect(() => {
    for (let i = 1; i <= 2; i++) {
      const n = page + i
      if (n < pageCount) new Image().src = url(n)
    }
  }, [page, pageCount, url])

  // ── Navigation ──────────────────────────────────────────────────────────────
  const step = mode === 'double' ? 2 : 1

  const goNext = useCallback(() => {
    setPage((p) => Math.min(p + step, pageCount - 1))
  }, [step, pageCount])

  const goPrev = useCallback(() => {
    setPage((p) => Math.max(p - step, 0))
  }, [step])

  // In RTL mode the directional meaning of left/right is swapped.
  const goForward = goNext
  const goBackward = goPrev
  const onLeftZone = rtl ? goForward : goBackward
  const onRightZone = rtl ? goBackward : goForward

  const goHome = useCallback(() => setPage(0), [])
  const goEnd = useCallback(() => setPage(pageCount - 1), [pageCount])

  // Keyboard shortcuts (paged mode; scroll modes use native scrolling).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      wake()
      if (isScroll && (e.key === 'PageDown' || e.key === 'PageUp')) return
      switch (e.key) {
        case 'ArrowRight':
          onRightZone()
          break
        case 'ArrowLeft':
          onLeftZone()
          break
        case ' ':
        case 'PageDown':
          e.preventDefault()
          goForward()
          break
        case 'PageUp':
          e.preventDefault()
          goBackward()
          break
        case 'Home':
          goHome()
          break
        case 'End':
          goEnd()
          break
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onLeftZone, onRightZone, goForward, goBackward, goHome, goEnd, isScroll, wake])

  // ── Scroll-mode current-page detection via IntersectionObserver ──────────────
  const pageEls = useRef<(HTMLElement | null)[]>([])
  useEffect(() => {
    if (!isScroll) return
    const root = scrollRef.current
    if (!root) return
    const observer = new IntersectionObserver(
      (entries) => {
        // Pick the most-visible intersecting page as the "current" page.
        let best: { idx: number; ratio: number } | null = null
        for (const e of entries) {
          if (!e.isIntersecting) continue
          const idx = Number((e.target as HTMLElement).dataset.page)
          if (!best || e.intersectionRatio > best.ratio) best = { idx, ratio: e.intersectionRatio }
        }
        if (best) setPage(best.idx)
      },
      { root, threshold: [0.25, 0.5, 0.75] },
    )
    for (const el of pageEls.current) if (el) observer.observe(el)
    return () => observer.disconnect()
  }, [isScroll, pageCount])

  // ── Progress reporting (debounced) ───────────────────────────────────────────
  useEffect(() => {
    const completed = page >= pageCount - 1
    const id = window.setTimeout(() => onProgress(page, completed), 800)
    return () => window.clearTimeout(id)
  }, [page, pageCount, onProgress])

  // Wheel zoom only in `original` fit (otherwise let the browser scroll).
  const onWheel = useCallback(
    (e: React.WheelEvent) => {
      if (fit !== 'original') return
      e.preventDefault()
      setZoom((z) => Math.min(Math.max(z + (e.deltaY < 0 ? 0.1 : -0.1), 0.2), 5))
    },
    [fit],
  )

  // ── Per-page <img> class based on fit ────────────────────────────────────────
  const imgClass = useMemo(() => {
    switch (fit) {
      case 'width':
        return 'w-full h-auto'
      case 'height':
        return 'h-full w-auto max-w-none'
      case 'original':
        return 'max-w-none'
    }
  }, [fit])

  const modeOptions: { value: ImageMode; label: string }[] = [
    { value: 'single', label: t('reader_mode_single') },
    { value: 'double', label: t('reader_mode_double') },
    { value: 'continuous', label: t('reader_mode_continuous') },
    { value: 'webtoon', label: t('reader_mode_webtoon') },
  ]
  const fitOptions: { value: ImageFit; label: string }[] = [
    { value: 'width', label: t('reader_fit_width') },
    { value: 'height', label: t('reader_fit_height') },
    { value: 'original', label: t('reader_fit_original') },
  ]

  // Double-page spread: pair (0,1) | (2,3) … reversed for RTL.
  const spread = useMemo(() => {
    const a = page - (page % 2)
    const pair = [a, a + 1].filter((n) => n < pageCount)
    return rtl ? [...pair].reverse() : pair
  }, [page, pageCount, rtl])

  return (
    <div
      ref={rootRef}
      className="absolute inset-0 select-none bg-black"
      onMouseMove={wake}
      onWheel={onWheel}
    >
      <ReaderToolbar
        title={title}
        onBack={onBack}
        indicator={`${page + 1} / ${pageCount}`}
        isFullscreen={isFullscreen}
        onToggleFullscreen={toggleFullscreen}
        visible={visible}
        controls={
          <>
            <ToolbarSelect value={mode} options={modeOptions} onChange={(v) => setPrefs({ imageMode: v })} label={t('reader_mode')} />
            <ToolbarSelect value={fit} options={fitOptions} onChange={(v) => setPrefs({ imageFit: v })} label={t('reader_fit')} />
            <Checkbox
              variant="dark"
              checked={rtl}
              onChange={(v) => setPrefs({ rtl: v })}
              label={t('reader_rtl')}
              labelClassName="text-white"
            />
          </>
        }
      />

      {isPaged ? (
        <div className="relative flex h-full w-full items-center justify-center overflow-auto">
          {/* Click zones for previous / next. */}
          <button
            type="button"
            aria-label="previous"
            onClick={onLeftZone}
            className="absolute inset-y-0 left-0 z-10 w-1/3 cursor-w-resize"
          />
          <button
            type="button"
            aria-label="next"
            onClick={onRightZone}
            className="absolute inset-y-0 right-0 z-10 w-1/3 cursor-e-resize"
          />
          <div
            className="flex h-full items-center justify-center gap-1"
            style={fit === 'original' ? { transform: `scale(${zoom})` } : undefined}
          >
            {(mode === 'double' ? spread : [page]).map((n) => (
              <img
                key={n}
                src={url(n)}
                alt={`page ${n + 1}`}
                className={imgClass}
                draggable={false}
              />
            ))}
          </div>
        </div>
      ) : (
        <div
          ref={scrollRef}
          className="h-full w-full overflow-y-auto"
          style={{ scrollbarWidth: 'thin' }}
        >
          <div className={`mx-auto flex max-w-3xl flex-col items-center ${mode === 'continuous' ? 'gap-2 py-2' : 'gap-0'}`}>
            {Array.from({ length: pageCount }, (_, n) => (
              <img
                key={n}
                ref={(el) => {
                  pageEls.current[n] = el
                }}
                data-page={n}
                src={url(n)}
                alt={`page ${n + 1}`}
                loading="lazy"
                className="w-full"
                draggable={false}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  )
}

/** Centered spinner shown while the page count is being computed. */
export function ImageReaderLoading({ label }: { label: string }) {
  return (
    <div className="absolute inset-0 flex items-center justify-center bg-black">
      <Spinner size="md" label={label} />
    </div>
  )
}
