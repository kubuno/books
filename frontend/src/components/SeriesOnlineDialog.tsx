import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button, Input, Checkbox } from '@ui'
import { Globe, Search, Library as LibraryIcon } from 'lucide-react'
import { searchOnlineSeries, type SeriesOnlineResult } from '../api'
import { LoadingState, EmptyState } from './shared'

interface Props {
  /** Initial query (the series name). */
  initialQuery: string
  onClose: () => void
  onApply: (result: SeriesOnlineResult, downloadCover: boolean) => void
}

/**
 * Search the web (Wikipedia / Google Books) for a series presentation and let the
 * admin pick the synopsis / artwork to apply.
 */
export default function SeriesOnlineDialog({ initialQuery, onClose, onApply }: Props) {
  const { t } = useTranslation('books')
  const [query, setQuery] = useState(initialQuery)
  const [results, setResults] = useState<SeriesOnlineResult[] | null>(null)
  const [searching, setSearching] = useState(false)
  const [downloadCover, setDownloadCover] = useState(true)
  const [selected, setSelected] = useState<number | null>(null)

  async function runSearch() {
    if (!query.trim()) return
    setSearching(true)
    setSelected(null)
    try {
      setResults(await searchOnlineSeries(query.trim()))
    } catch {
      setResults([])
    } finally {
      setSearching(false)
    }
  }

  return (
    <FloatingWindow
      title={t('books_series_online_title', { defaultValue: 'Informations de la série en ligne' })}
      icon={<Globe className="h-4 w-4" />}
      onClose={onClose}
      defaultWidth={640}
      defaultHeight={580}
      backdrop
    >
      <div className="flex h-full flex-col p-5" data-module="books">
        <div className="flex items-end gap-2">
          <Input
            label={t('books_online_query', { defaultValue: 'Recherche' })}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter') void runSearch() }}
            className="flex-1"
            autoFocus
          />
          <Button variant="primary" icon={<Search className="h-4 w-4" />} onClick={runSearch} loading={searching}>
            {t('books_online_search', { defaultValue: 'Rechercher' })}
          </Button>
        </div>

        <div className="mt-2">
          <Checkbox
            checked={downloadCover}
            onChange={setDownloadCover}
            label={t('books_series_online_download_cover', { defaultValue: "Télécharger l'illustration de la série" })}
          />
        </div>

        <div className="mt-3 min-h-0 flex-1 overflow-y-auto">
          {searching ? (
            <LoadingState label={t('books_online_searching', { defaultValue: 'Recherche…' })} />
          ) : results == null ? (
            <p className="py-10 text-center text-sm text-text-tertiary">
              {t('books_series_online_hint', { defaultValue: 'Lancez une recherche pour trouver la présentation de la série.' })}
            </p>
          ) : results.length === 0 ? (
            <EmptyState icon={<Globe className="h-10 w-10" />} message={t('books_online_no_results', { defaultValue: 'Aucun résultat.' })} />
          ) : (
            <ul className="flex flex-col gap-2">
              {results.map((r, i) => (
                <li key={i}>
                  <button
                    type="button"
                    onClick={() => setSelected(i)}
                    className={`flex w-full items-start gap-3 rounded-lg border p-2.5 text-left transition ${
                      selected === i ? 'border-primary bg-primary-light' : 'border-border bg-surface-0 hover:border-border-strong'
                    }`}
                  >
                    <div className="flex h-24 w-16 flex-shrink-0 items-center justify-center overflow-hidden rounded bg-surface-2">
                      {r.cover_url ? (
                        <img src={r.cover_url} alt={r.title} className="h-full w-full object-cover" loading="lazy" />
                      ) : (
                        <LibraryIcon className="h-6 w-6 text-text-tertiary" />
                      )}
                    </div>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-sm font-medium text-text-primary">{r.title}</p>
                      {r.description && (
                        <p className="mt-0.5 line-clamp-3 text-xs text-text-secondary">{r.description}</p>
                      )}
                      <span className="mt-1 inline-block rounded bg-surface-2 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-text-tertiary">
                        {r.source}
                      </span>
                    </div>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="mt-3 flex justify-end gap-2 border-t border-border pt-3">
          <Button variant="ghost" onClick={onClose}>{t('common_cancel', { defaultValue: 'Annuler' })}</Button>
          <Button
            variant="primary"
            disabled={selected == null}
            onClick={() => { if (selected != null && results) onApply(results[selected], downloadCover) }}
          >
            {t('books_online_apply', { defaultValue: 'Appliquer' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
