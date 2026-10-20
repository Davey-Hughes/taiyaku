import process from 'node:process'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { CatalogAddress } from './CatalogAddress'
import { clipboardContext } from './testing/clipboard'
import { mount } from './testing/mount'

// Another version than today's, so an address the component wrote for
// itself cannot pass for the one it was handed.
const CATALOG = '/opds/v1.3/catalog'
const ADDRESS = `${location.origin}${CATALOG}`

/** A `writeText` whose write lands only when the test says so. */
function heldWrite() {
  let land: (() => void) | undefined
  const writeText = vi.fn(
    () =>
      new Promise<void>((resolve) => {
        land = resolve
      }),
  )
  return {
    writeText,
    land: () => {
      if (!land) throw new Error('nothing was written')
      land()
    },
  }
}

/** Lets every pending promise settle, inside `act`. */
async function settle() {
  await act(async () => {
    await new Promise((resolve) => {
      setTimeout(resolve, 0)
    })
  })
}

function code(host: HTMLElement): HTMLElement {
  const all = host.querySelectorAll('code')
  if (all.length !== 1) throw new Error(`${all.length} addresses, not one`)
  return all[0] as HTMLElement
}

function copyButton(host: HTMLElement): HTMLButtonElement {
  const button = host.querySelector('button')
  if (!button) throw new Error('no Copy address button')
  return button
}

function region(host: HTMLElement): HTMLElement {
  const status = host.querySelector<HTMLElement>('[role="status"]')
  if (!status) throw new Error('no live region')
  return status
}

/**
 * The word the button is drawn saying, as the component marks it for
 * `admin.css` to draw (`About.browser.test.tsx` reads what is painted):
 * "Copy address" unless its words say a result, and then the result's own
 * word. A button with no such words is drawn saying its text.
 */
function shownWord(host: HTMLElement): string {
  const button = copyButton(host)
  const words = button.querySelector<HTMLElement>('.catalog-address-words')
  const results = button.querySelector<HTMLElement>('.catalog-address-results')
  if (!words || !results) return button.textContent
  switch (words.dataset.shows) {
    case undefined:
      return button.textContent
    case 'copied':
      return results.dataset.copied ?? ''
    case 'not-copied':
      return results.dataset.notCopied ?? ''
    default:
      throw new Error(`the button shows "${words.dataset.shows}"`)
  }
}

