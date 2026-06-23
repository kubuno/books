import { api } from '@kubuno/sdk'

// ── Types ─────────────────────────────────────────────────────────────────────
// Mirrors the backend contract under the `/books/*` proxy prefix.

export type LibraryType = 'comics' | 'books' | 'ebooks'
export type ScanStatus = 'idle' | 'scanning' | 'error'
export type SourceType = 'files_folder' | 'remote_mount' | 'filesystem'

export type ScanInterval =
  | 'disabled'
  | 'hourly'
  | 'every_6h'
  | 'every_12h'
  | 'daily'
  | 'weekly'

export type SeriesCoverChoice = 'first' | 'last'

/** Default per-book reading direction; '' means "Automatic" (let the reader decide). */
export type ReadingDirectionDefault = '' | 'ltr' | 'rtl' | 'vertical' | 'webtoon'

/**
 * Library settings stored in the `settings` JSONB column. Every field is
 * functional on the backend: the scanner honours the scan/file-type options and
 * the metadata importers honour the ComicInfo/EPUB toggles.
 */
export interface LibrarySettings {
  scanner: {
    scan_on_startup: boolean
    scan_interval: ScanInterval
    oneshots_dir: string
    scan_comics: boolean
    scan_pdf: boolean
    scan_epub: boolean
    excluded_dirs: string[]
  }
  options: {
    series_cover: SeriesCoverChoice
    /** 0-based index of the page used as the cover. */
    cover_page: number
    /** Generated thumbnail width, in pixels. */
    thumbnail_width: number
    analyze_dimensions: boolean
    hash_files: boolean
    default_reading_direction: ReadingDirectionDefault
  }
  metadata: {
    import_comicinfo: boolean
    comicinfo_book: boolean
    comicinfo_series: boolean
    comicinfo_volume_in_title: boolean
    import_epub: boolean
    epub_book: boolean
    epub_series: boolean
    /** Metadata language code, e.g. 'fr' ('' = unset). */
    metadata_language: string
  }
}

/** Default library settings — used to seed new libraries and to backfill
 * missing fields when merging server-provided settings. */
export const DEFAULT_LIBRARY_SETTINGS: LibrarySettings = {
  scanner: {
    scan_on_startup: false,
    scan_interval: 'every_6h',
    oneshots_dir: '',
    scan_comics: true,
    scan_pdf: true,
    scan_epub: true,
    excluded_dirs: ['#recycle', '@eaDir', '@Recycle'],
  },
  options: {
    series_cover: 'first',
    cover_page: 0,
    thumbnail_width: 480,
    analyze_dimensions: true,
    hash_files: true,
    default_reading_direction: '',
  },
  metadata: {
    import_comicinfo: true,
    comicinfo_book: true,
    comicinfo_series: true,
    comicinfo_volume_in_title: true,
    import_epub: true,
    epub_book: true,
    epub_series: true,
    metadata_language: '',
  },
}

/** Merges server-provided (possibly partial) settings over the defaults so the
 * UI always works with a fully-populated, section-merged settings object. */
export function withDefaultSettings(s?: Partial<LibrarySettings> | null): LibrarySettings {
  return {
    scanner: { ...DEFAULT_LIBRARY_SETTINGS.scanner, ...(s?.scanner ?? {}) },
    options: { ...DEFAULT_LIBRARY_SETTINGS.options, ...(s?.options ?? {}) },
    metadata: { ...DEFAULT_LIBRARY_SETTINGS.metadata, ...(s?.metadata ?? {}) },
  }
}

export interface Library {
  id: string
  name: string
  lib_type: LibraryType
  icon: string | null
  color: string | null
  is_shared: boolean
  item_count: number
  scan_status: ScanStatus
  scan_error: string | null
  source_type: SourceType
  files_folder_id: string | null
  last_scan_at: string | null
  created_at: string
  settings: Partial<LibrarySettings> | null
}

/** A Drive folder candidate when creating a library (admin only). */
export interface FilesFolder {
  id: string
  owner_id: string
  path: string
  name: string
  owner_email: string
  owner_display_name: string | null
}

export interface Series {
  id: string
  library_id: string
  name: string
  sort_name: string
  folder_path: string | null
  description: string | null
  publisher: string | null
  genres: string[]
  tags: string[]
  book_count: number
  total_book_count: number
  cover_format_id: string | null
  language: string | null
  reading_direction: string | null
  created_at: string
}

