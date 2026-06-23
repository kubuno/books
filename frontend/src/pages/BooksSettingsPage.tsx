import React, { useState } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { BookOpen, ArrowLeft, ExternalLink, Check } from 'lucide-react'
import { Toggle, Button, Radio } from '@ui'
import { useModulePrefs } from '../userPrefs'

// ── Per-user preferences (backend, cross-device via core users.preferences) ─────

interface BooksPrefs {
  readingDirection: string // 'ltr' | 'rtl' | 'webtoon'
  pageMode:         string // 'single' | 'double'
  fit:              string // 'width' | 'height' | 'page'
  readerTheme:      string // 'light' | 'dark' | 'sepia'
  libraryView:      string // 'grid' | 'list'
  sort:             string // 'title' | 'added' | 'read'
}

const DEFAULT_PREFS: BooksPrefs = {
  readingDirection: 'ltr',
  pageMode:         'single',
  fit:              'width',
  readerTheme:      'light',
  libraryView:      'grid',
  sort:             'title',
}

// ── Mail-style layout helpers ───────────────────────────────────────────────────

function SettingsRow({ label, description, children }: {
  label: string; description?: string; children: React.ReactNode
}) {
  return (
    <div className="flex items-start gap-8 py-4 border-b border-[#e8eaed] last:border-0">
      <div className="w-60 flex-shrink-0">
        <p className="text-sm text-[#202124] font-normal">{label}</p>
        {description && <p className="text-xs text-text-tertiary mt-0.5 leading-relaxed">{description}</p>}
      </div>
      <div className="flex-1">{children}</div>
    </div>
  )
}

function RadioGroup({ options, value, onChange }: {
  options: { value: string; label: string }[]; value: string; onChange: (v: string) => void
}) {
  return (
    <div className="flex flex-col items-start gap-2">
      {options.map(opt => (
        <Radio key={opt.value} checked={value === opt.value} onChange={() => onChange(opt.value)} label={opt.label} />
      ))}
    </div>
  )
}

// ── Préférences tab (per-user) ──────────────────────────────────────────────────

