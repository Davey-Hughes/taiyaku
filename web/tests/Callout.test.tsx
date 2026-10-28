import { act, useRef } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Callout } from '../src/Callout'
import { mount, rerender } from './support/mount'

/**
 * Where a callout is laid (`placeCallout`), read in jsdom, which lays
 * nothing out: each part of a fixture is drawn where its `data-rect` says
 * (top and foot, and its start and end where they are not the whole 400px),
 * and a callout where its own `top` and `height`, and its `left` and
 * `width` where it has them, put it in the part it is drawn in, as a
 * browser draws an absolute box. A ResizeObserver stands in for the
 * browser's, answered by `resized`.
 */
const px = (length: string) => Number.parseFloat(length) || 0

function drawnAt(this: Element) {
  let top = 0
  let bottom = 0
  let left = 0
  let right = 400
  if (this.matches('.callout')) {
    const frame = (this.parentElement as Element).getBoundingClientRect()
    const style = (this as HTMLElement).style
    top = frame.top + px(style.top)
    bottom = top + px(style.height)
    if (style.left !== '') {
      left = frame.left + px(style.left)
      right = left + px(style.width)
    }
  } else {
    ;[top = 0, bottom = 0, left = 0, right = 400] = (
      this.getAttribute('data-rect') ?? '0,0'
    )
      .split(',')
      .map(Number)
  }
  return {
    top,
    bottom,
    left,
    right,
    width: right - left,
    height: bottom - top,
    x: left,
    y: top,
    toJSON: () => ({}),
  } as DOMRect
}

let observers: Array<{ answer: () => void; observed: Element[] }> = []

/** Answers each ResizeObserver, as the browser does after a change of size. */
const resized = () =>
  act(() => {
    for (const each of observers) each.answer()
  })

beforeEach(() => {
  observers = []
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(
    drawnAt,
  )
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observed: Element[] = []
      answer: () => void
      constructor(callback: () => void) {
        this.answer = callback
        observers.push(this)
      }
      observe(el: Element) {
        this.observed.push(el)
      }
      disconnect() {
        observers = observers.filter((each) => each !== this)
      }
    },
  )
})

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

/**
 * A box's row: 12px of block padding and no edge, which jsdom, unlike a
 * browser, has to be told.
 */
const ROW = {
  paddingTop: '12px',
  paddingBottom: '12px',
  borderTopWidth: '0px',
  borderBottomWidth: '0px',
}

/**
 * A settings box of Current (0–50), Confirm (50–100), a row of help words
 * (100–180) and Change password's row (180–240, its button 193–227), its
 * callout drawn after Confirm saying `said`, `needs` tall.
 */
function passwordBox(said: string, needs = 40) {
  return (
    <div className="settings-box" data-rect="0,241">
      <form className="settings-group" data-rect="0,240">
        <div data-rect="0,50" style={ROW}>
          <input aria-label="Current" data-rect="12,38" />
        </div>
        <div data-rect="50,100" style={ROW}>
          <input aria-label="Confirm" data-rect="62,88" />
        </div>
        <span style={{ position: 'absolute' }} data-rect="100,101" />
        <Callout>
          <p role="status" data-rect={`0,${needs}`}>
            {said === '' ? null : said}
          </p>
        </Callout>
        <div data-rect="100,180" style={ROW}>
          words
        </div>
        <div data-rect="180,240" style={ROW}>
          <button type="submit" data-rect="193,227">
            Change password
          </button>
        </div>
      </form>
    </div>
  )
}

const calloutIn = (host: HTMLElement) =>
  host.querySelector('.callout') as HTMLElement

/** Where the callout is set to lie in its form: its top, and its foot. */
const laid = (host: HTMLElement) => {
  const { style } = calloutIn(host)
  return style.top === ''
    ? 'not laid out'
    : [px(style.top), px(style.top) + px(style.height)]
}

