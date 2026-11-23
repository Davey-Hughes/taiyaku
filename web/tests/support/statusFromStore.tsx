import type { ReactNode } from 'react'
import type { LibraryStatus } from '../../src/library'
import { LIBRARY_STATUS } from '../../src/pageReads'
import { STATUS_FAILED, StatusContext } from '../../src/shell'
import { usePage } from '../../src/usePage'

/**
 * Hands the screens under it the library status read through the store, as
 * `Shell` does: a status handed to `StatusContext` outright is never read
 * again, so neither a scan's end nor a screen's own `invalidate` of it could
 * change what the screen draws.
 */
export function StatusFromStore({ children }: { children: ReactNode }) {
  const {
    answers: [status],
    failures: [failure],
  } = usePage<[LibraryStatus]>([LIBRARY_STATUS])
  return (
    <StatusContext.Provider
      value={status ?? (failure === undefined ? undefined : STATUS_FAILED)}
    >
      {children}
    </StatusContext.Provider>
  )
}
