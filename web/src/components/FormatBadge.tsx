/** Small colored pill for a file format (CBZ/CBR/CB7/PDF/EPUB…). */

const FORMAT_COLORS: Record<string, { bg: string; fg: string }> = {
  cbz: { bg: '#e6f4ea', fg: '#1e8e3e' },
  cbr: { bg: '#fef7e0', fg: '#b06000' },
  cb7: { bg: '#fce8e6', fg: '#c5221f' },
  pdf: { bg: '#fde8e8', fg: '#d93025' },
  epub: { bg: '#e8f0fe', fg: '#1a73e8' },
  mobi: { bg: '#f3e8fd', fg: '#8430ce' },
  azw3: { bg: '#f3e8fd', fg: '#8430ce' },
}

const DEFAULT_COLOR = { bg: '#f1f3f4', fg: '#5f6368' }

export default function FormatBadge({ format }: { format: string }) {
  const key = format.toLowerCase()
  const { bg, fg } = FORMAT_COLORS[key] ?? DEFAULT_COLOR
  return (
    <span
      className="inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide"
      style={{ backgroundColor: bg, color: fg }}
    >
      {format}
    </span>
  )
}
