// Instance administration of books, rendered in the core admin console under
// Modules ▸ Books. Books stores nothing in core settings: libraries and the
// default metadata language live in the module's own DB and are edited through
// /books/admin/*. Both panes below are custom views (no generic form).
//
// This is also where the library management LIVES now — creating, editing,
// scanning and deleting a library is an administrator action, so it was moved
// out of the user-facing browsing pages into this panel.

import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Button, Input, ConfirmDialog } from '@ui'
import { ModuleAdminRegistry, useConfirm } from '@kubuno/sdk'
import { Library as LibraryIcon, Plus, Check } from 'lucide-react'
import {
  listLibraries,
  getAdminSettings,
  patchAdminSettings,
  listRestrictions,
  setRestrictions,
  type UserRestriction,
} from '../api'
import LibraryCard from '../pages/LibraryCard'
import LibrarySettingsDialog from '../components/LibrarySettingsDialog'

// ── Libraries pane: create / edit / scan / delete ─────────────────────────────

function LibrariesSection() {
  const { t } = useTranslation('books')
  const { confirm, confirmState, handleConfirm, handleCancel } = useConfirm()
  const [creating, setCreating] = useState(false)

  const { data: libraries, isLoading } = useQuery({
    queryKey: ['books', 'libraries'],
    queryFn: listLibraries,
  })

  return (
    <div data-module="books">
      <div className="mb-4 flex items-start justify-between gap-4">
        <p className="text-sm text-text-secondary">
          {t('books_admin_libraries_desc', {
            defaultValue:
              'Créez et configurez les bibliothèques de l’instance. Chaque bibliothèque pointe vers un dossier Drive ou un montage distant, puis est analysée pour indexer son contenu.',
          })}
        </p>
        <Button
          variant="primary"
          icon={<Plus className="h-4 w-4" />}
          onClick={() => setCreating(true)}
          className="flex-shrink-0"
        >
          {t('books_new_library')}
        </Button>
      </div>

      {isLoading ? (
        <p className="py-8 text-center text-sm text-text-tertiary">{t('books_loading')}</p>
      ) : libraries && libraries.length > 0 ? (
        <div className="grid gap-3 grid-cols-[repeat(auto-fill,minmax(280px,1fr))]">
          {libraries.map((lib) => (
            <LibraryCard key={lib.id} library={lib} isAdmin onConfirm={confirm} />
          ))}
        </div>
      ) : (
        <div className="rounded-xl border border-dashed border-border py-12 text-center">
          <LibraryIcon className="mx-auto mb-3 h-10 w-10 text-text-tertiary" />
          <p className="text-sm text-text-secondary">{t('books_empty_libraries')}</p>
        </div>
      )}

      {creating && (
        <LibrarySettingsDialog mode="create" onClose={() => setCreating(false)} />
      )}
      {confirmState && (
        <ConfirmDialog {...confirmState} onConfirm={handleConfirm} onCancel={handleCancel} />
      )}
    </div>
  )
}

// ── Metadata pane: instance-wide default cataloging language ───────────────────

