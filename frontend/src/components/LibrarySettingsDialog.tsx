import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useQueryClient } from '@tanstack/react-query'
import { FloatingWindow, Button, Input, Dropdown, ColorField, Checkbox } from '@ui'
import {
  Library as LibraryIcon,
  ScanSearch,
  SlidersHorizontal,
  FileText,
  AlertTriangle,
  Check,
  X,
} from 'lucide-react'
import { useFilesDialogStore } from '@kubuno/drive'
import {
  createLibrary,
  updateLibrary,
  scanLibrary,
  withDefaultSettings,
  DEFAULT_LIBRARY_SETTINGS,
  type Library,
  type LibraryType,
  type LibrarySettings,
} from '../api'

type Mode = 'create' | 'edit'
type TabId = 'general' | 'scanner' | 'options' | 'metadata'

interface Props {
  mode: Mode
  library?: Library
  onClose: () => void
}

// Deeply clone the settings object so per-section edits never mutate shared state.
function cloneSettings(s: LibrarySettings): LibrarySettings {
  return {
    scanner: { ...s.scanner, excluded_dirs: [...s.scanner.excluded_dirs] },
    options: { ...s.options },
    metadata: { ...s.metadata },
  }
}

// ── Small presentational helpers (kept local to the dialog) ───────────────────

/** A labelled checkbox row built on the shared @ui Checkbox primitive. */
function CheckRow({
  checked,
  onChange,
  label,
  indent,
}: {
  checked: boolean
  onChange: (v: boolean) => void
  label: string
  indent?: boolean
}) {
  return (
    <div className={`py-1 ${indent ? 'pl-6' : ''}`}>
      <Checkbox
        checked={checked}
        onChange={onChange}
        label={label}
        labelClassName="text-text-secondary"
      />
    </div>
  )
}

