import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { BookOpen, ExternalLink, Download, ListPlus, Pencil } from 'lucide-react'
import type { MenuItem } from '@ui'
import { bookDownloadUrl } from '../api'

/** Minimal book shape needed to build a context menu. */
interface MenuBook {
  id: string
  title: string
}

interface BookMenuOptions {
  /** Admin-only "Edit metadata" handler; omit to hide the entry. */
  onEdit?: (book: MenuBook) => void
  /** "Add to a list" handler; omit to hide the entry. */
  onAddToList?: (book: MenuBook) => void
}

/**
 * Builds the right-click context menu for a book cover card. Always exposes
 * Read / Open / Download; "Add to list" and admin "Edit metadata" are added
 * when the matching handlers are provided.
 */
export function useBookContextMenu(opts: BookMenuOptions = {}) {
  const navigate = useNavigate()
  const { t } = useTranslation('books')

  return (book: MenuBook): MenuItem[] => {
    const items: MenuItem[] = [
      {
        type: 'action',
        label: t('books_read', { defaultValue: 'Lire' }),
        icon: <BookOpen className="h-4 w-4" />,
        onClick: () => navigate(`/books/read/${book.id}`),
      },
      {
        type: 'action',
        label: t('books_ctx_open', { defaultValue: 'Ouvrir' }),
        icon: <ExternalLink className="h-4 w-4" />,
        onClick: () => navigate(`/books/book/${book.id}`),
      },
      {
        type: 'action',
        label: t('books_download', { defaultValue: 'Télécharger' }),
        icon: <Download className="h-4 w-4" />,
        // Trigger a download without leaving the page.
        onClick: () => {
          const a = document.createElement('a')
          a.href = bookDownloadUrl(book.id)
          a.download = ''
          document.body.appendChild(a)
          a.click()
          a.remove()
        },
      },
    ]

    if (opts.onAddToList) {
      items.push({
        type: 'action',
        label: t('books_ctx_add_to_list', { defaultValue: 'Ajouter à une liste' }),
        icon: <ListPlus className="h-4 w-4" />,
        onClick: () => opts.onAddToList?.(book),
      })
    }

    if (opts.onEdit) {
      items.push({ type: 'separator' })
      items.push({
        type: 'action',
        label: t('books_ctx_edit_metadata', { defaultValue: 'Éditer les métadonnées' }),
        icon: <Pencil className="h-4 w-4" />,
        onClick: () => opts.onEdit?.(book),
      })
    }

    return items
  }
}
