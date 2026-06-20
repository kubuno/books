import { useState, type KeyboardEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { Input, Button } from '@ui'
import { X, Plus, Star } from 'lucide-react'
import type { AuthorEntry } from '../api'

/** Editable list of string chips (tags / genres). Add via Enter or comma. */
export function ChipsField({
  label,
  value,
  onChange,
  placeholder,
}: {
  label?: string
  value: string[]
  onChange: (next: string[]) => void
  placeholder?: string
}) {
  const [draft, setDraft] = useState('')

  function commit() {
    const trimmed = draft.trim()
    if (trimmed && !value.includes(trimmed)) onChange([...value, trimmed])
    setDraft('')
  }

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault()
      commit()
    } else if (e.key === 'Backspace' && !draft && value.length > 0) {
      onChange(value.slice(0, -1))
    }
  }

  return (
    <div>
      {label && (
        <label className="mb-1.5 block text-sm font-medium text-text-secondary">{label}</label>
      )}
      <div className="flex flex-wrap items-center gap-1.5 rounded-md border border-border bg-surface-0 px-2 py-1.5">
        {value.map((tag) => (
          <span
            key={tag}
            className="inline-flex items-center gap-1 rounded bg-surface-2 px-1.5 py-0.5 text-xs text-text-secondary"
          >
            {tag}
            <button
              type="button"
              onClick={() => onChange(value.filter((v) => v !== tag))}
              className="text-text-tertiary hover:text-danger"
            >
              <X className="h-3 w-3" />
            </button>
          </span>
        ))}
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={onKeyDown}
          onBlur={commit}
          placeholder={placeholder}
          className="min-w-[80px] flex-1 bg-transparent text-sm text-text-primary outline-none placeholder:text-text-tertiary"
        />
      </div>
    </div>
  )
}

/** Editable list of authors (name + optional role). */
export function AuthorsField({
  label,
  value,
  onChange,
}: {
  label?: string
  value: AuthorEntry[]
  onChange: (next: AuthorEntry[]) => void
}) {
  const { t } = useTranslation('books')

  function update(i: number, patch: Partial<AuthorEntry>) {
    onChange(value.map((a, idx) => (idx === i ? { ...a, ...patch } : a)))
  }

  return (
    <div>
      {label && (
        <label className="mb-1.5 block text-sm font-medium text-text-secondary">{label}</label>
      )}
      <div className="flex flex-col gap-2">
        {value.map((author, i) => (
          <div key={i} className="flex items-center gap-2">
            <Input
              value={author.name}
              onChange={(e) => update(i, { name: e.target.value })}
              placeholder={t('books_field_author_name')}
              className="flex-1"
            />
            <Input
              value={author.role ?? ''}
              onChange={(e) => update(i, { role: e.target.value })}
              placeholder={t('books_field_author_role')}
              className="w-32"
            />
            <button
              type="button"
              onClick={() => onChange(value.filter((_, idx) => idx !== i))}
              className="flex-shrink-0 text-text-tertiary hover:text-danger"
              title={t('common_delete')}
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        ))}
        <Button
          variant="ghost"
          icon={<Plus className="h-4 w-4" />}
          onClick={() => onChange([...value, { name: '', role: '' }])}
          className="self-start"
        >
          {t('books_add_author')}
        </Button>
      </div>
    </div>
  )
}

/** 0–5 star rating picker. Clicking the active star clears it. */
export function StarRating({
  label,
  value,
  onChange,
}: {
  label?: string
  value: number | null
  onChange: (next: number | null) => void
}) {
  return (
    <div>
      {label && (
        <label className="mb-1.5 block text-sm font-medium text-text-secondary">{label}</label>
      )}
      <div className="flex items-center gap-0.5">
        {[1, 2, 3, 4, 5].map((n) => (
          <button
            key={n}
            type="button"
            onClick={() => onChange(value === n ? null : n)}
            className="text-text-tertiary transition hover:text-amber-500"
          >
            <Star
              className={`h-5 w-5 ${value != null && n <= value ? 'fill-amber-400 text-amber-400' : ''}`}
            />
          </button>
        ))}
      </div>
    </div>
  )
}
