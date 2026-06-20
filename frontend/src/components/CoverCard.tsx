import { useState } from 'react'
import { Link } from 'react-router-dom'
import { BookOpen } from 'lucide-react'
import { MenuDropdown, useMenuDropdown, type MenuItem } from '@ui'
import FormatBadge from './FormatBadge'

/**
 * Cover thumbnail used across grids (books & series).
 *
 * When `coverUrl` is set, the real cover image is rendered (authenticated by the
 * core via cookie, like Drive thumbnails). Books without a CBZ (epub/pdf only)
 * return 404/415, so on image error we fall back to the colored placeholder
 * block + BookOpen icon.
 */
export interface CoverCardProps {
  to: string
  title: string
  subtitle?: string | null
  /** When set, renders the actual cover image (falls back to placeholder on error). */
  coverUrl?: string | null
  /** Format codes to show as badges (books only). */
  formats?: string[]
  /** Small badge in the top-right corner (e.g. book count for a series). */
  count?: number | null
  /** Reading progress in [0,1]; renders a bottom progress bar when set. */
  progress?: number | null
  /** Right-click context menu items (opened at the cursor position when set). */
  contextMenu?: MenuItem[]
}

// Deterministic placeholder color from the title so cards look varied yet stable.
const PALETTE = [
  '#1a73e8',
  '#1e8e3e',
  '#d93025',
  '#b06000',
  '#8430ce',
  '#0b8043',
  '#c5221f',
  '#1967d2',
]

function colorFor(seed: string): string {
  let hash = 0
  for (let i = 0; i < seed.length; i++) hash = (hash * 31 + seed.charCodeAt(i)) >>> 0
  return PALETTE[hash % PALETTE.length]
}

export default function CoverCard({
  to,
  title,
  subtitle,
  coverUrl,
  formats,
  count,
  progress,
  contextMenu,
}: CoverCardProps) {
  const bg = colorFor(title)
  // Falls back to the placeholder when the cover image fails to load.
  const [imgFailed, setImgFailed] = useState(false)
  const showImage = !!coverUrl && !imgFailed
  const menu = useMenuDropdown()

  return (
    <>
    <Link
      to={to}
      onContextMenu={
        contextMenu && contextMenu.length > 0
          ? (e) => {
              e.preventDefault()
              menu.open(e)
            }
          : undefined
      }
      className="group flex flex-col rounded-lg border border-border bg-surface-0 overflow-hidden transition hover:shadow-md hover:border-border-strong focus:outline-none focus:ring-2 focus:ring-primary/40"
    >
      <div
        className="relative aspect-[2/3] w-full flex items-center justify-center"
        style={showImage ? undefined : { backgroundColor: bg }}
      >
        {showImage ? (
          <img
            src={coverUrl ?? undefined}
            alt={title}
            className="h-full w-full object-cover"
            loading="lazy"
            onError={() => setImgFailed(true)}
          />
        ) : (
          <BookOpen className="h-10 w-10 text-white/70" />
        )}

        {typeof count === 'number' && (
          <span className="absolute top-1.5 right-1.5 rounded-full bg-black/55 px-1.5 py-0.5 text-[10px] font-semibold text-white">
            {count}
          </span>
        )}

        {formats && formats.length > 0 && (
          <div className="absolute bottom-1.5 left-1.5 flex flex-wrap gap-1">
            {formats.slice(0, 3).map((f) => (
              <FormatBadge key={f} format={f} />
            ))}
          </div>
        )}

        {typeof progress === 'number' && (
          <div className="absolute inset-x-0 bottom-0 h-1.5 bg-black/30">
            <div
              className="h-full bg-primary"
              style={{ width: `${Math.min(Math.max(progress, 0), 1) * 100}%` }}
            />
          </div>
        )}
      </div>

      <div className="px-2.5 py-2">
        <p
          className="truncate text-sm font-medium text-text-primary group-hover:text-primary"
          title={title}
        >
          {title}
        </p>
        {subtitle && (
          <p className="truncate text-xs text-text-tertiary" title={subtitle}>
            {subtitle}
          </p>
        )}
      </div>
    </Link>
    {contextMenu && menu.isOpen && menu.pos && (
      <MenuDropdown items={contextMenu} pos={menu.pos} onClose={menu.close} />
    )}
    </>
  )
}