/**
 * API keys in a reader panel, which draws no box: the catalog's address
 * (0–56, Copy address 25–56) unless `address` is false, then the New key
 * row (64–107, or 0–43 first), a form laid out as a row, and a refused
 * Create's callout drawn after it, saying `needs` tall, or what is said as
 * a box is typed where `role` is a status. No key is listed, so no row
 * follows.
 */
function keysSection(needs: number, address = true, role = 'alert') {
  const at = address ? 64 : 0
  return (
    <section data-rect="0,107">
      {address && (
        <div data-rect="0,56">
          <code data-rect="0,21">https://example.org/opds</code>
          <button type="button" data-rect="25,56">
            Copy address
          </button>
        </div>
      )}
      <form
        className="row"
        style={{ display: 'flex' }}
        data-rect={`${at},${at + 43}`}
      >
        <input aria-label="New key" data-rect={`${at + 4},${at + 43}`} />
        <button type="submit" data-rect={`${at + 6},${at + 37}`}>
          Create
        </button>
      </form>
      <Callout>
        <p role={role} data-rect={`0,${needs}`}>
          a key needs a name
        </p>
      </Callout>
    </section>
  )
}

/** Where the callout is set to lie: its top, its foot, its start and end. */
const laidOver = (host: HTMLElement) => {
  const { style } = calloutIn(host)
  return [
    px(style.top),
    px(style.top) + px(style.height),
    px(style.left),
    px(style.left) + px(style.width),
  ]
}