describe('CatalogAddress', () => {
  it.each([
    ['outside a secure context', false, { writeText: vi.fn() }],
    ['without a clipboard', true, undefined],
    ['with neither', false, undefined],
    ['beside the Copy address button', true, { writeText: vi.fn() }],
  ] as const)(
    'draws the address as text, this page’s origin before the path it is handed, %s',
    (_, secure, clipboard) => {
      clipboardContext(secure, clipboard)
      const host = mount(<CatalogAddress catalog={CATALOG} />)
      const address = code(host)
      expect(address.textContent).toBe(ADDRESS)
      expect(address.closest('button')).toBeNull()
    },
  )

  it('draws no Copy address button outside a secure context, though there is a clipboard', () => {
    clipboardContext(false, { writeText: vi.fn() })
    const host = mount(<CatalogAddress catalog={CATALOG} />)
    expect(host.querySelector('button')).toBeNull()
    expect(host.querySelector('[role="status"]')).toBeNull()
  })

  it('draws no Copy address button without a clipboard, though the context is secure', () => {
    clipboardContext(true, undefined)
    const host = mount(<CatalogAddress catalog={CATALOG} />)
    expect(host.querySelector('button')).toBeNull()
    expect(host.querySelector('[role="status"]')).toBeNull()
  })

  /**
   * **The live region is there, empty, before anything is copied.** A
   * screen reader speaks a region's text changing, not a region arriving
   * with its text already in it, so one mounted by the click would say
   * nothing.
   */
  it('draws Copy address, and mounts an empty live region with it, in a secure context with a clipboard', () => {
    clipboardContext(true, { writeText: vi.fn() })
    const host = mount(<CatalogAddress catalog={CATALOG} />)
    const button = copyButton(host)
    expect(button.textContent).toBe('Copy address')
    expect(button.type).toBe('button')
    expect(region(host).textContent).toBe('')
  })

  /**
   * **Each button is named for what it copies and described by the address
   * beside it**, not by another placement's: two on one page, holding two
   * catalogs, so an id shared between placements would describe the second
   * button by the first one's address. The name is the button's own text,
   * with no `aria-label` or `aria-labelledby` to put another in its place.
   */
  it('names each button "Copy address" and describes it by its own address', () => {
    clipboardContext(true, { writeText: vi.fn() })
    const host = mount(
      <>
        <CatalogAddress catalog={CATALOG} />
        <CatalogAddress catalog="/opds/v1.4/catalog" />
      </>,
    )
    const drawn = [...host.querySelectorAll('.catalog-address')].map(
      (placement) => {
        const button = copyButton(placement as HTMLElement)
        const described = (button.getAttribute('aria-describedby') ?? '')
          .split(' ')
          .map((id) => document.getElementById(id))
        return {
          name: button.textContent,
          renamed: ['aria-label', 'aria-labelledby'].filter((attr) =>
            button.hasAttribute(attr),
          ),
          description: described.map((el) => el?.textContent),
          ownAddress: described.map(
            (el) => el !== null && el === placement.querySelector('code'),
          ),
        }
      },
    )
    expect(drawn).toEqual([
      {
        name: 'Copy address',
        renamed: [],
        description: [ADDRESS],
        ownAddress: [true],
      },
      {
        name: 'Copy address',
        renamed: [],
        description: [`${location.origin}/opds/v1.4/catalog`],
        ownAddress: [true],
      },
    ])
  })

  it.each([
    ['reader-button', 'reader-button'],
    [undefined, null],
  ] as const)(
    'gives the button the class it is handed: %s',
    (className, drawn) => {
      clipboardContext(true, { writeText: vi.fn() })
      const host = mount(
        <CatalogAddress catalog={CATALOG} className={className} />,
      )
      expect(copyButton(host).getAttribute('class')).toBe(drawn)
    },
  )

  it('writes exactly the address, and says "Copied" once the write has landed', async () => {
    const write = heldWrite()
    clipboardContext(true, { writeText: write.writeText })
    const host = mount(<CatalogAddress catalog={CATALOG} />)

    act(() => {
      copyButton(host).click()
    })
    // Read before anything here awaits, so it pins the write to the click
    // itself: one started after an `await` would not have been made yet,
    // and in Safari and Firefox would have lost the click's user activation.
    expect(write.writeText.mock.calls).toEqual([[ADDRESS]])
    await settle()
    // Not before: "Copied" while the write is pending would claim a write
    // that may yet be refused.
    expect(region(host).textContent).toBe('')

    write.land()
    await settle()
    expect(region(host).textContent).toBe('Copied')
  })

  /**
   * **Emptied as each write starts**, so a second copy is spoken again: a
   * region whose text stays "Copied" has not changed, and says nothing.
   */
  it('empties what it said when the next write starts', async () => {
    const write = heldWrite()
    clipboardContext(true, { writeText: write.writeText })
    const host = mount(<CatalogAddress catalog={CATALOG} />)
    act(() => {
      copyButton(host).click()
    })
    write.land()
    await settle()
    expect(region(host).textContent).toBe('Copied')

    act(() => {
      copyButton(host).click()
    })
    await settle()
    expect(write.writeText).toHaveBeenCalledTimes(2)
    expect(region(host).textContent).toBe('')
  })

  /**
   * **A refusal is said, and goes no further.** A browser may refuse the
   * write — a permission prompt answered no, a document without focus — and
   * a rejection the button let go of would reach the page as an unhandled
   * one while the region said nothing.
   */
  it('says "Not copied" when the write is refused, and throws nothing', async () => {
    const unhandled: unknown[] = []
    const record = (reason: unknown) => {
      unhandled.push(reason)
    }
    process.on('unhandledRejection', record)
    try {
      const writeText = vi.fn(() =>
        Promise.reject(
          new DOMException('Write permission denied.', 'NotAllowedError'),
        ),
      )
      clipboardContext(true, { writeText })
      const host = mount(<CatalogAddress catalog={CATALOG} />)
      act(() => {
        copyButton(host).click()
      })
      await settle()
      await settle()
      expect(writeText.mock.calls).toEqual([[ADDRESS]])
      expect(region(host).textContent).toBe('Not copied')
    } finally {
      process.off('unhandledRejection', record)
    }
    expect(unhandled).toEqual([])
  })
})

/**
 * Settles what a write's answer started, inside `act`, where `settle` cannot
 * run: its `setTimeout` is one of the faked ones.
 */
async function settleNow() {
  await act(async () => {})
}

/**
 * **The button says what a press did, for about two seconds, and then what
 * it copies again.** Its text, which is its name, says "Copy address" all the
 * while: the live region is what says the result.
 */
