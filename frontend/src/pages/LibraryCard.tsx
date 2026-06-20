import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { MenuDropdown, useMenuDropdown } from '@ui'
import { MoreVertical, Library as LibraryIcon, BookOpen, BookMarked, RefreshCw, Pencil, Trash2 } from 'lucide-react'
import { useConfirm } from '@kubuno/sdk'
import type { MenuItem } from '@ui'
import LibrarySettingsDialog from '../components/LibrarySettingsDialog'
import {
  scanLibrary,
  deleteLibrary,
  getScanStatus,
  type Library,
  type LibraryType,
} from '../api'
import ScanStatusPill from '../components/ScanStatusPill'

const TYPE_ICON: Record<LibraryType, typeof LibraryIcon> = {
  comics: BookMarked,
  books: BookOpen,
  ebooks: LibraryIcon,
}

interface Props {
  library: Library
  isAdmin: boolean
  onConfirm: ReturnType<typeof useConfirm>['confirm']
}

/** One library tile on the home page, with an admin context menu. */
export default function LibraryCard({ library, isAdmin, onConfirm }: Props) {
  const { t } = useTranslation('books')
  const qc = useQueryClient()
  const menu = useMenuDropdown()
  const [busy, setBusy] = useState(false)
  const [editing, setEditing] = useState(false)

  const Icon = TYPE_ICON[library.lib_type] ?? LibraryIcon

  // Poll scan status while scanning to flip the pill back to idle/error.
  const { data: liveStatus } = useQuery({
    queryKey: ['books', 'scan-status', library.id],
    queryFn: () => getScanStatus(library.id),
    enabled: library.scan_status === 'scanning',
    refetchInterval: (q) => (q.state.data?.status === 'scanning' ? 1500 : false),
  })

  const status =
    library.scan_status === 'scanning' && liveStatus ? liveStatus.status : library.scan_status

  // When a scan finishes, refresh the library list (item counts may change).
  if (library.scan_status === 'scanning' && liveStatus && liveStatus.status !== 'scanning') {
    void qc.invalidateQueries({ queryKey: ['books', 'libraries'] })
  }

  async function rescan() {
    setBusy(true)
    try {
      await scanLibrary(library.id)
      await qc.invalidateQueries({ queryKey: ['books', 'libraries'] })
    } finally {
      setBusy(false)
    }
  }

  async function remove() {
    const ok = await onConfirm({
      title: t('books_delete_title'),
      message: t('books_delete_message', { name: library.name }),
      confirmLabel: t('common_delete'),
      variant: 'danger',
    })
    if (!ok) return
    setBusy(true)
    try {
      await deleteLibrary(library.id)
      await qc.invalidateQueries({ queryKey: ['books', 'libraries'] })
    } finally {
      setBusy(false)
    }
  }

  const menuItems: MenuItem[] = [
    {
      type: 'action',
      label: t('books_action_edit', { defaultValue: 'Modifier' }),
      icon: <Pencil className="h-4 w-4" />,
      onClick: () => {
        menu.close()
        setEditing(true)
      },
    },
    {
      type: 'action',
      label: t('books_action_rescan'),
      icon: <RefreshCw className="h-4 w-4" />,
      onClick: () => {
        menu.close()
        void rescan()
      },
    },
    { type: 'separator' },
    {
      type: 'action',
      label: t('common_delete'),
      icon: <Trash2 className="h-4 w-4" />,
      danger: true,
      onClick: () => {
        menu.close()
        void remove()
      },
    },
  ]

  return (
    <div
      className="relative flex items-center gap-3 rounded-lg border border-border bg-surface-0 px-4 py-3 transition hover:border-border-strong hover:shadow-sm"
      onContextMenu={
        isAdmin
          ? (e) => {
              e.preventDefault()
              menu.open(e)
            }
          : undefined
      }
    >
      <Link
        to={`/books/library/${library.id}`}
        className="flex min-w-0 flex-1 items-center gap-3 focus:outline-none"
      >
        <div className="flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-lg bg-primary-light">
          <Icon className="h-5 w-5 text-primary" />
        </div>
        <div className="min-w-0">
          <p className="truncate font-medium text-text-primary" title={library.name}>
            {library.name}
          </p>
          <div className="mt-0.5 flex items-center gap-2">
            <span className="text-xs text-text-tertiary">
              {t('books_item_count', { count: library.item_count })}
            </span>
            <ScanStatusPill status={status} />
          </div>
        </div>
      </Link>

      {isAdmin && (
        <button
          type="button"
          disabled={busy}
          onClick={menu.open}
          className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full text-text-tertiary hover:bg-surface-2 hover:text-text-primary disabled:opacity-50"
          aria-label={t('books_actions')}
        >
          <MoreVertical className="h-4 w-4" />
        </button>
      )}

      {menu.isOpen && menu.pos && (
        <MenuDropdown items={menuItems} pos={menu.pos} onClose={menu.close} />
      )}

      {editing && (
        <LibrarySettingsDialog
          mode="edit"
          library={library}
          onClose={() => setEditing(false)}
        />
      )}
    </div>
  )
}
