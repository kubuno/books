/** MODULE bundle: books — loaded at runtime by the host (cf. vite.config). */
import { lazy } from 'react'
import {
  RouteRegistry,
  WaffleAppRegistry,
  FaviconRegistry,
  useSidebarStore,
  SDK_VERSION,
} from '@kubuno/sdk'
import { BookOpen } from 'lucide-react'
import './index.css'
import './i18n'
import BooksSidebar from './BooksSidebar'

export const sdkVersion = SDK_VERSION

export function register() {
  // App names are brands, never translated.
  WaffleAppRegistry.register('books', 'Books', [
    { id: 'books', label: 'Books', Icon: BookOpen, path: '/books' },
  ])

  useSidebarStore.getState().register({
    moduleId: 'books',
    routePrefix: '/books',
    SidebarBody: BooksSidebar,
    collapsedBody: true,
  })

  FaviconRegistry.register('books', '/books-logo.svg')

  // Routes (paths are relative to the host shell — no leading slash).
  const LibrariesPage = lazy(() => import('./pages/LibrariesPage'))
  const LibraryPage = lazy(() => import('./pages/LibraryPage'))
  const SeriesPage = lazy(() => import('./pages/SeriesPage'))
  const BookPage = lazy(() => import('./pages/BookPage'))
  const RecentPage = lazy(() => import('./pages/RecentPage'))
  const ReaderView = lazy(() => import('./pages/ReaderView'))
  const BrowsePage = lazy(() => import('./pages/BrowsePage'))
  const CollectionsPage = lazy(() => import('./pages/CollectionsPage'))
  const CollectionPage = lazy(() => import('./pages/CollectionPage'))
  const ReadListsPage = lazy(() => import('./pages/ReadListsPage'))
  const ReadListPage = lazy(() => import('./pages/ReadListPage'))
  const DuplicatesPage = lazy(() => import('./pages/DuplicatesPage'))

  RouteRegistry.register('books', LibrariesPage)
  RouteRegistry.register('books/library/:id', LibraryPage)
  RouteRegistry.register('books/series/:id', SeriesPage)
  RouteRegistry.register('books/book/:id', BookPage)
  RouteRegistry.register('books/recent', RecentPage)
  RouteRegistry.register('books/read/:id', ReaderView)
  RouteRegistry.register('books/browse', BrowsePage)
  RouteRegistry.register('books/collections', CollectionsPage)
  RouteRegistry.register('books/collection/:id', CollectionPage)
  RouteRegistry.register('books/readlists', ReadListsPage)
  RouteRegistry.register('books/readlist/:id', ReadListPage)
  RouteRegistry.register('books/duplicates', DuplicatesPage)
}