function MetadataSection() {
  const { t } = useTranslation('books')
  const qc = useQueryClient()
  const { data } = useQuery({
    queryKey: ['books', 'admin', 'settings'],
    queryFn: getAdminSettings,
  })

  const [lang, setLang] = useState<string | null>(null)
  const [savedFlag, setSavedFlag] = useState(false)
  const [busy, setBusy] = useState(false)

  // `lang === null` means "not yet edited": show the server value until touched.
  const value = lang ?? data?.metadata_language ?? ''

  async function save() {
    setBusy(true)
    try {
      await patchAdminSettings({ metadata_language: value.trim() })
      await qc.invalidateQueries({ queryKey: ['books', 'admin', 'settings'] })
      setSavedFlag(true)
      setTimeout(() => setSavedFlag(false), 2500)
    } finally {
      setBusy(false)
    }
  }

  return (
    <div data-module="books" className="max-w-2xl">
      <p className="mb-4 text-sm text-text-secondary">
        {t('books_admin_metadata_desc', {
          defaultValue:
            'Langue par défaut utilisée pour récupérer les métadonnées (titres, résumés, séries) lors de l’analyse. Une bibliothèque qui définit sa propre langue l’emporte ; celle-ci ne sert que de repli.',
        })}
      </p>
      <div className="rounded-xl border border-border bg-surface-0 p-5">
        <label className="mb-1.5 block text-sm text-text-primary">
          {t('books_md_language', { defaultValue: 'Langue des métadonnées' })}
        </label>
        <Input
          value={value}
          onChange={(e) => setLang(e.target.value)}
          placeholder={t('books_md_language_ph', { defaultValue: 'ex. fr' })}
          className="w-40"
        />
        <p className="mt-1.5 text-xs text-text-tertiary">
          {t('books_admin_metadata_hint', {
            defaultValue: 'Code ISO à deux lettres (fr, en, de…). Vide = deviné par fichier.',
          })}
        </p>
        <div className="mt-4">
          <Button onClick={save} loading={busy}>
            {savedFlag ? (
              <>
                <Check size={14} className="mr-1.5 inline" />
                {t('books_settings_saved', { defaultValue: 'Enregistré' })}
              </>
            ) : (
              t('books_settings_save_changes', { defaultValue: 'Enregistrer' })
            )}
          </Button>
        </div>
      </div>
    </div>
  )
}

// ── Access pane: per-account restrictions ────────────────────────────────────
//
// The instance-wide switches of this page (downloads, OPDS, unrated content)
// are DECLARED in module.toml and rendered by the core above this section. What
// is left here is the part no generic form can express: a per-account rule made
// of a library multi-select and an age ceiling.
//
// Two states must stay distinguishable, and the wording is what does it:
//   • "toutes les bibliothèques" → `library_ids = null`
//   • no library ticked          → `library_ids = []`, a real answer that hides
//                                   everything, not an unsaved form.

/** Age ceilings offered. `null` = no ceiling at all. */
const AGE_CEILINGS: { value: number | null; label: string }[] = [
  { value: null, label: 'Aucune limite' },
  { value: 3,    label: '3 ans' },
  { value: 7,    label: '7 ans' },
  { value: 10,   label: '10 ans' },
  { value: 13,   label: '13 ans' },
  { value: 16,   label: '16 ans' },
  { value: 18,   label: '18 ans' },
]

