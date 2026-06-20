import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { ExternalLink, Pencil, FolderPlus } from 'lucide-react'
import type { MenuItem } from '@ui'

/** Minimal series shape needed to build a context menu. */
interface MenuSeries {
  id: string
  name: string
}

interface SeriesMenuOptions {
  /** Admin-only "Edit series" handler; omit to hide the entry. */
  onEdit?: (series: MenuSeries) => void
  /** "Add to a collection" handler; omit to hide the entry. */
  onAddToCollection?: (series: MenuSeries) => void
}

/**
 * Builds the right-click context menu for a series cover card. Always exposes
 * Open; admin "Edit series" and "Add to a collection" are added when the
 * matching handlers are provided.
 */
export function useSeriesContextMenu(opts: SeriesMenuOptions = {}) {
  const navigate = useNavigate()
  const { t } = useTranslation('books')

  return (series: MenuSeries): MenuItem[] => {
    const items: MenuItem[] = [
      {
        type: 'action',
        label: t('books_ctx_open', { defaultValue: 'Ouvrir' }),
        icon: <ExternalLink className="h-4 w-4" />,
        onClick: () => navigate(`/books/series/${series.id}`),
      },
    ]

    if (opts.onAddToCollection) {
      items.push({
        type: 'action',
        label: t('books_ctx_add_to_collection', { defaultValue: 'Ajouter à une collection' }),
        icon: <FolderPlus className="h-4 w-4" />,
        onClick: () => opts.onAddToCollection?.(series),
      })
    }

    if (opts.onEdit) {
      items.push({ type: 'separator' })
      items.push({
        type: 'action',
        label: t('books_ctx_edit_series', { defaultValue: 'Éditer la série' }),
        icon: <Pencil className="h-4 w-4" />,
        onClick: () => opts.onEdit?.(series),
      })
    }

    return items
  }
}