/** Lightweight book entry used in grids/lists. */
export interface BookListItem {
  id: string
  library_id: string
  series_id: string | null
  title: string
  sort_title: string
  series_index: number | null
  page_count: number | null
  cover_format_id: string | null
  added_at: string
  /** Available format codes, e.g. ['cbz'] or ['epub', 'pdf']. */
  formats: string[]
}

/** A concrete file backing a book (one per available format). */
export interface BookFormat {
  id: string
  book_id: string
  format: string
  file_id: string
  file_name: string
  storage_path: string
  size_bytes: number
  page_count: number | null
  added_at: string
}

/** Full book detail (list item + extended metadata). */
export interface Book extends BookListItem {
  description: string | null
  publisher: string | null
  published_date: string | null
  isbn: string | null
  identifiers: Record<string, string> | null
  language: string | null
  rating: number | null
  age_rating: string | null
  authors: string[]
  tags: string[]
  metadata: Record<string, unknown> | null
  folder_id: string | null
  file_modified_at: string | null
}

// ── DTOs ──────────────────────────────────────────────────────────────────────

export interface CreateLibraryDto {
  name: string
  lib_type: LibraryType
  source_type: SourceType
  files_folder_id?: string
  remote_mount_id?: string
  remote_mount_path?: string
  settings?: LibrarySettings
}

export interface UpdateLibraryDto {
  name?: string
  icon?: string
  color?: string
  is_shared?: boolean
  settings?: LibrarySettings
}

// ── API client (thin wrapper over the SDK axios instance) ─────────────────────
// `api` is pre-configured with base URL `/api/v1/`; module routes are proxied by
// the host under `/books/*`.

export async function listLibraries(): Promise<Library[]> {
  const { data } = await api.get<{ libraries: Library[] }>('/books/libraries')
  return data.libraries
}

export async function getFilesFolders(): Promise<FilesFolder[]> {
  const { data } = await api.get<{ folders: FilesFolder[] }>('/books/libraries/files-folders')
  return data.folders
}

export async function createLibrary(dto: CreateLibraryDto): Promise<Library> {
  const { data } = await api.post<Library>('/books/libraries', dto)
  return data
}

export async function updateLibrary(id: string, dto: UpdateLibraryDto): Promise<Library> {
  const { data } = await api.patch<Library>(`/books/libraries/${id}`, dto)
  return data
}

export async function deleteLibrary(id: string): Promise<void> {
  await api.delete(`/books/libraries/${id}`)
}

export async function scanLibrary(id: string): Promise<{ message: string; library_id: string }> {
  const { data } = await api.post<{ message: string; library_id: string }>(
    `/books/libraries/${id}/scan`,
  )
  return data
}

export async function getScanStatus(id: string): Promise<{ status: ScanStatus }> {
  const { data } = await api.get<{ status: ScanStatus }>(`/books/libraries/${id}/scan/status`)
  return data
}

export async function listSeries(libraryId: string): Promise<Series[]> {
  const { data } = await api.get<{ series: Series[] }>('/books/series', {
    params: { library_id: libraryId },
  })
  return data.series
}

/** A library reference (id + name) embedded in series/book detail responses. */
export interface LibraryRef {
  id: string
  name: string
}

/** A series reference (id + name) embedded in book detail responses. */
export interface SeriesRef {
  id: string
  name: string
}

export async function getSeries(
  id: string,
): Promise<{ series: Series; library: LibraryRef }> {
  const { data } = await api.get<{ series: Series; library: LibraryRef }>(
    `/books/series/${id}`,
  )
  return data
}

export async function getSeriesBooks(id: string): Promise<BookListItem[]> {
  const { data } = await api.get<{ books: BookListItem[] }>(`/books/series/${id}/books`)
  return data.books
}

export interface ListBooksParams {
  library_id?: string
  series_id?: string
  search?: string
  limit?: number
  offset?: number
  // P6 facet filters + sort.
  tag?: string
  author?: string
  publisher?: string
  language?: string
  format?: string
  sort?: string
}

export async function listBooks(params: ListBooksParams = {}): Promise<BookListItem[]> {
  const { data } = await api.get<{ books: BookListItem[] }>('/books/books', { params })
  return data.books
}

