import type { ReactNode } from 'react'
import { ArrowLeft, Maximize2, Minimize2 } from 'lucide-react'

/**
 * Full-screen overlay chrome shared by every reader. Hosts a back button, the
 * book title, a free-form indicator (page X / N), trailing controls, and a
 * native-fullscreen toggle. The bar auto-hides via the `visible` flag.
 */
export function ReaderToolbar({
  title,
  onBack,
  indicator,
  controls,
  isFullscreen,
  onToggleFullscreen,
  visible,
}: {
  title: string
  onBack: () => void
  indicator?: ReactNode
  controls?: ReactNode
  isFullscreen: boolean
  onToggleFullscreen: () => void
  visible: boolean
}) {
  return (
    <div
      className={`pointer-events-none absolute inset-x-0 top-0 z-20 flex items-center gap-3 bg-gradient-to-b from-black/70 to-transparent px-4 py-3 transition-opacity duration-200 ${
        visible ? 'opacity-100' : 'opacity-0'
      }`}
    >
      <button
        type="button"
        onClick={onBack}
        className="pointer-events-auto flex h-9 w-9 items-center justify-center rounded-full bg-white/10 text-white hover:bg-white/20"
        aria-label="back"
      >
        <ArrowLeft className="h-5 w-5" />
      </button>
      <span className="pointer-events-auto min-w-0 flex-1 truncate text-sm font-medium text-white" title={title}>
        {title}
      </span>
      {indicator && (
        <span className="pointer-events-auto flex-shrink-0 text-xs text-white/80">{indicator}</span>
      )}
      {controls && (
        <div className="pointer-events-auto flex flex-shrink-0 items-center gap-2">{controls}</div>
      )}
      <button
        type="button"
        onClick={onToggleFullscreen}
        className="pointer-events-auto flex h-9 w-9 items-center justify-center rounded-full bg-white/10 text-white hover:bg-white/20"
        aria-label="fullscreen"
      >
        {isFullscreen ? <Minimize2 className="h-5 w-5" /> : <Maximize2 className="h-5 w-5" />}
      </button>
    </div>
  )
}

/** A compact <select> styled for the dark reader toolbar. */
export function ToolbarSelect<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T
  options: { value: T; label: string }[]
  onChange: (v: T) => void
  label?: string
}) {
  return (
    <select
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value as T)}
      className="h-8 rounded bg-white/10 px-2 text-xs text-white outline-none hover:bg-white/20 [&>option]:text-black"
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  )
}

/** A small icon button for the dark reader toolbar. */
export function ToolbarButton({
  onClick,
  children,
  title,
  active,
}: {
  onClick: () => void
  children: ReactNode
  title?: string
  active?: boolean
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={title}
      aria-label={title}
      className={`flex h-8 items-center gap-1 rounded px-2 text-xs text-white hover:bg-white/20 ${
        active ? 'bg-white/25' : 'bg-white/10'
      }`}
    >
      {children}
    </button>
  )
}