describe('Copy address’s word', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  const fakeTimers = () => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  }
  const advance = (ms: number) => {
    act(() => {
      vi.advanceTimersByTime(ms)
    })
  }

  it('says "Copied" once the write lands, and "Copy address" again two seconds later', async () => {
    fakeTimers()
    const write = heldWrite()
    clipboardContext(true, { writeText: write.writeText })
    const host = mount(<CatalogAddress catalog={CATALOG} />)
    act(() => {
      copyButton(host).click()
    })
    await settleNow()
    // Not while the write is pending, for the reason the region waits.
    expect(shownWord(host)).toBe('Copy address')

    write.land()
    await settleNow()
    expect(shownWord(host)).toBe('Copied')
    expect(copyButton(host).textContent).toBe('Copy address')
    expect(region(host).textContent).toBe('Copied')
    advance(1900)
    expect(shownWord(host)).toBe('Copied')
    advance(200)
    expect(shownWord(host)).toBe('Copy address')
  })

  it('says "Not copied" when the write is refused, and "Copy address" again two seconds later', async () => {
    fakeTimers()
    const writeText = vi.fn(() =>
      Promise.reject(new DOMException('Refused.', 'NotAllowedError')),
    )
    clipboardContext(true, { writeText })
    const host = mount(<CatalogAddress catalog={CATALOG} />)
    act(() => {
      copyButton(host).click()
    })
    await settleNow()
    expect(shownWord(host)).toBe('Not copied')
    expect(copyButton(host).textContent).toBe('Copy address')
    expect(region(host).textContent).toBe('Not copied')
    advance(1900)
    expect(shownWord(host)).toBe('Not copied')
    advance(200)
    expect(shownWord(host)).toBe('Copy address')
  })

  /**
   * **A second press restarts the clock**, though its result is the same
   * word: the first result's two seconds would otherwise end the second's
   * half a second in.
   */
  it('says the second press’s result for two seconds from when it lands', async () => {
    fakeTimers()
    const writeText = vi.fn(() => Promise.resolve())
    clipboardContext(true, { writeText })
    const host = mount(<CatalogAddress catalog={CATALOG} />)
    act(() => {
      copyButton(host).click()
    })
    await settleNow()
    expect(shownWord(host)).toBe('Copied')
    advance(1500)
    act(() => {
      copyButton(host).click()
    })
    await settleNow()
    expect(writeText).toHaveBeenCalledTimes(2)
    expect(shownWord(host)).toBe('Copied')
    advance(1000)
    expect(shownWord(host)).toBe('Copied')
    advance(1100)
    expect(shownWord(host)).toBe('Copy address')
  })

  /**
   * **Two presses whose writes overlap**: the first write's "Copied" is drawn
   * while the second is still pending, and the second's "Copied", landing
   * 1.5 s later, still gets two seconds of its own. Nothing between the two
   * results puts the button back to "Copy address", so only a new value for
   * each result, and not its word, starts the clock again.
   */
  it('gives the later of two overlapping writes its own two seconds, though both say "Copied"', async () => {
    fakeTimers()
    const lands: (() => void)[] = []
    const writeText = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          lands.push(resolve)
        }),
    )
    clipboardContext(true, { writeText })
    const host = mount(<CatalogAddress catalog={CATALOG} />)
    act(() => {
      copyButton(host).click()
    })
    act(() => {
      copyButton(host).click()
    })
    await settleNow()
    expect(lands).toHaveLength(2)

    lands[0]?.()
    await settleNow()
    expect(shownWord(host)).toBe('Copied')
    advance(1500)
    lands[1]?.()
    await settleNow()
    advance(1000)
    expect(shownWord(host)).toBe('Copied')
    advance(1100)
    expect(shownWord(host)).toBe('Copy address')
  })

  /**
   * **The live region is emptied when the button goes back**, so a screen
   * reader reading the page later, past the button, finds no result from a
   * press two seconds or more ago. Emptied, not removed: the next press's
   * result is a change to its text, and is spoken.
   */
  it('empties the live region when the button says "Copy address" again', async () => {
    fakeTimers()
    clipboardContext(true, { writeText: vi.fn(() => Promise.resolve()) })
    const host = mount(<CatalogAddress catalog={CATALOG} />)
    act(() => {
      copyButton(host).click()
    })
    await settleNow()
    expect(region(host).textContent).toBe('Copied')
    advance(1900)
    expect(region(host).textContent).toBe('Copied')
    advance(200)
    expect([shownWord(host), region(host).textContent]).toEqual([
      'Copy address',
      '',
    ])

    act(() => {
      copyButton(host).click()
    })
    await settleNow()
    expect(region(host).textContent).toBe('Copied')
  })

  /**
   * **Nothing it started outlives it**: unmounted while it says a result,
   * it leaves no timer to set its state once it is gone.
   */
  it('leaves no timer behind when it is unmounted while it says a result', async () => {
    fakeTimers()
    clipboardContext(true, { writeText: vi.fn(() => Promise.resolve()) })
    const errors = vi.spyOn(console, 'error').mockImplementation(() => {})
    try {
      const host = document.createElement('div')
      document.body.append(host)
      const root = createRoot(host)
      act(() => {
        root.render(<CatalogAddress catalog={CATALOG} />)
      })
      act(() => {
        copyButton(host).click()
      })
      await settleNow()
      expect(shownWord(host)).toBe('Copied')
      expect(vi.getTimerCount()).toBe(1)

      act(() => {
        root.unmount()
      })
      host.remove()
      expect(vi.getTimerCount()).toBe(0)
      advance(2100)
      expect(errors).not.toHaveBeenCalled()
    } finally {
      errors.mockRestore()
    }
  })
})