export async function recentBooks(limit = 12): Promise<BookListItem[]> {
  const { data } = await api.get<{ books: BookListItem[] }>('/books/books/recent', {
    params: { limit },
  })
  return data.books
}

export async function getBook(id: string): Promise<{
  book: Book
  formats: BookFormat[]
  library: LibraryRef
  series: SeriesRef | null
}> {
  const { data } = await api.get<{
    book: Book
    formats: BookFormat[]
    library: LibraryRef
    series: SeriesRef | null
  }>(`/books/books/${id}`)
  return data
}

// ── Cover image URLs ──────────────────────────────────────────────────────────
// Authenticated by the core via cookie (loaded directly in <img src>, no token).
// Books without a CBZ (epub/pdf only) return 404/415; callers fall back to a
// placeholder via the <img onError> handler.

/** Direct URL to a book's cover image. */
export function bookCoverUrl(bookId: string): string {
  return `/api/v1/books/books/${bookId}/cover`
}

/** Direct URL to a series' cover image. */
export function seriesCoverUrl(seriesId: string): string {
  return `/api/v1/books/series/${seriesId}/cover`
}

// ── Reading progress & reader assets ──────────────────────────────────────────
// NB: book entity routes use the double prefix `/books/books/...` (module_id +
// entity). Image/raw URLs below are loaded directly in <img>/fetch — the core
// authenticates them via cookie, so no Authorization header is needed.

/** Per-user reading progress for a book. */
export interface Progress {
  page: number
  location: string | null
  completed: boolean
}

export interface PutProgressDto {
  page?: number
  location?: string
  completed?: boolean
}

export async function getProgress(bookId: string): Promise<Progress> {
  const { data } = await api.get<Progress>(`/books/books/${bookId}/progress`)
  return data
}

export async function putProgress(bookId: string, dto: PutProgressDto): Promise<Progress> {
  const { data } = await api.put<Progress>(`/books/books/${bookId}/progress`, dto)
  return data
}

export async function markRead(bookId: string): Promise<void> {
  await api.post(`/books/books/${bookId}/read`)
}

export async function markUnread(bookId: string): Promise<void> {
  await api.post(`/books/books/${bookId}/unread`)
}

/** An item in the "keep reading" rail (book + its reading progress). */
export interface KeepItem {
  id: string
  library_id: string
  series_id: string | null
  title: string
  series_index: number | null
  page_count: number | null
  cover_format_id: string | null
  formats: string[]
  progress_page: number
  progress_updated: string
}

export async function keepReading(limit?: number): Promise<KeepItem[]> {
  const { data } = await api.get<{ books: KeepItem[] }>('/books/books/keep-reading', {
    params: limit != null ? { limit } : undefined,
  })
  return data.books
}

/** Returns the page count, triggering server-side indexing of image archives. */
export async function getPageCount(bookId: string): Promise<number> {
  const { data } = await api.get<{ page_count: number }>(`/books/books/${bookId}/pages`)
  return data.page_count
}

/** Direct URL to a rendered archive page image (0-based index). */
export function pageImageUrl(bookId: string, n: number): string {
  return `/api/v1/books/books/${bookId}/pages/${n}`
}

/** Direct URL to a format's raw file (used by the PDF / EPUB readers). */
export function formatRawUrl(formatId: string): string {
  return `/api/v1/books/formats/${formatId}/raw`
}

// ── P4: metadata editing ──────────────────────────────────────────────────────
// Book/series entity routes use the double prefix `/books/books/...` /
// `/books/series/...`. Bulk + facets + collections live under the single prefix.

/** An author entry (name + optional role, e.g. "writer", "artist"). */
export interface AuthorEntry {
  name: string
  role?: string
}

/** Editable book fields (all optional — partial PATCH). */
export interface UpdateBookDto {
  title?: string
  sort_title?: string
  series_index?: number | null
  description?: string | null
  publisher?: string | null
  published_date?: string | null
  isbn?: string | null
  language?: string | null
  rating?: number | null
  age_rating?: number | null
  reading_direction?: 'ltr' | 'rtl' | 'vertical' | 'webtoon' | null
  authors?: AuthorEntry[]
  tags?: string[]
  identifiers?: Record<string, string>
}

