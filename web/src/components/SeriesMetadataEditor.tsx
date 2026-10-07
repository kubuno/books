import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useQueryClient } from '@tanstack/react-query'
import { FloatingWindow, Button, Input, Textarea, Dropdown, NumberInput } from '@ui'
import { Pencil, AlertTriangle } from 'lucide-react'
import { updateSeries, type Series } from '../api'
import { ChipsField } from './fields'

interface Props {
  series: Series
  onClose: () => void
}

const DIRECTIONS = ['ltr', 'rtl', 'vertical', 'webtoon'] as const
type Direction = (typeof DIRECTIONS)[number]

/** P4 — metadata editor for a series (admin only). */
export default function SeriesMetadataEditor({ series, onClose }: Props) {
  const { t } = useTranslation('books')
  const qc = useQueryClient()

  const [name, setName] = useState(series.name)
  const [sortName, setSortName] = useState(series.sort_name)
  const [description, setDescription] = useState(series.description ?? '')
  const [publisher, setPublisher] = useState(series.publisher ?? '')
  const [language, setLanguage] = useState(series.language ?? '')
  const [ageRating, setAgeRating] = useState(0)
  const [direction, setDirection] = useState<Direction>(
    (series.reading_direction as Direction) ?? 'ltr',
  )
  const [totalCount, setTotalCount] = useState(series.total_book_count ?? 0)
  const [genres, setGenres] = useState<string[]>([])
  const [tags, setTags] = useState<string[]>([])

  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const directionOptions = DIRECTIONS.map((d) => ({ value: d, label: t(`books_direction_${d}`) }))

  async function save() {
    setSaving(true)
    setError(null)
    try {
      await updateSeries(series.id, {
        name: name.trim(),
        sort_name: sortName.trim(),
        description: description.trim() || null,
        publisher: publisher.trim() || null,
        language: language.trim() || null,
        age_rating: ageRating || null,
        reading_direction: direction,
        total_book_count: totalCount || null,
        genres,
        tags,
      })
      void qc.invalidateQueries({ queryKey: ['books', 'series-detail', series.id] })
      void qc.invalidateQueries({ queryKey: ['books', 'series', series.library_id] })
      onClose()
    } catch {
      setError(t('books_save_error'))
      setSaving(false)
    }
  }

  return (
    <FloatingWindow
      title={t('books_edit_series_title')}
      icon={<Pencil className="h-4 w-4" />}
      onClose={onClose}
      defaultWidth={580}
      defaultHeight={560}
      backdrop
    >
      <div className="flex h-full flex-col" data-module="books">
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-5">
          <Input
            label={t('books_field_name')}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <Input
            label={t('books_field_sort_name')}
            value={sortName}
            onChange={(e) => setSortName(e.target.value)}
          />
          <Textarea
            label={t('books_field_description')}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            rows={3}
          />

          <div className="grid grid-cols-2 gap-4">
            <Input
              label={t('books_field_publisher')}
              value={publisher}
              onChange={(e) => setPublisher(e.target.value)}
            />
            <Input
              label={t('books_field_language')}
              value={language}
              onChange={(e) => setLanguage(e.target.value)}
              placeholder="fr, en…"
            />
            <NumberInput
              label={t('books_field_total_count')}
              value={totalCount}
              onChange={setTotalCount}
              min={0}
            />
            <NumberInput
              label={t('books_field_age_rating')}
              value={ageRating}
              onChange={setAgeRating}
              min={0}
              max={21}
            />
          </div>

          <div>
            <label className="mb-1.5 block text-sm font-medium text-text-secondary">
              {t('books_field_direction')}
            </label>
            <Dropdown
              value={direction}
              onChange={(v) => setDirection(v as Direction)}
              options={directionOptions}
              width="100%"
              height={36}
            />
          </div>

          <ChipsField label={t('books_field_genres')} value={genres} onChange={setGenres} />
          <ChipsField label={t('books_field_tags')} value={tags} onChange={setTags} />

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
          <Button variant="primary" onClick={save} loading={saving} disabled={!name.trim()}>
            {t('common_save')}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
