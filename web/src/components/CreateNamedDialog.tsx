import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { FloatingWindow, Button, Input, Textarea, Checkbox } from '@ui'
import { AlertTriangle, FolderPlus } from 'lucide-react'

interface Props {
  title: string
  /** Whether to show the "public" checkbox (collections / read lists). */
  showPublic?: boolean
  onClose: () => void
  onCreate: (dto: { name: string; description?: string; is_public?: boolean }) => Promise<void>
}

/**
 * Reusable create dialog for collections and read lists (name + description +
 * optional public flag).
 */
export default function CreateNamedDialog({ title, showPublic = true, onClose, onCreate }: Props) {
  const { t } = useTranslation('books')
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [isPublic, setIsPublic] = useState(false)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  async function submit() {
    if (!name.trim()) return
    setSaving(true)
    setError(null)
    try {
      await onCreate({
        name: name.trim(),
        description: description.trim() || undefined,
        ...(showPublic ? { is_public: isPublic } : {}),
      })
      onClose()
    } catch {
      setError(t('books_save_error'))
      setSaving(false)
    }
  }

  return (
    <FloatingWindow
      title={title}
      icon={<FolderPlus className="h-4 w-4" />}
      onClose={onClose}
      defaultWidth={460}
      defaultHeight={360}
      resizable={false}
      backdrop
    >
      <div className="flex flex-col gap-4 p-5" data-module="books">
        <Input
          label={t('books_field_name')}
          value={name}
          onChange={(e) => setName(e.target.value)}
          autoFocus
        />
        <Textarea
          label={t('books_field_description')}
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          rows={3}
        />
        {showPublic && (
          <Checkbox checked={isPublic} onChange={setIsPublic} label={t('books_field_public')} />
        )}

        {error && (
          <p className="flex items-center gap-1.5 text-sm text-danger">
            <AlertTriangle className="h-4 w-4 flex-shrink-0" />
            {error}
          </p>
        )}

        <div className="mt-2 flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose} disabled={saving}>
            {t('common_cancel')}
          </Button>
          <Button variant="primary" onClick={submit} loading={saving} disabled={!name.trim()}>
            {t('common_create')}
          </Button>
        </div>
      </div>
    </FloatingWindow>
  )
}
