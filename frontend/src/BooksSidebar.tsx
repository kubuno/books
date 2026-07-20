import { useLocation } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { Library, Clock, Compass, FolderHeart, ListChecks, CopyCheck } from 'lucide-react'
import { SidebarNavItem, useAuthStore } from '@kubuno/sdk'

function SectionLabel({ label, collapsed }: { label: string; collapsed?: boolean }) {
  if (collapsed) return <div className="mx-2 my-2 h-px bg-border" />
  return (
    <div className="px-3 pt-4 pb-1">
      <span className="text-[11px] font-semibold uppercase tracking-wider text-text-tertiary">
        {label}
      </span>
    </div>
  )
}

interface NavEntry {
  labelKey: string
  icon: React.ReactNode
  path: string
  /** When true, the entry is only shown to admins. */
  adminOnly?: boolean
}

const COLLECTION_ITEMS: NavEntry[] = [
  { labelKey: 'books_nav_library', icon: <Library className="w-4 h-4 flex-shrink-0" />, path: '/books' },
  { labelKey: 'books_nav_browse',  icon: <Compass className="w-4 h-4 flex-shrink-0" />, path: '/books/browse' },
  { labelKey: 'books_nav_recent',  icon: <Clock className="w-4 h-4 flex-shrink-0" />,   path: '/books/recent' },
]

const ORGANIZE_ITEMS: NavEntry[] = [
  { labelKey: 'books_nav_collections', icon: <FolderHeart className="w-4 h-4 flex-shrink-0" />, path: '/books/collections' },
  { labelKey: 'books_nav_readlists',   icon: <ListChecks className="w-4 h-4 flex-shrink-0" />,  path: '/books/readlists' },
  { labelKey: 'books_nav_duplicates',  icon: <CopyCheck className="w-4 h-4 flex-shrink-0" />,   path: '/books/duplicates', adminOnly: true },
]

export default function BooksSidebar({ collapsed = false }: { collapsed?: boolean }) {
  const { pathname } = useLocation()
  const { t }        = useTranslation('books')
  const user         = useAuthStore((s) => s.user)
  const isAdmin      = user?.role === 'admin'

  const isActive = (path: string) => {
    // "Library" owns the whole collection tree except the dedicated views below.
    if (path === '/books') return pathname === '/books' || pathname.startsWith('/books/library')
        || pathname.startsWith('/books/series') || pathname.startsWith('/books/book')
    if (path === '/books/collections')
      return pathname === path || pathname.startsWith('/books/collection')
    if (path === '/books/readlists')
      return pathname === path || pathname.startsWith('/books/readlist')
    return pathname === path || pathname.startsWith(path + '/')
  }

  const renderItems = (items: NavEntry[]) =>
    items
      .filter((it) => !it.adminOnly || isAdmin)
      .map(({ labelKey, icon, path }) => (
        // `to` makes the item a real <a href> (React Router <Link>), never a <button>.
        <SidebarNavItem collapsed={collapsed} key={path} label={t(labelKey)} icon={icon}
          active={isActive(path)} to={path} />
      ))

  return (
    <nav className={`flex-1 overflow-y-auto py-1 space-y-0.5 ${collapsed ? "px-2" : "px-3"}`}>
      <SectionLabel collapsed={collapsed} label={t('books_section_collection')} />
      {renderItems(COLLECTION_ITEMS)}
      <SectionLabel collapsed={collapsed} label={t('books_section_organize')} />
      {renderItems(ORGANIZE_ITEMS)}
    </nav>
  )
}