/** A toggle button that shows a check when active (file-type pickers). */
function ToggleChip({
  active,
  onClick,
  label,
}: {
  active: boolean
  onClick: () => void
  label: string
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex items-center gap-1.5 rounded-full border px-3 py-1.5 text-sm transition ${
        active
          ? 'border-primary bg-primary-light text-primary'
          : 'border-border bg-surface-0 text-text-secondary hover:border-border-strong'
      }`}
    >
      {active && <Check className="h-3.5 w-3.5" />}
      {label}
    </button>
  )
}

/** A tag-input field: removable chips + an input to add new entries. */
function ChipInput({
  values,
  onChange,
  placeholder,
}: {
  values: string[]
  onChange: (v: string[]) => void
  placeholder?: string
}) {
  const [draft, setDraft] = useState('')

  function add() {
    const v = draft.trim()
    if (v && !values.includes(v)) onChange([...values, v])
    setDraft('')
  }

  return (
    <div className="flex flex-wrap items-center gap-1.5 rounded-md border border-border bg-surface-0 px-2 py-1.5">
      {values.map((v) => (
        <span
          key={v}
          className="flex items-center gap-1 rounded bg-surface-2 px-2 py-0.5 text-xs text-text-primary"
        >
          {v}
          <button
            type="button"
            onClick={() => onChange(values.filter((x) => x !== v))}
            className="text-text-tertiary hover:text-danger"
            aria-label={`remove ${v}`}
          >
            <X className="h-3 w-3" />
          </button>
        </span>
      ))}
      <input
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault()
            add()
          }
        }}
        onBlur={add}
        placeholder={placeholder}
        className="min-w-[120px] flex-1 bg-transparent text-sm text-text-primary outline-none placeholder:text-text-tertiary"
      />
    </div>
  )
}

/** A bold section heading used inside the tab panels. */
function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h3 className="mb-1 mt-1 text-sm font-semibold text-text-primary">{children}</h3>
}

function FieldLabel({ children }: { children: React.ReactNode }) {
  return (
    <label className="mb-1.5 block text-sm font-medium text-text-secondary">{children}</label>
  )
}

/**
 * Multi-tab dialog used for BOTH creating and editing a library.
 * Tabs: General / Scanner / Options / Metadata. A left rail switches tabs; the
 * primary action (Add / Save) is reachable from any tab.
 */
export default function LibrarySettingsDialog({ mode, library, onClose }: Props) {
  const { t } = useTranslation('books')
  const qc = useQueryClient()

  const [tab, setTab] = useState<TabId>('general')

  // ── General fields ──
  const [name, setName] = useState(library?.name ?? '')
  const [libType, setLibType] = useState<LibraryType>(library?.lib_type ?? 'comics')
  const [icon, setIcon] = useState(library?.icon ?? '📚')
  const [color, setColor] = useState(library?.color ?? '#1a73e8')
  const [shared, setShared] = useState(library?.is_shared ?? false)
  const [folder, setFolder] = useState<{
    id: string | null
    name: string
    remoteMount?: { mountId: string; path: string }
  } | null>(null)

  // ── Settings (all tabs but General) ──
  const [settings, setSettings] = useState<LibrarySettings>(() =>
    mode === 'edit'
      ? cloneSettings(withDefaultSettings(library?.settings))
      : cloneSettings(DEFAULT_LIBRARY_SETTINGS),
  )

  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [touchedName, setTouchedName] = useState(false)

  // Helpers to update a single settings section immutably.
  const sc = settings.scanner
  const op = settings.options
  const md = settings.metadata
  function setScanner(p: Partial<LibrarySettings['scanner']>) {
    setSettings((s) => ({ ...s, scanner: { ...s.scanner, ...p } }))
  }
  function setOptions(p: Partial<LibrarySettings['options']>) {
    setSettings((s) => ({ ...s, options: { ...s.options, ...p } }))
  }
  function setMetadata(p: Partial<LibrarySettings['metadata']>) {
    setSettings((s) => ({ ...s, metadata: { ...s.metadata, ...p } }))
  }

  // Last path segment of a canonical "[storage]/a/b/c" path → "c" (for the default name).
  function lastSegment(canonical: string): string {
    const rel = canonical.replace(/^\[[^\]]*\]\/?/, '')
    const parts = rel.split('/').filter(Boolean)
    return parts.length ? parts[parts.length - 1] : canonical.replace(/[[\]]/g, '')
  }

  async function chooseFolder() {
    const sel = await useFilesDialogStore
      .getState()
      .pickFolder({ title: t('books_pick_folder_title') })
    if (!sel) return
    // sel.name is the canonical "[storage]/path" — shown verbatim in the field.
    if (sel.id) {
      setFolder({ id: sel.id, name: sel.name })
      if (!name.trim()) setName(lastSegment(sel.name))
    } else if (sel.remote) {
      setFolder({ id: null, name: sel.name, remoteMount: { mountId: sel.remote.mountId, path: sel.remote.path } })
      if (!name.trim()) setName(lastSegment(sel.name))
    }
  }

  const nameEmpty     = name.trim().length === 0
  const folderMissing = mode === 'create' && folder === null
  const canSubmit     = !nameEmpty && !folderMissing && !submitting

  async function submit() {
    if (!canSubmit) return
    setSubmitting(true)
    setError(null)
    try {
      if (mode === 'create') {
        if (!folder) return
        const lib = await createLibrary(
          folder.remoteMount
            ? {
                name: name.trim(),
                lib_type: libType,
                source_type: 'remote_mount',
                remote_mount_id: folder.remoteMount.mountId,
                remote_mount_path: folder.remoteMount.path,
                settings,
              }
            : {
                name: name.trim(),
                lib_type: libType,
                source_type: 'files_folder',
                files_folder_id: folder.id!,
                settings,
              },
        )
        // Kick off the initial scan (fire and forget; cards poll status).
        try {
          await scanLibrary(lib.id)
        } catch {
          /* scan failures surface via scan_status on the library card */
        }
      } else if (library) {
        await updateLibrary(library.id, {
          name: name.trim(),
          icon,
          color,
          is_shared: shared,
          settings,
        })
      }
      await qc.invalidateQueries({ queryKey: ['books', 'libraries'] })
      onClose()
    } catch {
      setError(
        mode === 'create'
          ? t('books_create_error')
          : t('books_edit_library_error', { defaultValue: 'Échec de la modification.' }),
      )
      setSubmitting(false)
    }
  }

  const tabs: { id: TabId; label: string; icon: React.ReactNode }[] = [
    {
      id: 'general',
      label: t('books_tab_general', { defaultValue: 'GÉNÉRAL' }),
      icon: <LibraryIcon className="h-4 w-4" />,
    },
    {
      id: 'scanner',
      label: t('books_tab_scanner', { defaultValue: 'SCANNEUR' }),
      icon: <ScanSearch className="h-4 w-4" />,
    },
    {
      id: 'options',
      label: t('books_tab_options', { defaultValue: 'OPTIONS' }),
      icon: <SlidersHorizontal className="h-4 w-4" />,
    },
    {
      id: 'metadata',
      label: t('books_tab_metadata', { defaultValue: 'MÉTADONNÉES' }),
      icon: <FileText className="h-4 w-4" />,
    },
  ]

  const intervalOptions = [
    { value: 'disabled', label: t('books_interval_disabled', { defaultValue: 'Désactivé' }) },
    { value: 'hourly', label: t('books_interval_hourly', { defaultValue: 'Toutes les heures' }) },
    {
      value: 'every_6h',
      label: t('books_interval_6h', { defaultValue: 'Toutes les 6 heures' }),
    },
    {
      value: 'every_12h',
      label: t('books_interval_12h', { defaultValue: 'Toutes les 12 heures' }),
    },
    { value: 'daily', label: t('books_interval_daily', { defaultValue: 'Tous les jours' }) },
    {
      value: 'weekly',
      label: t('books_interval_weekly', { defaultValue: 'Toutes les semaines' }),
    },
  ]

  const typeOptions = [
    { value: 'comics', label: t('books_type_comics') },
    { value: 'books', label: t('books_type_books') },
    { value: 'ebooks', label: t('books_type_ebooks') },
  ]

  const seriesCoverOptions = [
    { value: 'first', label: t('books_cover_first', { defaultValue: 'Premier' }) },
    { value: 'last', label: t('books_cover_last', { defaultValue: 'Dernier' }) },
  ]

  const thumbnailWidthOptions = [
    { value: '240', label: '240 px' },
    { value: '480', label: '480 px' },
    { value: '720', label: '720 px' },
    { value: '960', label: '960 px' },
  ]

  const readingDirectionOptions = [
    { value: '', label: t('books_reading_dir_auto', { defaultValue: 'Automatique' }) },
    { value: 'ltr', label: t('books_reading_dir_ltr', { defaultValue: 'De gauche à droite' }) },
    { value: 'rtl', label: t('books_reading_dir_rtl', { defaultValue: 'De droite à gauche' }) },
    { value: 'vertical', label: t('books_reading_dir_vertical', { defaultValue: 'Vertical' }) },
    { value: 'webtoon', label: t('books_reading_dir_webtoon', { defaultValue: 'Webtoon' }) },
  ]

  return (
    <FloatingWindow
      title={
        mode === 'create'
          ? t('books_create_title', { defaultValue: 'Ajouter une bibliothèque' })
          : t('books_edit_library_title', { defaultValue: 'Modifier la bibliothèque' })
      }
      icon={<LibraryIcon className="h-4 w-4" />}
      onClose={onClose}
      defaultWidth={640}
      defaultHeight={560}
      minWidth={520}
      minHeight={420}
      resizable
      backdrop
    >
      <div className="flex h-full flex-col" data-module="books">
        <div className="flex min-h-0 flex-1">
          {/* Left tab rail */}
          <nav className="flex w-44 flex-shrink-0 flex-col gap-0.5 border-r border-border bg-surface-1 p-2">
            {tabs.map((tb) => {
              const active = tab === tb.id
              return (
                <button
                  key={tb.id}
                  type="button"
                  onClick={() => setTab(tb.id)}
                  className={`flex items-center gap-2.5 rounded-md px-3 py-2 text-left text-xs font-semibold tracking-wide transition ${
                    active
                      ? 'bg-primary-light text-primary'
                      : 'text-text-secondary hover:bg-surface-2 hover:text-text-primary'
                  }`}
                >
                  {tb.icon}
                  {tb.label}
                </button>
              )
            })}
          </nav>

          {/* Right content */}
          <div className="min-w-0 flex-1 overflow-y-auto p-5">
            {tab === 'general' && (
              <div className="flex flex-col gap-4">
                <div>
                  <Input
                    label={t('books_field_name', { defaultValue: 'Nom' })}
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    onBlur={() => setTouchedName(true)}
                    error={
                      touchedName && nameEmpty
                        ? t('books_field_required', { defaultValue: 'Requis' })
                        : undefined
                    }
                    autoFocus
                  />
                </div>

                {mode === 'create' && (
                  <div>
                    <FieldLabel>{t('books_field_type')}</FieldLabel>
                    <Dropdown
                      value={libType}
                      onChange={(v) => setLibType(v as LibraryType)}
                      options={typeOptions}
                      width="100%"
                      height={36}
                    />
                  </div>
                )}

                {mode === 'edit' && (
                  <>
                    <div className="flex items-end gap-4">
                      <div>
                        <FieldLabel>
                          {t('books_field_icon', { defaultValue: 'Icône' })}
                        </FieldLabel>
                        <Input
                          value={icon}
                          onChange={(e) => setIcon(e.target.value)}
                          maxLength={4}
                          className="w-20 text-center"
                        />
                      </div>
                      <div>
                        <FieldLabel>
                          {t('books_field_color', { defaultValue: 'Couleur' })}
                        </FieldLabel>
                        <ColorField t={t} color={color} onChange={setColor} />
                      </div>
                    </div>

                    <Checkbox
                      checked={shared}
                      onChange={setShared}
                      label={t('books_field_shared', {
                        defaultValue: 'Partagée avec tous les utilisateurs',
                      })}
                      labelClassName="text-text-secondary"
                    />
                  </>
                )}

                <div>
                  <FieldLabel>
                    {t('books_field_root_folder', { defaultValue: 'Dossier racine' })}
                  </FieldLabel>
                  {mode === 'create' ? (
                    <div className="flex flex-col gap-1">
                      <div className="flex items-center gap-2">
                        <input
                          readOnly
                          value={folder?.name ?? ''}
                          placeholder={t('books_no_folder', {
                            defaultValue: 'Aucun dossier sélectionné',
                          })}
                          className="flex-1 cursor-default rounded-md border border-border bg-surface-1 px-3 py-2 text-sm text-text-primary outline-none placeholder:text-text-tertiary"
                        />
                        <Button variant="secondary" onClick={chooseFolder}>
                          {t('books_browse', { defaultValue: 'PARCOURIR' })}
                        </Button>
                      </div>
                    </div>
                  ) : (
                    <input
                      readOnly
                      value={
                        library?.files_folder_id
                          ? t('books_folder_linked', { defaultValue: 'Dossier lié' })
                          : '—'
                      }
                      className="w-full cursor-default rounded-md border border-border bg-surface-1 px-3 py-2 text-sm text-text-tertiary outline-none"
                    />
                  )}
                </div>
              </div>
            )}

            {tab === 'scanner' && (
              <div className="flex flex-col gap-3">
                <div>
                  <CheckRow
                    checked={sc.scan_on_startup}
                    onChange={(v) => setScanner({ scan_on_startup: v })}
                    label={t('books_scan_on_startup', { defaultValue: 'Scan au démarrage' })}
                  />
                </div>

                <div>
                  <FieldLabel>
                    {t('books_scan_interval', { defaultValue: 'Intervalle de Scan' })}
                  </FieldLabel>
                  <Dropdown
                    value={sc.scan_interval}
                    onChange={(v) =>
                      setScanner({ scan_interval: v as LibrarySettings['scanner']['scan_interval'] })
                    }
                    options={intervalOptions}
                    width="100%"
                    height={36}
                  />
                </div>

                <div>
                  <FieldLabel>
                    {t('books_oneshots_dir', { defaultValue: 'Répertoire des One-shots' })}
                  </FieldLabel>
                  <Input
                    value={sc.oneshots_dir}
                    onChange={(e) => setScanner({ oneshots_dir: e.target.value })}
                  />
                </div>

                <div>
                  <SectionTitle>
                    {t('books_scan_filetypes', { defaultValue: 'Scanner ces types de fichiers' })}
                  </SectionTitle>
                  <div className="flex flex-wrap gap-2">
                    <ToggleChip
                      active={sc.scan_comics}
                      onClick={() => setScanner({ scan_comics: !sc.scan_comics })}
                      label={t('books_filetype_comics', {
                        defaultValue: 'Archive des bandes dessinées',
                      })}
                    />
                    <ToggleChip
                      active={sc.scan_pdf}
                      onClick={() => setScanner({ scan_pdf: !sc.scan_pdf })}
                      label={t('books_filetype_pdf', { defaultValue: 'PDF' })}
                    />
                    <ToggleChip
                      active={sc.scan_epub}
                      onClick={() => setScanner({ scan_epub: !sc.scan_epub })}
                      label={t('books_filetype_epub', { defaultValue: 'Epub' })}
                    />
                  </div>
                </div>

                <div>
                  <SectionTitle>
                    {t('books_excluded_dirs', { defaultValue: 'Répertoires exclus' })}
                  </SectionTitle>
                  <ChipInput
                    values={sc.excluded_dirs}
                    onChange={(v) => setScanner({ excluded_dirs: v })}
                    placeholder={t('books_add_dir', { defaultValue: 'Ajouter un répertoire…' })}
                  />
                </div>
              </div>
            )}

            {tab === 'options' && (
              <div className="flex flex-col gap-3">
                <div>
                  <FieldLabel>
                    {t('books_opt_series_cover', { defaultValue: 'Couverture de la série' })}
                  </FieldLabel>
                  <Dropdown
                    value={op.series_cover}
                    onChange={(v) =>
                      setOptions({ series_cover: v as LibrarySettings['options']['series_cover'] })
                    }
                    options={seriesCoverOptions}
                    width="100%"
                    height={36}
                  />
                </div>

                <div>
                  <FieldLabel>
                    {t('books_opt_cover_page', {
                      defaultValue: 'Page utilisée comme couverture (0 = première)',
                    })}
                  </FieldLabel>
                  <Input
                    type="number"
                    min={0}
                    value={String(op.cover_page)}
                    onChange={(e) =>
                      setOptions({ cover_page: Math.max(0, Number(e.target.value) || 0) })
                    }
                    className="w-32"
                  />
                </div>

                <div>
                  <FieldLabel>
                    {t('books_opt_thumbnail_width', { defaultValue: 'Largeur des vignettes' })}
                  </FieldLabel>
                  <Dropdown
                    value={String(op.thumbnail_width)}
                    onChange={(v) => setOptions({ thumbnail_width: Number(v) })}
                    options={thumbnailWidthOptions}
                    width="100%"
                    height={36}
                  />
                </div>

                <div>
                  <CheckRow
                    checked={op.analyze_dimensions}
                    onChange={(v) => setOptions({ analyze_dimensions: v })}
                    label={t('books_opt_analyze_dim', {
                      defaultValue: 'Analyser les dimensions des pages',
                    })}
                  />
                  <CheckRow
                    checked={op.hash_files}
                    onChange={(v) => setOptions({ hash_files: v })}
                    label={t('books_opt_hash_files', {
                      defaultValue: 'Calculer une empreinte pour les fichiers',
                    })}
                  />
                </div>

                <div>
                  <FieldLabel>
                    {t('books_opt_reading_dir', { defaultValue: 'Sens de lecture par défaut' })}
                  </FieldLabel>
                  <Dropdown
                    value={op.default_reading_direction}
                    onChange={(v) =>
                      setOptions({
                        default_reading_direction:
                          v as LibrarySettings['options']['default_reading_direction'],
                      })
                    }
                    options={readingDirectionOptions}
                    width="100%"
                    height={36}
                  />
                </div>
              </div>
            )}

            {tab === 'metadata' && (
              <div className="flex flex-col gap-3">
                <div>
                  <CheckRow
                    checked={md.import_comicinfo}
                    onChange={(v) => setMetadata({ import_comicinfo: v })}
                    label={t('books_md_import_comicinfo', {
                      defaultValue:
                        'Importer les métadonnées pour les fichiers CBR/CBZ contenant un fichier ComicInfo.xml',
                    })}
                  />
                  <CheckRow
                    indent
                    checked={md.comicinfo_book}
                    onChange={(v) => setMetadata({ comicinfo_book: v })}
                    label={t('books_md_comicinfo_book', { defaultValue: 'Métadonnées du livre' })}
                  />
                  <CheckRow
                    indent
                    checked={md.comicinfo_series}
                    onChange={(v) => setMetadata({ comicinfo_series: v })}
                    label={t('books_md_comicinfo_series', {
                      defaultValue: 'Métadonnées de la série',
                    })}
                  />
                  <CheckRow
                    indent
                    checked={md.comicinfo_volume_in_title}
                    onChange={(v) => setMetadata({ comicinfo_volume_in_title: v })}
                    label={t('books_md_comicinfo_volume', {
                      defaultValue: 'Ajouter le volume au titre de la série',
                    })}
                  />
                </div>

                <div>
                  <CheckRow
                    checked={md.import_epub}
                    onChange={(v) => setMetadata({ import_epub: v })}
                    label={t('books_md_import_epub', {
                      defaultValue: 'Importer les métadonnées des fichiers EPUB',
                    })}
                  />
                  <CheckRow
                    indent
                    checked={md.epub_book}
                    onChange={(v) => setMetadata({ epub_book: v })}
                    label={t('books_md_epub_book', { defaultValue: 'Métadonnées du livre' })}
                  />
                  <CheckRow
                    indent
                    checked={md.epub_series}
                    onChange={(v) => setMetadata({ epub_series: v })}
                    label={t('books_md_epub_series', { defaultValue: 'Métadonnées de la série' })}
                  />
                </div>

                <div>
                  <FieldLabel>
                    {t('books_md_language', { defaultValue: 'Langue des métadonnées' })}
                  </FieldLabel>
                  <Input
                    value={md.metadata_language}
                    onChange={(e) => setMetadata({ metadata_language: e.target.value })}
                    placeholder={t('books_md_language_ph', { defaultValue: 'ex. fr' })}
                    className="w-32"
                  />
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-end gap-2 border-t border-border px-5 py-3">
          {error && (
            <p className="mr-auto flex items-center gap-1.5 text-sm text-danger">
              <AlertTriangle className="h-4 w-4 flex-shrink-0" />
              {error}
            </p>
          )}
          <Button variant="ghost" onClick={onClose} disabled={submitting}>
            {t('common_cancel', { defaultValue: 'ANNULER' })}
          </Button>
          <Button variant="primary" onClick={submit} disabled={!canSubmit} loading={submitting}>
            {mode === 'create'
              ? t('common_add', { defaultValue: 'AJOUTER' })
              : t('common_save', { defaultValue: 'ENREGISTRER' })}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
