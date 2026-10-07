import { useCallback, useEffect, useState } from 'react'

/**
 * Persisted reader display preferences (localStorage). Shared by the image, PDF
 * and EPUB readers so a returning reader keeps their last viewing choices.
 */
export type ImageMode = 'single' | 'double' | 'continuous' | 'webtoon'
export type ImageFit = 'width' | 'height' | 'original'
export type EpubTheme = 'light' | 'dark' | 'sepia'

export interface ReaderPrefs {
  /** Image (CBZ/CB7/CBR) reading layout. */
  imageMode: ImageMode
  /** Image fit/zoom strategy. */
  imageFit: ImageFit
  /** Manga-style right-to-left navigation & spread order. */
  rtl: boolean
  /** PDF zoom scale. */
  pdfScale: number
  /** EPUB colour theme. */
  epubTheme: EpubTheme
  /** EPUB font size, in percent. */
  epubFontSize: number
}

const STORAGE_KEY = 'kubuno.books.reader'

const DEFAULTS: ReaderPrefs = {
  imageMode: 'single',
  imageFit: 'height',
  rtl: false,
  pdfScale: 1.2,
  epubTheme: 'light',
  epubFontSize: 100,
}

function load(): ReaderPrefs {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return DEFAULTS
    return { ...DEFAULTS, ...(JSON.parse(raw) as Partial<ReaderPrefs>) }
  } catch {
    return DEFAULTS
  }
}

/** Reader preferences with a setter that persists to localStorage. */
export function useReaderPrefs(): [ReaderPrefs, (patch: Partial<ReaderPrefs>) => void] {
  const [prefs, setPrefs] = useState<ReaderPrefs>(load)

  const update = useCallback((patch: Partial<ReaderPrefs>) => {
    setPrefs((prev) => {
      const next = { ...prev, ...patch }
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(next))
      } catch {
        // Ignore quota / private-mode write failures.
      }
      return next
    })
  }, [])

  return [prefs, update]
}

/** Tracks native fullscreen state for `el` and exposes a toggle. */
export function useFullscreen(el: HTMLElement | null): [boolean, () => void] {
  const [isFullscreen, setIsFullscreen] = useState(false)

  useEffect(() => {
    const onChange = () => setIsFullscreen(document.fullscreenElement != null)
    document.addEventListener('fullscreenchange', onChange)
    return () => document.removeEventListener('fullscreenchange', onChange)
  }, [])

  const toggle = useCallback(() => {
    if (document.fullscreenElement) {
      void document.exitFullscreen().catch(() => {})
    } else if (el) {
      void el.requestFullscreen().catch(() => {})
    }
  }, [el])

  return [isFullscreen, toggle]
}

/** Auto-hides a toolbar after the mouse has been idle for `delayMs`. */
export function useAutoHideToolbar(delayMs = 2500): [boolean, () => void] {
  const [visible, setVisible] = useState(true)

  const wake = useCallback(() => setVisible(true), [])

  useEffect(() => {
    if (!visible) return
    const timer = window.setTimeout(() => setVisible(false), delayMs)
    return () => window.clearTimeout(timer)
  }, [visible, delayMs])

  return [visible, wake]
}
