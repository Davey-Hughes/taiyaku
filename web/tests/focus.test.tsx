import { act, useEffect, useRef } from 'react'
import { afterEach, describe, expect, it } from 'vitest'
import {
  barStops,
  barsMounted,
  controlStops,
  focusStop,
  ringAt,
  ringSizes,
  usePanelFocus,
} from '../src/focus'
import { mount, rerender } from './support/mount'

/**
 * A reader's skeleton: the page area with a gap's move inside it, the top bar,
 * a strip of three tiles of which one is the stop, and a note list below.
 */
function skeleton({ bars = true } = {}): HTMLElement {
  const root = document.createElement('div')
  root.innerHTML = `
    <div data-reader-page tabindex="-1">
      <button id="move">take the nearest page</button>
    </div>
    <div data-ring="controls"><button id="note">a note</button></div>
    ${
      bars
        ? `<div data-ring="top">
             <button id="library">Library</button>
             <button id="gear" disabled>Settings</button>
           </div>
           <div data-ring="bottom">
             <button id="t0" tabindex="-1">1</button>
             <button id="t1">2</button>
             <button id="t2" tabindex="-1">3</button>
           </div>`
        : ''
    }`
  document.body.append(root)
  return root
}

afterEach(() => {
  document.body.innerHTML = ''
})

const ids = (els: HTMLElement[]) => els.map((el) => el.id)

describe('the ring’s stops, read off the page', () => {
  it('the bars: the top edge, then the strip’s one stop, skipping what is disabled', () => {
    expect(ids(barStops(skeleton()))).toEqual(['library', 't1'])
  })

  it('the page’s own controls: inside the page area, then the note list', () => {
    expect(ids(controlStops(skeleton()))).toEqual(['move', 'note'])
  })

  it('counts the bars only while they are mounted', () => {
    expect(barsMounted(skeleton())).toBe(true)
    expect(ringSizes(skeleton({ bars: false }))).toEqual({
      bars: undefined,
      controls: 2,
    })
  })

  it('places focus: a bar stop, a control, or the page for anything else', () => {
    const root = skeleton()
    expect(ringAt(root, root.querySelector('#t1'))).toEqual({
      region: 'bars',
      index: 1,
    })
    expect(ringAt(root, root.querySelector('#note'))).toEqual({
      region: 'controls',
      index: 1,
    })
    expect(ringAt(root, document.body)).toEqual({ region: 'page', index: 0 })
  })

  it('focuses a stop, -1 being the last, and the page area for the page', () => {
    const root = skeleton()
    focusStop(root, { region: 'bars', index: -1 })
    expect(document.activeElement?.id).toBe('t1')
    focusStop(root, { region: 'page', index: 0 })
    expect(document.activeElement).toBe(
      root.querySelector('[data-reader-page]'),
    )
  })

  it('falls back to the page area when a stop refuses focus', () => {
    const root = skeleton()
    const stop = root.querySelector<HTMLElement>('#t1')
    if (stop) stop.focus = () => {}
    focusStop(root, { region: 'bars', index: -1 })
    expect(document.activeElement).toBe(
      root.querySelector('[data-reader-page]'),
    )
  })
})

/**
 * `Panel` and `Reader`, nested inside a function rather than declared at
 * module scope — where `style/useComponentExportOnlyModules` and
 * `suspicious/noExportsInTest` meet, the same reason `Gate.test.tsx`'s
 * `probe` gives.
 */
function readerFixture() {
  /** A panel the way the reader's are: `null` while closed, focus handled by the hook. */
  function Panel({
    open,
    typing = false,
  }: {
    open: boolean
    typing?: boolean
  }) {
    const ref = useRef<HTMLDivElement>(null)
    const text = useRef<HTMLTextAreaElement>(null)
    usePanelFocus(ref, open)
    // Declared after the hook, as `NotePanel`'s textarea effect must be.
    useEffect(() => {
      if (open && typing) text.current?.focus()
    }, [open, typing])
    if (!open) return null
    return (
      <div role="dialog" aria-label="Panel" ref={ref} tabIndex={-1}>
        <button type="button">first</button>
        {typing && <textarea ref={text} />}
        <button type="button">last</button>
      </div>
    )
  }

  function Reader({
    open,
    typing,
    opener = true,
  }: {
    open: boolean
    typing?: boolean
    opener?: boolean
  }) {
    return (
      <>
        <div data-reader-page="" tabIndex={-1} />
        {opener && <button type="button">opener</button>}
        <Panel open={open} typing={typing} />
      </>
    )
  }

  return Reader
}

const Reader = readerFixture()

const named = (text: string) =>
  [...document.querySelectorAll('button')].find(
    (b) => b.textContent === text,
  ) as HTMLButtonElement

function tab(shiftKey = false): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key: 'Tab',
    shiftKey,
    bubbles: true,
    cancelable: true,
  })
  act(() => {
    ;(document.activeElement ?? document.body).dispatchEvent(event)
  })
  return event
}

describe('usePanelFocus (spec 2026-09-26 §3.3)', () => {
  it('moves focus onto the panel itself when it opens', () => {
    mount(<Reader open={false} />)
    act(() => named('opener').focus())
    rerender(<Reader open />)
    expect(document.activeElement?.getAttribute('aria-label')).toBe('Panel')
  })

  it('hands focus back to what opened it when it closes', () => {
    mount(<Reader open={false} />)
    act(() => named('opener').focus())
    rerender(<Reader open />)
    act(() => named('first').focus())
    rerender(<Reader open={false} />)
    expect(document.activeElement).toBe(named('opener'))
  })

  it('hands it to the page when nothing had it', () => {
    mount(<Reader open={false} />)
    rerender(<Reader open />)
    rerender(<Reader open={false} />)
    expect(document.activeElement?.hasAttribute('data-reader-page')).toBe(true)
  })

  it('hands it to the page when the opener was removed while the panel was open', () => {
    mount(<Reader open={false} />)
    act(() => named('opener').focus())
    rerender(<Reader open />)
    rerender(<Reader open opener={false} />)
    rerender(<Reader open={false} opener={false} />)
    expect(document.activeElement?.hasAttribute('data-reader-page')).toBe(true)
  })

  it('wraps Tab at either end, and Shift+Tab from the panel itself', () => {
    mount(<Reader open />)
    expect(tab(true).defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(named('last'))
    expect(tab().defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(named('first'))
    expect(tab(true).defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(named('last'))
  })

  it('leaves a Tab between the ends to the browser', () => {
    mount(<Reader open typing />)
    act(() => named('first').focus())
    expect(tab().defaultPrevented).toBe(false)
  })
})