function PreferencesTab() {
  const { t } = useTranslation('books')
  const { prefs: saved, update } = useModulePrefs<BooksPrefs>('books', DEFAULT_PREFS)
  const [prefs, setPrefs] = useState<BooksPrefs>(saved)
  const [savedFlag, setSavedFlag] = useState(false)
  const [busy, setBusy] = useState(false)

  const set = <K extends keyof BooksPrefs>(key: K, value: BooksPrefs[K]) =>
    setPrefs(p => ({ ...p, [key]: value }))

  const save = async () => {
    setBusy(true)
    try {
      await update(prefs)
      setSavedFlag(true)
      setTimeout(() => setSavedFlag(false), 2500)
    } finally { setBusy(false) }
  }

  return (
    <div>
      <SettingsRow
        label={t('books_pref_reading_dir', { defaultValue: 'Sens de lecture' })}
        description={t('books_pref_reading_dir_desc', { defaultValue: 'Direction de progression des pages dans le lecteur.' })}
      >
        <RadioGroup
          value={prefs.readingDirection}
          onChange={v => set('readingDirection', v)}
          options={[
            { value: 'ltr',     label: t('books_pref_dir_ltr',     { defaultValue: 'De gauche à droite' }) },
            { value: 'rtl',     label: t('books_pref_dir_rtl',     { defaultValue: 'De droite à gauche (manga)' }) },
            { value: 'webtoon', label: t('books_pref_dir_webtoon', { defaultValue: 'Webtoon (défilement vertical)' }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow
        label={t('books_pref_page_mode', { defaultValue: 'Mode d\'affichage' })}
        description={t('books_pref_page_mode_desc', { defaultValue: 'Nombre de pages affichées côte à côte.' })}
      >
        <RadioGroup
          value={prefs.pageMode}
          onChange={v => set('pageMode', v)}
          options={[
            { value: 'single', label: t('books_pref_page_single', { defaultValue: 'Page simple' }) },
            { value: 'double', label: t('books_pref_page_double', { defaultValue: 'Double page' }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow
        label={t('books_pref_fit', { defaultValue: 'Ajustement' })}
        description={t('books_pref_fit_desc', { defaultValue: 'Façon dont la page remplit la zone de lecture.' })}
      >
        <RadioGroup
          value={prefs.fit}
          onChange={v => set('fit', v)}
          options={[
            { value: 'width',  label: t('books_pref_fit_width',  { defaultValue: 'Largeur' }) },
            { value: 'height', label: t('books_pref_fit_height', { defaultValue: 'Hauteur' }) },
            { value: 'page',   label: t('books_pref_fit_page',   { defaultValue: 'Page entière' }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow
        label={t('books_pref_reader_theme', { defaultValue: 'Thème du lecteur' })}
        description={t('books_pref_reader_theme_desc', { defaultValue: 'Couleur de fond et de texte des eBooks.' })}
      >
        <RadioGroup
          value={prefs.readerTheme}
          onChange={v => set('readerTheme', v)}
          options={[
            { value: 'light', label: t('books_pref_theme_light', { defaultValue: 'Clair' }) },
            { value: 'dark',  label: t('books_pref_theme_dark',  { defaultValue: 'Sombre' }) },
            { value: 'sepia', label: t('books_pref_theme_sepia', { defaultValue: 'Sépia' }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow
        label={t('books_pref_library_view', { defaultValue: 'Vue de la bibliothèque' })}
        description={t('books_pref_library_view_desc', { defaultValue: 'Présentation par défaut des couvertures.' })}
      >
        <RadioGroup
          value={prefs.libraryView}
          onChange={v => set('libraryView', v)}
          options={[
            { value: 'grid', label: t('books_pref_view_grid', { defaultValue: 'Grille' }) },
            { value: 'list', label: t('books_pref_view_list', { defaultValue: 'Liste' }) },
          ]}
        />
      </SettingsRow>

      <SettingsRow label={t('books_pref_sort', { defaultValue: 'Tri par défaut' })}>
        <RadioGroup
          value={prefs.sort}
          onChange={v => set('sort', v)}
          options={[
            { value: 'title', label: t('books_pref_sort_title', { defaultValue: 'Titre (A → Z)' }) },
            { value: 'added', label: t('books_pref_sort_added', { defaultValue: 'Date d\'ajout (récents d\'abord)' }) },
            { value: 'read',  label: t('books_pref_sort_read',  { defaultValue: 'Dernière lecture' }) },
          ]}
        />
      </SettingsRow>

      <div className="pt-5 flex items-center gap-3">
        <Button onClick={save} loading={busy}>
          {savedFlag
            ? <><Check size={14} className="mr-1.5 inline" />{t('books_settings_saved', { defaultValue: 'Enregistré' })}</>
            : t('books_settings_save_changes', { defaultValue: 'Enregistrer les modifications' })}
        </Button>
        <Button variant="ghost" onClick={() => setPrefs(saved)}>
          {t('common_cancel', { defaultValue: 'Annuler' })}
        </Button>
      </div>
    </div>
  )
}

// ── À propos tab ────────────────────────────────────────────────────────────────

function AboutTab() {
  const { t } = useTranslation('books')
  return (
    <div className="rounded-xl border border-border overflow-hidden">
      <div className="flex items-center gap-3 px-5 py-4 border-b border-border bg-surface-1">
        <div className="w-10 h-10 rounded-xl bg-amber-100 flex items-center justify-center shrink-0">
          <BookOpen size={20} className="text-amber-600" />
        </div>
        <div>
          <p className="text-sm font-semibold text-text-primary">Kubuno Books</p>
          <p className="text-xs text-text-tertiary">v0.1.0 · {t('books_official_module', { defaultValue: 'Module officiel' })}</p>
        </div>
        <span className="ml-auto text-xs font-medium px-2 py-0.5 rounded-full bg-orange-100 text-orange-700">Rust</span>
      </div>
      <div className="px-5 py-4">
        <a href="https://github.com/kubuno/books" target="_blank" rel="noopener noreferrer"
          className="inline-flex items-center gap-1.5 text-sm text-primary hover:underline">
          <ExternalLink size={13} /> github.com/kubuno/books
        </a>
      </div>
    </div>
  )
}

// ── Main page (mail-style breadcrumb + tab bar) ─────────────────────────────────

type Tab = 'preferences' | 'about'

export default function BooksSettingsPage() {
  const { t } = useTranslation('books')
  const [tab, setTab] = useState<Tab>('preferences')

  const tabs: { id: Tab; label: string }[] = [
    { id: 'preferences', label: t('books_tab_preferences', { defaultValue: 'Préférences' }) },
    { id: 'about',       label: t('books_tab_about', { defaultValue: 'À propos' }) },
  ]

  return (
    <div className="flex flex-col h-full bg-white overflow-hidden" data-module="books">
      {/* Breadcrumb header */}
      <div className="flex items-center gap-2 px-6 py-2.5 border-b border-[#e8eaed] flex-shrink-0" style={{ background: '#f8f9fa' }}>
        <Link to="/books" className="flex items-center gap-1.5 text-sm text-[#1a73e8] hover:underline">
          <ArrowLeft size={14} />
          Books
        </Link>
        <span className="text-text-tertiary text-sm">/</span>
        <div className="flex items-center gap-1.5">
          <BookOpen size={15} className="text-text-secondary" />
          <span className="text-sm text-text-primary">{t('books_settings_title', { defaultValue: 'Réglages' })}</span>
        </div>
      </div>

      {/* Tab bar (Gmail-style) */}
      <div className="flex items-end border-b border-[#e8eaed] px-4 flex-shrink-0 overflow-x-auto" style={{ background: '#fff' }}>
        {tabs.map(tb => (
          <button key={tb.id} onClick={() => setTab(tb.id)}
            className={`px-4 py-3 text-sm border-b-2 -mb-px transition-colors whitespace-nowrap ${
              tab === tb.id ? 'border-[#1a73e8] text-[#1a73e8] font-medium' : 'border-transparent text-[#5f6368] hover:text-[#202124] hover:bg-[#f1f3f4]'}`}>
            {tb.label}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        <div className="max-w-3xl mx-auto px-8 py-6">
          {tab === 'preferences' && <PreferencesTab />}
          {tab === 'about'       && <AboutTab />}
        </div>
      </div>
    </div>
  )
}