export async function updateBook(id: string, fields: UpdateBookDto): Promise<Book> {
  const { data } = await api.patch<Book>(`/books/books/${id}`, fields)
  return data
}

/** Editable series fields (all optional — partial PATCH). */
export interface UpdateSeriesDto {
  name?: string
  sort_name?: string
  description?: string | null
  publisher?: string | null
  language?: string | null
  age_rating?: number | null
  reading_direction?: 'ltr' | 'rtl' | 'vertical' | 'webtoon' | null
  total_book_count?: number | null
  genres?: string[]
  tags?: string[]
}

export async function updateSeries(id: string, fields: UpdateSeriesDto): Promise<Series> {
  const { data } = await api.patch<Series>(`/books/series/${id}`, fields)
  return data
}

/** Fields applicable to a bulk update of several books at once. */
export interface BulkUpdateBooksDto {
  ids: string[]
  tags?: string[]
  publisher?: string | null
  language?: string | null
  reading_direction?: 'ltr' | 'rtl' | 'vertical' | 'webtoon' | null
  age_rating?: number | null
}

export async function bulkUpdateBooks(dto: BulkUpdateBooksDto): Promise<{ updated: number }> {
  const { data } = await api.patch<{ updated: number }>('/books/books/bulk', dto)
  return data
}

/** Re-import a single book's metadata from its source file. */
export async function refreshBookMetadata(id: string): Promise<void> {
  await api.post(`/books/books/${id}/refresh-metadata`)
}

/** Re-import metadata for every book in a library. */
export async function refreshLibraryMetadata(id: string): Promise<void> {
  await api.post(`/books/libraries/${id}/refresh-metadata`)
}

// ── P5: online metadata lookup ────────────────────────────────────────────────

/** A single match from an online metadata provider (OpenLibrary, Google…). */
export interface OnlineMetadataResult {
  source: string
  title: string
  authors: string[]
  publisher: string | null
  date: string | null
  isbn: string | null
  description: string | null
  language: string | null
  tags: string[]
  cover_url: string | null
}

export async function searchOnlineMetadata(q: string): Promise<OnlineMetadataResult[]> {
  const { data } = await api.get<{ results: OnlineMetadataResult[] }>('/books/metadata/search', {
    params: { q },
  })
  return data.results
}

/** Payload to apply an online match to a book (optionally downloading the cover). */
export interface ApplyMetadataDto {
  title?: string
  authors?: string[]
  publisher?: string | null
  published_date?: string | null
  isbn?: string | null
  description?: string | null
  language?: string | null
  tags?: string[]
  cover_url?: string | null
  download_cover?: boolean
}

export async function applyOnlineMetadata(id: string, payload: ApplyMetadataDto): Promise<Book> {
  const { data } = await api.post<Book>(`/books/books/${id}/apply-metadata`, payload)
  return data
}

// ── Series online presentation (Wikipedia / Google Books) ─────────────────────

/** A candidate online presentation for a series. */
export interface SeriesOnlineResult {
  source: string
  title: string
  description: string | null
  publisher: string | null
  authors: string[]
  genres: string[]
  cover_url: string | null
}

export async function searchOnlineSeries(q: string): Promise<SeriesOnlineResult[]> {
  const { data } = await api.get<{ results: SeriesOnlineResult[] }>('/books/metadata/series-search', {
    params: { q },
  })
  return data.results
}

export interface ApplySeriesMetadataDto {
  description?: string | null
  publisher?: string | null
  genres?: string[]
  cover_url?: string | null
  download_cover?: boolean
}

export async function applyOnlineSeriesMetadata(id: string, payload: ApplySeriesMetadataDto): Promise<void> {
  await api.post(`/books/series/${id}/apply-metadata`, payload)
}

/** Auto-enrich one series from the web (fills empty fields). Returns whether anything changed. */
export async function refreshSeriesMetadata(id: string): Promise<boolean> {
  const { data } = await api.post<{ applied: boolean }>(`/books/series/${id}/refresh-metadata`)
  return data.applied
}

/** Auto-enrich every series of a library from the web (background). */
export async function refreshLibrarySeriesMetadata(id: string): Promise<void> {
  await api.post(`/books/libraries/${id}/refresh-series-metadata`)
}

// ── P6: collections, read lists, saved searches, facets ───────────────────────

export interface Collection {
  id: string
  name: string
  description: string | null
  is_public: boolean
  series_count: number
}

