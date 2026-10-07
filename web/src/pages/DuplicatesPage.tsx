import { useState } from 'react'
import { Link } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import { useQuery } from '@tanstack/react-query'
import { Button } from '@ui'
import { useAuthStore } from '@kubuno/sdk'
import { CopyCheck, Copy, Check, Rss, FileText } from 'lucide-react'
import { getDuplicates, OPDS_URL } from '../api'
import { Breadcrumb } from '../components/Breadcrumb'
import { LoadingState, EmptyState, PageHeader } from '../components/shared'

/** P7 — admin: duplicate groups (same content hash) + an OPDS feed callout. */
export default function DuplicatesPage() {
  const { t } = useTranslation('books')
  const user = useAuthStore((s) => s.user)
  const isAdmin = user?.role === 'admin'
  const [copied, setCopied] = useState(false)

  const { data: groups, isLoading } = useQuery({
    queryKey: ['books', 'duplicates'],
    queryFn: getDuplicates,
    enabled: isAdmin,
  })

  const opdsAbsolute = `${window.location.origin}${OPDS_URL}`

  function copyOpds() {
    void navigator.clipboard.writeText(opdsAbsolute).then(() => {
      setCopied(true)
      window.setTimeout(() => setCopied(false), 2000)
    })
  }

  if (!isAdmin) {
    return (
      <div className="mx-auto max-w-4xl p-6" data-module="books">
        <EmptyState icon={<CopyCheck className="h-10 w-10" />} message={t('books_admin_only')} />
      </div>
    )
  }

  return (
    <div className="mx-auto max-w-4xl p-6" data-module="books">
      <div className="mb-4">
        <Breadcrumb crumbs={[{ label: t('books_nav_duplicates') }]} />
      </div>
      <PageHeader title={t('books_nav_duplicates')} subtitle={t('books_duplicates_subtitle')} />

      {/* OPDS feed callout. */}
      <div className="mb-6 flex items-start gap-3 rounded-lg border border-border bg-surface-0 p-4">
        <span className="mt-0.5 flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-md bg-primary-light text-primary">
          <Rss className="h-5 w-5" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="text-sm font-medium text-text-primary">{t('books_opds_title')}</p>
          <p className="text-xs text-text-secondary">{t('books_opds_hint')}</p>
          <code className="mt-1.5 block truncate rounded bg-surface-2 px-2 py-1 text-xs text-text-secondary">
            {opdsAbsolute}
          </code>
        </div>
        <Button
          variant="secondary"
          icon={copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
          onClick={copyOpds}
        >
          {copied ? t('books_copied') : t('books_copy')}
        </Button>
      </div>

      {isLoading ? (
        <LoadingState label={t('books_loading')} />
      ) : groups && groups.length > 0 ? (
        <div className="flex flex-col gap-3">
          {groups.map((g) => (
            <div key={g.hash} className="rounded-lg border border-border bg-surface-0 p-4">
              <p className="mb-2 flex items-center gap-2 text-xs text-text-tertiary">
                <CopyCheck className="h-3.5 w-3.5" />
                {t('books_duplicate_group', { count: g.books.length })}
                <code className="rounded bg-surface-2 px-1.5 py-0.5">{g.hash.slice(0, 12)}</code>
              </p>
              <ul className="space-y-1">
                {g.books.map((b) => (
                  <li key={b.id}>
                    <Link
                      to={`/books/book/${b.id}`}
                      className="flex items-center gap-2 rounded px-2 py-1 text-sm text-text-primary transition hover:bg-surface-1 hover:text-primary"
                    >
                      <FileText className="h-4 w-4 flex-shrink-0 text-text-tertiary" />
                      <span className="truncate">{b.title}</span>
                    </Link>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      ) : (
        <EmptyState icon={<CopyCheck className="h-10 w-10" />} message={t('books_no_duplicates')} />
      )}
    </div>
  )
}
