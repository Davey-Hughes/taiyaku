import { useEffect, useState } from 'react'

/** How long what a copy did is drawn and said. */
export const RESULT_SHOWN_MS = 2000

export type CopyResult = 'Copied' | 'Not copied'

/**
 * Writing text to the clipboard from a press, for `CatalogAddress`'s Copy
 * address and `Account`'s one-time key.
 *
 * **`canCopy` is whether the Clipboard API can write**, which is a secure
 * context that has `navigator.clipboard`. A phone that reached this server
 * over plain http has neither, and each caller then draws its text to be
 * selected by hand and offers no copy. Both halves are asked, because a
 * secure context is not enough on its own: a browser may be in one and still
 * have no clipboard.
 *
 * **`said` is for a polite live region**: "Copied", or "Not copied" for a
 * refused write, once the write lands, and empty again `RESULT_SHOWN_MS`
 * later, so no result is left in it to be read later. The caller mounts the
 * region empty before anything is copied, because a screen reader speaks a
 * region's text changing and not a region arriving. `shown` is the same
 * result for a caller that also draws it, a new object for each result.
 *
 * `copy` answers the result as well, for a caller that does something more
 * on a refusal.
 */
export function useCopy() {
  const [said, setSaid] = useState('')
  // A new object for every result, so the clock below starts again for each
  // one. When both presses come before either result, the two results land
  // with nothing putting the state back between them; compared by its word,
  // the second "Copied" would end when the first one's two seconds did.
  const [shown, setShown] = useState<{ result: CopyResult } | undefined>()
  const canCopy = window.isSecureContext && navigator.clipboard !== undefined

  useEffect(() => {
    if (shown === undefined) return
    const timer = setTimeout(() => {
      setShown(undefined)
      setSaid('')
    }, RESULT_SHOWN_MS)
    // Cleared by the next result and by unmounting, so no timer sets the
    // state of a caller that is gone.
    return () => {
      clearTimeout(timer)
    }
  }, [shown])

  const copy = async (text: string): Promise<CopyResult> => {
    // Emptied as each write starts, so a second copy is spoken again: text
    // that stays "Copied" has not changed, and a live region says nothing.
    // An earlier press's write that lands while this one is pending still
    // says its result until this one lands or that result's two seconds end.
    setSaid('')
    setShown(undefined)
    let result: CopyResult
    try {
      // Called before anything is awaited, inside the press itself: a write
      // started after an `await` can find the press's user activation spent,
      // and Safari and Firefox refuse a write without one.
      await navigator.clipboard.writeText(text)
      result = 'Copied'
    } catch {
      result = 'Not copied'
    }
    setSaid(result)
    setShown({ result })
    return result
  }

  return { canCopy, said, shown, copy }
}