describe('Callout', () => {
  /**
   * New account's form: Confirm password's row (0–60), the line saying the
   * two passwords differ laid `over` Role's select, Role's row (60–180) and
   * Add's (180–240). Beside one another, Role's label and help are in a
   * column to 200 and its select is 224–400, 118–142; `stacked`, they are
   * above a select as wide as the row, 138–168. The line's words are drawn
   * `words`: their height, and their width on one line. Declared in this
   * block because `useComponentExportOnlyModules` refuses an unexported
   * component at module scope.
   */
  function RoleForm({
    words,
    stacked = false,
  }: {
    words: string
    stacked?: boolean
  }) {
    const role = useRef<HTMLSelectElement>(null)
    return (
      <div className="settings-box" data-rect="0,241">
        <form className="settings-group" data-rect="0,240">
          <div className="settings-row" data-rect="0,60" style={ROW}>
            <input aria-label="Confirm" data-rect="12,48,224,400" />
          </div>
          <Callout over={role}>
            <p role="status" data-rect={words}>
              The two passwords do not match.
            </p>
          </Callout>
          <div className="settings-row" data-rect="60,180" style={ROW}>
            <div
              className="settings-row-text"
              data-rect={stacked ? '72,130,0,400' : '72,168,0,200'}
            >
              Role
            </div>
            <div className="settings-row-control">
              <select
                ref={role}
                aria-label="Role"
                data-rect={stacked ? '138,168,0,400' : '118,142,224,400'}
              />
            </div>
          </div>
          <div className="settings-actions" data-rect="180,240" style={ROW}>
            <button type="submit" data-rect="193,227">
              Add
            </button>
          </div>
        </form>
      </div>
    )
  }

  it('lies from the foot of the row it answers as far as its words need, where they end in a row’s padding', () => {
    const host = mount(passwordBox('short', 10))
    // To 100 + 10, inside the help row's top padding (100–112): none of its
    // content is reached, so none is covered.
    expect(laid(host)).toEqual([100, 110])
  })

  it('covers the content of each row its words reach into whole', () => {
    const host = mount(passwordBox('The two passwords do not match.'))
    // 100 + 40 reaches into the help row's content (112–168), so it covers
    // that content to its foot.
    expect(laid(host)).toEqual([100, 168])
    rerender(passwordBox('a sentence longer still', 95))
    // 100 + 95 reaches on into Change password's row's content (192–228),
    // which it covers whole, Change password with it.
    expect(laid(host)).toEqual([100, 228])
  })

  it('is pulled up, saying what is typed, where its words would cross the foot of the last row’s content, over the rows above, each whole', () => {
    const host = mount(passwordBox('a long refusal', 140))
    // 100 + 140 is past the foot of Change password's content, 228, so its
    // foot is 228 and its top 88, the foot of Confirm's content (62–88),
    // which it leaves as it is.
    expect(laid(host)).toEqual([88, 228])
    rerender(passwordBox('a longer refusal', 150))
    // 228 - 150 = 78, inside Confirm's content, which it covers whole.
    expect(laid(host)).toEqual([62, 228])
  })

  it('lies, a refusal that would cross the foot of the last row’s content, over the rows above the row it answers, each whole, its foot at that row’s top, where what is said as a box is typed is pulled up over that row', () => {
    const host = mount(keysSection(39))
    // 64 + 39 is past the foot of the New key row, the last, 107. Over the
    // rows above it instead: its foot at 64, the New key row's top, and its
    // top 64 - 39 = 25, inside the address's row (0–56), which it covers
    // whole, Copy address with it.
    expect(laid(host)).toEqual([0, 64])
    rerender(keysSection(39, true, 'status'))
    // What is said as a box is typed is pulled up over the row it answers,
    // as the line saying two passwords differ covers the held button.
    expect(laid(host)).toEqual([64, 107])
  })

  it('is pulled up over the row a refusal answers only where the rows above that row have no room for its words', () => {
    const host = mount(keysSection(39, false))
    // The New key row (0–43) is the section's only row, so nothing above it
    // has room: its foot is that row's foot, 43, and it reaches up over it
    // whole.
    expect(laid(host)).toEqual([0, 43])
  })

  it('lies over the control it is laid `over` alone, centred on it, as tall as its words need and as wide as it, or wider toward its row’s start where its words need it, as far as its label’s column', async () => {
    const host = mount(<RoleForm words="0,40,0,190" />)
    // Drawn before the select, it is laid out once the select is drawn, by
    // its section's ResizeObserver, and never over the rows meanwhile.
    expect(laid(host)).toBe('not laid out')
    await resized()
    // Its words, 40 tall, are centred on the select (118–142); 190 wide,
    // they need more than the select's 176, which room beside it (from
    // the label's column's end, 200) holds.
    expect(laidOver(host)).toEqual([110, 150, 210, 400])
    rerender(<RoleForm words="0,40,0,230" />)
    // Words 230 wide would reach over the label's column, so it ends there.
    expect(laidOver(host)).toEqual([110, 150, 200, 400])
    rerender(<RoleForm words="0,20,0,190" />)
    // Words shorter than the select: it is the select's height.
    expect(laidOver(host)).toEqual([118, 142, 210, 400])
    rerender(<RoleForm words="0,40,0,190" stacked />)
    // Under its label, the select is as wide as the row: so is the callout.
    expect(laidOver(host)).toEqual([133, 173, 0, 400])
  })

  it('takes no top and no height while it says nothing, and gives up those it had when it falls silent', () => {
    const host = mount(passwordBox(''))
    expect(laid(host)).toBe('not laid out')
    rerender(passwordBox('The two passwords do not match.'))
    expect(laid(host)).toEqual([100, 168])
    rerender(passwordBox(''))
    expect(laid(host)).toBe('not laid out')
  })

  it('is laid out again when its section changes size, with no render of its own', async () => {
    const host = mount(passwordBox('a longer sentence', 60))
    expect(laid(host)).toEqual([100, 168])
    const [observer] = observers
    expect(observer?.observed).toEqual([host.querySelector('.settings-box')])
    // The help row takes 100–150 as the box widens, its content 112–138,
    // and Change password's row moves up under it.
    const help = host.querySelector('[data-rect="100,180"]') as HTMLElement
    help.setAttribute('data-rect', '100,150')
    const change = help.nextElementSibling as HTMLElement
    change.setAttribute('data-rect', '150,210')
    await resized()
    expect(laid(host)).toEqual([100, 160])
  })

  it('is not laid out in a document with no ResizeObserver, which lays nothing out', () => {
    vi.stubGlobal('ResizeObserver', undefined)
    const host = mount(passwordBox('The two passwords do not match.'))
    expect(laid(host)).toBe('not laid out')
  })

  it('stops following its section once it is gone', () => {
    const host = mount(passwordBox('The two passwords do not match.'))
    expect(observers).toHaveLength(1)
    rerender(<div />)
    expect([observers, host.querySelector('.callout')]).toEqual([[], null])
  })

  it('covers a row that draws its own edge, and a control that is a row, to their edges', () => {
    const host = mount(
      <section data-rect="0,200">
        <form className="row" style={{ display: 'flex' }} data-rect="0,40">
          <input aria-label="New key" data-rect="0,40" />
        </form>
        <Callout>
          <p role="alert" data-rect="0,30">
            a key needs a name
          </p>
        </Callout>
        <div
          className="secret"
          style={{ ...ROW, borderBottomWidth: '1px' }}
          data-rect="40,100"
        >
          the key
        </div>
        <button type="button" style={ROW} data-rect="100,140">
          Revoke
        </button>
      </section>,
    )
    // The secret's box, 40–100, is covered to its edge, not its padding.
    expect(laid(host)).toEqual([40, 100])
    rerender(
      <section data-rect="0,200">
        <form className="row" style={{ display: 'flex' }} data-rect="0,40">
          <input aria-label="New key" data-rect="0,40" />
        </form>
        <Callout>
          <p role="alert" data-rect="0,70">
            a key needs a name, so the right one can be revoked later
          </p>
        </Callout>
        <div
          className="secret"
          style={{ ...ROW, borderBottomWidth: '1px' }}
          data-rect="40,100"
        >
          the key
        </div>
        <button type="button" style={ROW} data-rect="100,140">
          Revoke
        </button>
      </section>,
    )
    // On into Revoke, a control: to its edge, 140, past its padding.
    expect(laid(host)).toEqual([40, 140])
  })

  it('is not laid out where nothing is drawn before it, or it is in no section', () => {
    const host = mount(
      <>
        <section data-rect="0,100">
          <Callout>
            <p role="alert" data-rect="0,30">
              first in its section
            </p>
          </Callout>
          <div data-rect="0,100">a row</div>
        </section>
        <div data-rect="100,200">
          <div data-rect="100,150">a row</div>
          <Callout>
            <p role="alert" data-rect="0,30">
              in no section
            </p>
          </Callout>
        </div>
      </>,
    )
    expect(
      [...host.querySelectorAll<HTMLElement>('.callout')].map(
        (callout) => callout.style.top,
      ),
    ).toEqual(['', ''])
  })

  it('marks itself over focus while the control focused, and marked as focused, lies under it, and not once focus moves off it or it falls silent', () => {
    let keyboard = true
    const matches = Element.prototype.matches
    vi.spyOn(Element.prototype, 'matches').mockImplementation(function (
      this: Element,
      selector: string,
    ) {
      // jsdom marks no focus as seen; a key press would, here.
      if (selector === ':focus-visible') {
        return keyboard && this === document.activeElement
      }
      return matches.call(this, selector)
    })
    const host = mount(passwordBox('a sentence longer still', 95))
    const callout = calloutIn(host)
    const change = host.querySelector('button') as HTMLButtonElement
    const confirm = host.querySelector(
      'input[aria-label="Confirm"]',
    ) as HTMLInputElement
    const over = () => callout.hasAttribute('data-over-focus')
    expect(over()).toBe(false)
    // Change password, 193–227, under the callout, 100–228.
    act(() => change.focus())
    expect(over()).toBe(true)
    // Confirm, 62–88, above it.
    act(() => confirm.focus())
    expect(over()).toBe(false)
    keyboard = false
    act(() => change.focus())
    expect(over()).toBe(false)
    keyboard = true
    act(() => confirm.focus())
    act(() => change.focus())
    expect(over()).toBe(true)
    rerender(passwordBox(''))
    expect(over()).toBe(false)
  })
})