export async function listCollections(): Promise<Collection[]> {
  const { data } = await api.get<{ collections: Collection[] }>('/books/collections')
  return data.collections
}

export interface CreateCollectionDto {
  name: string
  description?: string
  is_public?: boolean
}

export async function createCollection(dto: CreateCollectionDto): Promise<Collection> {
  const { data } = await api.post<Collection>('/books/collections', dto)
  return data
}

export async function getCollection(
  id: string,
): Promise<{ collection: Collection; series: Series[] }> {
  const { data } = await api.get<{ collection: Collection; series: Series[] }>(
    `/books/collections/${id}`,
  )
  return data
}

export async function updateCollection(
  id: string,
  dto: Partial<CreateCollectionDto>,
): Promise<Collection> {
  const { data } = await api.patch<Collection>(`/books/collections/${id}`, dto)
  return data
}

export async function deleteCollection(id: string): Promise<void> {
  await api.delete(`/books/collections/${id}`)
}

export async function addSeriesToCollection(id: string, seriesId: string): Promise<void> {
  await api.post(`/books/collections/${id}/series`, { series_id: seriesId })
}

export async function removeSeriesFromCollection(id: string, seriesId: string): Promise<void> {
  await api.delete(`/books/collections/${id}/series/${seriesId}`)
}

export interface ReadList {
  id: string
  name: string
  description: string | null
  is_public: boolean
  book_count: number
}

export async function listReadLists(): Promise<ReadList[]> {
  const { data } = await api.get<{ read_lists: ReadList[] }>('/books/readlists')
  return data.read_lists
}

export interface CreateReadListDto {
  name: string
  description?: string
  is_public?: boolean
}

export async function createReadList(dto: CreateReadListDto): Promise<ReadList> {
  const { data } = await api.post<ReadList>('/books/readlists', dto)
  return data
}

export async function getReadList(
  id: string,
): Promise<{ read_list: ReadList; books: BookListItem[] }> {
  const { data } = await api.get<{ read_list: ReadList; books: BookListItem[] }>(
    `/books/readlists/${id}`,
  )
  return data
}

export async function deleteReadList(id: string): Promise<void> {
  await api.delete(`/books/readlists/${id}`)
}

export async function addBookToReadList(id: string, bookId: string): Promise<void> {
  await api.post(`/books/readlists/${id}/books`, { book_id: bookId })
}

export async function removeBookFromReadList(id: string, bookId: string): Promise<void> {
  await api.delete(`/books/readlists/${id}/books/${bookId}`)
}

export interface SavedSearch {
  id: string
  name: string
  filters: Record<string, string>
}

export async function listSavedSearches(): Promise<SavedSearch[]> {
  const { data } = await api.get<{ saved_searches: SavedSearch[] }>('/books/saved-searches')
  return data.saved_searches
}

export async function createSavedSearch(dto: {
  name: string
  filters: Record<string, string>
}): Promise<SavedSearch> {
  const { data } = await api.post<SavedSearch>('/books/saved-searches', dto)
  return data
}

export async function deleteSavedSearch(id: string): Promise<void> {
  await api.delete(`/books/saved-searches/${id}`)
}

/** A facet value with its occurrence count. */
export interface FacetValue {
  value: string
  count: number
}

export interface Facets {
  tags: FacetValue[]
  authors: FacetValue[]
  publishers: FacetValue[]
  languages: FacetValue[]
}

export async function getFacets(): Promise<Facets> {
  const { data } = await api.get<Facets>('/books/facets')
  return data
}

// ── P7: downloads, duplicates, OPDS ───────────────────────────────────────────

/** Direct URL to download a book's primary file (authenticated via cookie). */
export function bookDownloadUrl(bookId: string): string {
  return `/api/v1/books/books/${bookId}/download`
}

/** URL to the OPDS catalog feed (authenticated via cookie). */
export const OPDS_URL = '/api/v1/books/opds'

/** A group of books sharing the same content hash (potential duplicates). */
export interface DuplicateGroup {
  hash: string
  books: { id: string; title: string }[]
}

export async function getDuplicates(): Promise<DuplicateGroup[]> {
  const { data } = await api.get<{ duplicates: DuplicateGroup[] }>('/books/duplicates')
  return data.duplicates
}
