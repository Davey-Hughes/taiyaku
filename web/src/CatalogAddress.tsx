/**
 * The OPDS catalog's address, and a button that copies it: on `/about`, and
 * beside the OPDS line in both of `Account`'s placements.
 *
 * **The address is this page's origin before the path `/api/about`
 * answers**, so it is the address the reader reached this server at, and the
 * web never writes the OPDS version into a path of its own.
 *
 * **The address is always drawn as text; the button only where the Clipboard
 * API can write**, which is a secure context that has `navigator.clipboard`.
 * A phone that reached this server over plain http has neither, and selects
 * the address by hand. Both halves are asked, because a secure context is not
 * enough on its own: a browser may be in one and still have no clipboard.
 *
 * **The button says what it copies, and the address is its description**
 * (`aria-describedby`): on Account it sits two rows above a fresh API key's
 * secret, which is shown once, and a button saying only "Copy" there reads
 * as the secret's. The id comes from `useId`, so each placement's button is
 * described by its own address.
 *
 * **What a press did is drawn on the button and said in a polite live
 * region.** The button's word is "Copied", or "Not copied" for a refused
 * write, for `RESULT_SHOWN_MS`, and then "Copy address" again. It keeps the
 * width of the widest of its three words, so nothing around it moves
 * (`admin.css`). Its text, and so its name, is "Copy address" in every state:
 * the result is spoken once, by the region, and not again by a name that
 * changed. The region is visually hidden, so it takes no room on the page;
 * it is mounted with the button and empty until then, because a screen
 * reader speaks a region's text changing and not a region arriving. Both
 * wait for the write to land, and the region is emptied when the word goes
 * back, so no result is left in it to be read later.
 *
 * `className` is the button's, so each placement draws it as it draws its
 * own buttons: the reader's panel hands it `reader-button`, and the settings
 * area hands it nothing, since `base.css` draws every button there.
 */
import { useEffect, useId, useState } from 'react'

export type CatalogAddressProps = {
  /** The catalog's path on this server, as `/api/about` answers it. */
  catalog: string
  /** The Copy address button's class. */
  className?: string
}

/** How long the button says what a press did. */
const RESULT_SHOWN_MS = 2000

type Result = 'Copied' | 'Not copied'

/** Which of the button's words `admin.css` draws for each result. */
const SHOWS: Record<Result, string> = {
  Copied: 'copied',
  'Not copied': 'not-copied',
}

export function CatalogAddress({ catalog, className }: CatalogAddressProps) {
  const [said, setSaid] = useState('')
  // A new object for every result, so the clock below starts again for each
  // one. When both presses come before either result, the two results land
  // with nothing putting the button back between them; compared by its
  // word, the second "Copied" would end when the first one's two seconds
  // did.
  const [shown, setShown] = useState<{ result: Result } | undefined>()
  const addressId = useId()
  const address = `${location.origin}${catalog}`
  const canCopy = window.isSecureContext && navigator.clipboard !== undefined

  useEffect(() => {
    if (shown === undefined) return
    const timer = setTimeout(() => {
      setShown(undefined)
      // The region is emptied with the word, so a screen reader reading the
      // page later finds no result left after the button.
      setSaid('')
    }, RESULT_SHOWN_MS)
    // Cleared by the next result and by unmounting, so no timer sets the
    // state of a button that is gone.
    return () => {
      clearTimeout(timer)
    }
  }, [shown])

  const copy = async () => {
    // Emptied as each write starts, so a second copy is spoken again: text
    // that stays "Copied" has not changed, and a live region says nothing.
    // The button says "Copy address" again with it. An earlier press's write
    // that lands while this one is pending still draws and says its result
    // until this one lands or that result's two seconds end.
    setSaid('')
    setShown(undefined)
    let result: Result
    try {
      // Called before anything is awaited, inside the click itself: a write
      // started after an `await` can find the click's user activation spent,
      // and Safari and Firefox refuse a write without one.
      await navigator.clipboard.writeText(address)
      result = 'Copied'
    } catch {
      result = 'Not copied'
    }
    setSaid(result)
    setShown({ result })
  }

  return (
    <div className="catalog-address">
      <code id={addressId} className="catalog-address-text">
        {address}
      </code>
      {canCopy ? (
        <>
          <button
            type="button"
            className={className}
            aria-describedby={addressId}
            onClick={() => {
              void copy()
            }}
          >
            <span
              className="catalog-address-words"
              data-shows={shown === undefined ? undefined : SHOWS[shown.result]}
            >
              <span className="catalog-address-word">Copy address</span>
              {/* The results are this box's generated content, and it is
                  hidden from assistive technology, so neither is the
                  button's text nor part of its name. */}
              <span
                className="catalog-address-results"
                aria-hidden="true"
                data-copied="Copied"
                data-not-copied="Not copied"
              />
            </span>
          </button>
          <span role="status" className="visually-hidden">
            {said}
          </span>
        </>
      ) : null}
    </div>
  )
}
