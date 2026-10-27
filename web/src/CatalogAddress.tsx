/**
 * The OPDS catalog's address, and a button that copies it: on `/about`, and
 * beside the OPDS line in both of `Account`'s placements.
 *
 * **The address is this page's origin before the path `/api/about`
 * answers**, so it is the address the reader reached this server at, and the
 * web never writes the OPDS version into a path of its own.
 *
 * **The address is always drawn as text; the button only where the Clipboard
 * API can write** (`useCopy`'s `canCopy`). Elsewhere the address is selected
 * by hand.
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
 * it is mounted with the button and empty until then. Both wait for the
 * write to land, and both go when `useCopy`'s result does.
 *
 * `className` is the button's, so each placement draws it as it draws its
 * own buttons: the reader's panel hands it `reader-button`, and the settings
 * area hands it nothing, since `base.css` draws every button there.
 *
 * **Where the address could not be read (`catalog` is `null`), the row says
 * so in its place; while it is being read (`undefined`), the row is empty.**
 * With `keepsButtonRoom`, a row that draws no Copy address, for either of
 * those or for a clipboard that cannot be written, keeps the room one takes
 * with an unseen button of the same words and look, so the row is in every
 * state at least as tall as a line holding Copy address, and what lies over
 * it, such as Account's refused Create, has that room.
 */
import { type ReactNode, useId } from 'react'
import { type CopyResult, useCopy } from './copy'

export type CatalogAddressProps = {
  /**
   * The catalog's path on this server, as `/api/about` answers it; `null`
   * where it could not be read, and `undefined` while it is being read.
   */
  catalog: string | null | undefined
  /** The Copy address button's class. */
  className?: string
  /** Whether a row with no Copy address keeps the room one takes. */
  keepsButtonRoom?: boolean
}

/** Which of the button's words `admin.css` draws for each result. */
const SHOWS: Record<CopyResult, string> = {
  Copied: 'copied',
  'Not copied': 'not-copied',
}

/**
 * The words of a button that copies, drawn in one cell with what a press
 * did, "Copied" or "Not copied", which take their place while `shown`
 * holds a result (`useCopy`), so that nothing around them moves
 * (`admin.css`): Copy address's word, and the shown key's caption. The
 * results are generated content of a box hidden from assistive technology,
 * so the words, which `id` names, stay the button's name throughout.
 */
export function CopyWords({
  id,
  className,
  shown,
  children,
}: {
  id?: string
  className?: string
  shown: { result: CopyResult } | undefined
  children: ReactNode
}) {
  return (
    <span
      className={`copy-words ${className ?? ''}`.trim()}
      data-shows={shown === undefined ? undefined : SHOWS[shown.result]}
    >
      <span id={id} className="copy-word">
        {children}
      </span>
      <span
        className="copy-results"
        aria-hidden="true"
        data-copied="Copied"
        data-not-copied="Not copied"
      />
    </span>
  )
}

/** What the row says where the catalog's address could not be read. */
const ADDRESS_UNREAD = 'The catalog’s address could not be read.'

export function CatalogAddress({
  catalog,
  className,
  keepsButtonRoom = false,
}: CatalogAddressProps) {
  const { canCopy, said, shown, copy } = useCopy()
  const addressId = useId()
  const address =
    typeof catalog === 'string' ? `${location.origin}${catalog}` : undefined

  return (
    <div className="catalog-address">
      {address !== undefined ? (
        <code id={addressId} className="catalog-address-text">
          {address}
        </code>
      ) : catalog === null ? (
        <span>{ADDRESS_UNREAD}</span>
      ) : null}
      {canCopy && address !== undefined ? (
        <>
          <button
            type="button"
            className={className}
            aria-describedby={addressId}
            onClick={() => {
              void copy(address)
            }}
          >
            <CopyWords shown={shown}>Copy address</CopyWords>
          </button>
          <span role="status" className="visually-hidden">
            {said}
          </span>
        </>
      ) : keepsButtonRoom ? (
        // Unseen and out of reach: it keeps the room Copy address takes.
        <button
          type="button"
          className={className}
          aria-hidden="true"
          tabIndex={-1}
          disabled
        >
          <CopyWords shown={undefined}>Copy address</CopyWords>
        </button>
      ) : null}
    </div>
  )
}
