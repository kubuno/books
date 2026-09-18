interface BooksLogoProps {
  size?:      number
  className?: string
  title?:     string
}

/** Books logo (designer artwork, raster). Served by the host from
 *  `/books-logo.png`; rendered as a square image so it weighs the same as its
 *  neighbours in the waffle menu. */
export function BooksLogo({ size = 24, className, title = 'Books' }: BooksLogoProps) {
  return (
    <img
      src="/books-logo.png"
      width={size}
      height={size}
      alt={title}
      className={className}
      style={{ display: 'block', objectFit: 'contain' }}
    />
  )
}

export default BooksLogo