function RestrictionRow({
  user,
  libraries,
  onSaved,
}: {
  user: UserRestriction
  libraries: { id: string; name: string }[]
  onSaved: () => void
}) {
  const [allLibraries, setAllLibraries] = useState(user.library_ids === null)
  const [picked, setPicked] = useState<string[]>(user.library_ids ?? [])
  const [ageMax, setAgeMax] = useState<number | null>(user.age_max)
  const [busy, setBusy] = useState(false)
  const [saved, setSaved] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const dirty =
    allLibraries !== (user.library_ids === null) ||
    ageMax !== user.age_max ||
    (!allLibraries &&
      JSON.stringify([...picked].sort()) !== JSON.stringify([...(user.library_ids ?? [])].sort()))

  function toggle(id: string) {
    setPicked((cur) => (cur.includes(id) ? cur.filter((x) => x !== id) : [...cur, id]))
  }

  async function save() {
    setBusy(true)
    setError(null)
    try {
      await setRestrictions(user.id, {
        library_ids: allLibraries ? null : picked,
        age_max: ageMax,
      })
      setSaved(true)
      setTimeout(() => setSaved(false), 2500)
      onSaved()
    } catch {
      setError('Enregistrement impossible.')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="rounded-xl border border-border bg-surface-0 p-4">
      <div className="mb-3 flex flex-wrap items-baseline justify-between gap-2">
        <div className="min-w-0">
          <div className="truncate text-sm text-text-primary">
            {user.display_name || user.email}
          </div>
          <div className="truncate text-xs text-text-tertiary">{user.email}</div>
        </div>
        {user.role === 'admin' && (
          <span className="rounded-full bg-surface-2 px-2 py-0.5 text-xs text-text-secondary">
            Administrateur
          </span>
        )}
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <div>
          <div className="mb-1.5 text-sm text-text-primary">Bibliothèques autorisées</div>
          <label className="flex items-center gap-2 text-sm text-text-secondary">
            <input
              type="checkbox"
              checked={allLibraries}
              onChange={(e) => setAllLibraries(e.target.checked)}
            />
            Toutes les bibliothèques
          </label>
          {!allLibraries && (
            <div className="mt-2 space-y-1">
              {libraries.map((lib) => (
                <label key={lib.id} className="flex items-center gap-2 text-sm text-text-secondary">
                  <input
                    type="checkbox"
                    checked={picked.includes(lib.id)}
                    onChange={() => toggle(lib.id)}
                  />
                  {lib.name}
                </label>
              ))}
              {picked.length === 0 && (
                <p className="mt-1 text-xs text-warning">
                  Aucune bibliothèque cochée : ce compte ne verra aucun livre.
                </p>
              )}
            </div>
          )}
        </div>

        <div>
          <div className="mb-1.5 text-sm text-text-primary">Âge maximal</div>
          <select
            value={ageMax === null ? '' : String(ageMax)}
            onChange={(e) => setAgeMax(e.target.value === '' ? null : Number(e.target.value))}
            className="rounded-lg border border-border bg-surface-0 px-2.5 py-1.5 text-sm text-text-primary"
          >
            {AGE_CEILINGS.map((a) => (
              <option key={String(a.value)} value={a.value === null ? '' : String(a.value)}>
                {a.label}
              </option>
            ))}
          </select>
          <p className="mt-1.5 text-xs text-text-tertiary">
            Masque les livres dont la classification dépasse cet âge. Les livres non classés
            dépendent du réglage « Bloquer les contenus sans classification » ci-dessus.
          </p>
        </div>
      </div>

      {error && <p className="mt-3 text-xs text-danger">{error}</p>}

      <div className="mt-4">
        <Button onClick={save} loading={busy} disabled={!dirty && !saved}>
          {saved ? (
            <>
              <Check size={14} className="mr-1.5 inline" />
              Enregistré
            </>
          ) : (
            'Enregistrer'
          )}
        </Button>
      </div>
    </div>
  )
}

function RestrictionsSection() {
  const qc = useQueryClient()
  const { data: users, isLoading } = useQuery({
    queryKey: ['books', 'admin', 'restrictions'],
    queryFn: listRestrictions,
  })
  const { data: libraries } = useQuery({
    queryKey: ['books', 'libraries'],
    queryFn: listLibraries,
  })

  const libs = (libraries ?? []).map((l) => ({ id: l.id, name: l.name }))
  const refresh = () => { void qc.invalidateQueries({ queryKey: ['books', 'admin', 'restrictions'] }) }

  return (
    <div data-module="books" className="max-w-3xl">
      <p className="mb-4 text-sm text-text-secondary">
        Restrictions appliquées compte par compte. Elles valent partout : listes, recherche,
        couvertures, pages, téléchargement, export et catalogue OPDS.
      </p>
      {isLoading && <p className="text-sm text-text-tertiary">Chargement…</p>}
      <div className="space-y-3">
        {(users ?? []).map((u) => (
          <RestrictionRow key={u.id} user={u} libraries={libs} onSaved={refresh} />
        ))}
      </div>
    </div>
  )
}

/** Registers the books admin sections into the core admin console. */
export function registerBooksAdmin() {
  // No label → the section IS the whole page for its group.
  ModuleAdminRegistry.register({
    moduleId: 'books',
    id: 'libraries',
    group: 'libraries',
    position: 10,
    Component: LibrariesSection,
  })
  ModuleAdminRegistry.register({
    moduleId: 'books',
    id: 'metadata',
    group: 'metadata',
    position: 10,
    Component: MetadataSection,
  })
  // Labelled, so it becomes a TAB of the access page rather than replacing it:
  // the instance-wide switches declared in module.toml keep their own generic
  // form next to it.
  ModuleAdminRegistry.register({
    moduleId: 'books',
    id: 'restrictions',
    group: 'access',
    label: 'Restrictions par compte',
    icon: 'Users',
    position: 20,
    Component: RestrictionsSection,
  })
}
