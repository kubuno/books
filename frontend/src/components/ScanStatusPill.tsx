import { useTranslation } from 'react-i18next'
import { Loader2, AlertTriangle, CheckCircle2 } from 'lucide-react'
import type { ScanStatus } from '../api'

/** Inline pill reflecting a library's scan state. */
export default function ScanStatusPill({ status }: { status: ScanStatus }) {
  const { t } = useTranslation('books')

  if (status === 'scanning') {
    return (
      <span className="inline-flex items-center gap-1 rounded-full bg-warning-light px-2 py-0.5 text-[11px] font-medium text-warning">
        <Loader2 className="h-3 w-3 animate-spin" />
        {t('books_scan_scanning')}
      </span>
    )
  }
  if (status === 'error') {
    return (
      <span className="inline-flex items-center gap-1 rounded-full bg-danger-light px-2 py-0.5 text-[11px] font-medium text-danger">
        <AlertTriangle className="h-3 w-3" />
        {t('books_scan_error')}
      </span>
    )
  }
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-success-light px-2 py-0.5 text-[11px] font-medium text-success">
      <CheckCircle2 className="h-3 w-3" />
      {t('books_scan_idle')}
    </span>
  )
}
