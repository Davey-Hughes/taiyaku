import { useEffect, useRef } from 'react'

/**
 * The line that says the page asked for is not there, on every Configuration
 * screen drawn as pages. It takes focus when it is drawn: whatever had focus
 * went with the page it was on, or the person came by an address. Its caller
 * keys it by what the address asks for, a library's or an account's name or
 * a directory's number, so a second unknown address is a line drawn anew and
 * takes focus again.
 *
 * A file of its own, not `configPages.tsx`'s: a module that exports a
 * component exports nothing else, so that Fast Refresh can swap it.
 */
export function Missing({ children }: { children: string }) {
  const line = useRef<HTMLParagraphElement>(null)
  useEffect(() => {
    line.current?.focus()
  }, [])
  return (
    <p className="missing" ref={line} tabIndex={-1}>
      {children}
    </p>
  )
}
