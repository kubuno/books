import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useQueryClient } from '@tanstack/react-query'
import { FloatingWindow, Button, Input, Dropdown, NumberInput, Checkbox } from '@ui'
import { Layers, AlertTriangle } from 'lucide-react'
import { bulkUpdateBooks } from '../api'
import { ChipsField } from './fields'

interface Props {
  /** IDs of the selected books. */
  ids: string[]
  onClose: () => void
  /** Called after a successful bulk update. */
  onDone?: () => void
}

const DIRECTIONS = ['ltr', 'rtl', 'vertical', 'webtoon'] as const
type Direction = (typeof DIRECTIONS)[number]

/**
 * P4 — bulk-edit a subset of fields across several books. Each field is opt-in:
 * only checked fields are sent in the PATCH.
 */
export default function BulkEditDialog({ ids, onClose, onDone }: Props) {
  const { t } = useTranslation('books')
  const qc = useQueryClient()

  const [setTags, setSetTags] = useState(false)
  const [tags, setTagsValue] = useState<string[]>([])
  const [setPublisher, setSetPublisher] = useState(false)
  const [publisher, setPublisherValue] = useState('')
  const [setLanguage, setSetLanguage] = useState(false)
  const [language, setLanguageValue] = useState('')
  const [setDirection, setSetDirection] = useState(false)
  const [direction, setDirectionValue] = useState<Direction>('ltr')
  const [setAge, setSetAge] = useState(false)
  const [age, setAgeValue] = useState(0)

  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const directionOptions = DIRECTIONS.map((d) => ({ value: d, label: t(`books_direction_${d}`) }))
  const anyField = setTags || setPublisher || setLanguage || setDirection || setAge

  async function apply() {
    setSaving(true)
    setError(null)
    try {
      await bulkUpdateBooks({
        ids,
        ...(setTags ? { tags } : {}),
        ...(setPublisher ? { publisher: publisher.trim() || null } : {}),
        ...(setLanguage ? { language: language.trim() || null } : {}),
        ...(setDirection ? { reading_direction: direction } : {}),
        ...(setAge ? { age_rating: age || null } : {}),
      })
      void qc.invalidateQueries({ queryKey: ['books'] })
      onDone?.()
      onClose()
    } catch {
      setError(t('books_save_error'))
      setSaving(false)
    }
  }

  return (
    <FloatingWindow
      title={t('books_bulk_title', { count: ids.length })}
      icon={<Layers className="h-4 w-4" />}
      onClose={onClose}
      defaultWidth={520}
      defaultHeight={480}
      backdrop
    >
      <div className="flex h-full flex-col" data-module="books">
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-5">
          <p className="text-sm text-text-secondary">{t('books_bulk_hint')}</p>

          <div className="flex flex-col gap-2">
            <Checkbox checked={setTags} onChange={setSetTags} label={t('books_field_tags')} />
            {setTags && <ChipsField value={tags} onChange={setTagsValue} />}
          </div>

          <div className="flex flex-col gap-2">
            <Checkbox
              checked={setPublisher}
              onChange={setSetPublisher}
              label={t('books_field_publisher')}
            />
            {setPublisher && (
              <Input value={publisher} onChange={(e) => setPublisherValue(e.target.value)} />
            )}
          </div>

          <div className="flex flex-col gap-2">
            <Checkbox
              checked={setLanguage}
              onChange={setSetLanguage}
              label={t('books_field_language')}
            />
            {setLanguage && (
              <Input
                value={language}
                onChange={(e) => setLanguageValue(e.target.value)}
                placeholder="fr, en…"
              />
            )}
          </div>

          <div className="flex flex-col gap-2">
            <Checkbox
              checked={setDirection}
              onChange={setSetDirection}
              label={t('books_field_direction')}
            />
            {setDirection && (
              <Dropdown
                value={direction}
                onChange={(v) => setDirectionValue(v as Direction)}
                options={directionOptions}
                width="100%"
                height={36}
              />
            )}
          </div>

          <div className="flex flex-col gap-2">
            <Checkbox checked={setAge} onChange={setSetAge} label={t('books_field_age_rating')} />
            {setAge && <NumberInput value={age} onChange={setAgeValue} min={0} max={21} />}
          </div>

          {error && (
            <p className="flex items-center gap-1.5 text-sm text-danger">
              <AlertTriangle className="h-4 w-4 flex-shrink-0" />
              {error}
            </p>
          )}
        </div>

        <div className="flex justify-end gap-2 border-t border-border p-4">
          <Button variant="ghost" onClick={onClose} disabled={saving}>
            {t('common_cancel')}
          </Button>
          <Button
            variant="primary"
            onClick={apply}
            loading={saving}
            disabled={!anyField}
          >
            {t('books_bulk_apply')}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
