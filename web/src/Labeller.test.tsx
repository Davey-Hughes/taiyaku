import { act } from 'react'
import { describe, expect, it } from 'vitest'
import type { FetchedTruth } from './api'
import { Labeller } from './Labeller'
import type { LabelBox } from './labelling'
import { mount } from './testing/mount'
import type { TruthDoc } from './truth'
import type { TruthPorts } from './truthWrite'

/**
 * The frame-measurement effect builds its own `ResizeObserver` and jsdom does
 * not carry one — `Annotations.test.tsx` stubs it for the identical reason,
 * for `PageAnnotations`' identical use of it.
 */
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver =
  StubResizeObserver as unknown as typeof ResizeObserver

const one: LabelBox[] = [
  {
    id: 'a',
    x: 0.1,
    y: 0.1,
    w: 0.2,
    h: 0.2,
    kind: 'dialogue',
    axis: 'v',
    ignore: false,
  },
]

function region(host: HTMLElement, id: string): HTMLElement {
  return host.querySelector(`[data-testid="region-${id}"]`) as HTMLElement
}

function press(key: string) {
  act(() => {
    window.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }))
  })
}

describe('Labeller keys', () => {
  it('tags a kind by number', () => {
    const host = mount(<Labeller initialBoxes={one} initialSelected="a" />)
    press('3')
    expect(region(host, 'a').getAttribute('data-kind')).toBe('sfx')
  })

  it('sets an axis, including not-applicable', () => {
    const host = mount(<Labeller initialBoxes={one} initialSelected="a" />)
    press('n')
    expect(
      host.querySelector('[data-testid="axis-readout"]')?.textContent,
    ).toBe('—')
    press('h')
    expect(
      host.querySelector('[data-testid="axis-readout"]')?.textContent,
    ).toBe('h')
  })

  it('toggles ignore', () => {
    const host = mount(<Labeller initialBoxes={one} initialSelected="a" />)
    press('i')
    expect(region(host, 'a').getAttribute('data-ignore')).toBe('true')
  })

  it('deletes the selected box', () => {
    const host = mount(<Labeller initialBoxes={one} initialSelected="a" />)
    press('Delete')
    expect(host.querySelector('[data-testid="region-a"]')).toBeNull()
  })

  it('does nothing on a kind key with no box selected', () => {
    const host = mount(<Labeller initialBoxes={one} />)
    press('3')
    expect(region(host, 'a').getAttribute('data-kind')).toBe('dialogue')
  })

  it('shows how many boxes are on this page, so progress is visible', () => {
    const host = mount(<Labeller initialBoxes={one} />)
    expect(host.querySelector('[data-testid="box-count"]')?.textContent).toBe(
      '1',
    )
  })

  it('shows the selected box’s kind, so a kind key confirms itself', () => {
    // Nothing rendered `data-kind` and nothing printed it, so pressing `1`-`6`
    // produced no confirmation of any sort — 1,500 boxes tagged blind against
    // the one input this instrument has.
    const host = mount(<Labeller initialBoxes={one} initialSelected="a" />)
    const readout = () =>
      host.querySelector('[data-testid="kind-readout"]')?.textContent
    expect(readout()).toBe('dialogue')
    press('3')
    expect(readout()).toBe('sfx')
    press('6')
    expect(readout()).toBe('other')
  })

  it('shows nothing for a kind when no box is selected', () => {
    const host = mount(<Labeller initialBoxes={one} />)
    expect(
      host.querySelector('[data-testid="kind-readout"]')?.textContent,
    ).toBe('')
  })

  it('drags a box against the page element when no image is on screen', () => {
    // No `ports` means no `PageView` — the standalone editor's own fallback,
    // which `toFractions` reaches whenever `container.querySelector('img')`
    // finds nothing. `pointOnPage`'s own tests in `truthWrite.test.ts` pin
    // the arithmetic; this is the wiring that reaches it from a real
    // gesture. `Labeller.browser.test.tsx` drives the identical case with
    // real layout instead of a stubbed rect.
    const host = mount(<Labeller />)
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    stubRect(page, { left: 0, top: 0, width: 400, height: 400 })
    act(() => {
      page.dispatchEvent(
        new PointerEvent('pointerdown', {
          clientX: 40,
          clientY: 40,
          bubbles: true,
        }),
      )
      page.dispatchEvent(
        new PointerEvent('pointermove', {
          clientX: 200,
          clientY: 200,
          bubbles: true,
        }),
      )
      page.dispatchEvent(
        new PointerEvent('pointerup', {
          clientX: 200,
          clientY: 200,
          bubbles: true,
        }),
      )
    })
    const drawn = host.querySelector('.region-box') as HTMLElement
    expect(drawn.style.left).toBe('10%')
    expect(drawn.style.top).toBe('10%')
    expect(drawn.style.width).toBe('40%')
    expect(drawn.style.height).toBe('40%')
  })
})

/** Drains the promise chain a fake port's `.then` runs through — the load
 * effects, the derived `setState` calls, and the debounce they arm — inside
 * `act` so the resulting state updates are not a warning. `PageView.test.tsx`'s
 * `flush` is the same idiom, for the same reason. */
async function flush(): Promise<void> {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
}

/** A `TruthPorts` that answers from an in-memory work/sidecar/document, and
 * records every `put` — a stand-in for `taiyaku serve`'s truth route. */
function fakePorts(over: Partial<TruthPorts> = {}): {
  ports: TruthPorts
  puts: { work: string; etag: string; doc: TruthDoc }[]
} {
  const puts: { work: string; etag: string; doc: TruthDoc }[] = []
  const sidecar = JSON.stringify({
    hub: 'raw-mono',
    editions: {
      'raw-mono': { members: [{ sha256: 'aa', pages: 3 }] },
      'en-mono': { members: [{ sha256: 'bb', pages: 3 }] },
    },
  })
  let doc: TruthDoc = { taiyaku_truth: 1, editions: {} }
  let tag = 'v1'
  const ports: TruthPorts = {
    works: () => Promise.resolve(['gotoubun']),
    sidecar: () => Promise.resolve(sidecar),
    get: (): Promise<FetchedTruth> => Promise.resolve({ doc, etag: tag }),
    put: (work, etag, sent) => {
      puts.push({ work, etag, doc: sent })
      doc = sent
      tag = `v${puts.length + 1}`
      return Promise.resolve({ doc, etag: tag })
    },
    ...over,
  }
  return { ports, puts }
}

/**
 * Two works, each with its own sidecar, edition name and truth document — and
 * **the same ETag for both**, which is not a shortcut: two never-labelled
 * works serialise to identical bytes, so the server derives identical
 * validators for them, and that is precisely why a misdirected write is not
 * caught by `If-Match`.
 *
 * `fakePorts` above cannot express this: it answers one sidecar and one
 * document whatever work it is asked about, so nothing it returns could tell
 * a write to the wrong work from a write to the right one.
 */
function twoWorkPorts(): {
  ports: TruthPorts
  puts: { work: string; etag: string; doc: TruthDoc }[]
} {
  const puts: { work: string; etag: string; doc: TruthDoc }[] = []
  // The two works share an edition *name* and differ only in the archives
  // behind it, which is the ordinary case across this corpus — every work has
  // a `raw-mono`. It is also the sharp one: a switch that left the previous
  // work's document in place would not even change which edition is selected,
  // so nothing downstream would be re-derived and the stale boxes would simply
  // stay on screen.
  const sidecars: Record<string, string> = {
    a: JSON.stringify({
      hub: 'raw-mono',
      editions: { 'raw-mono': { members: [{ sha256: 'aa', pages: 3 }] } },
    }),
    b: JSON.stringify({
      hub: 'raw-mono',
      editions: { 'raw-mono': { members: [{ sha256: 'bb', pages: 3 }] } },
    }),
  }
  const docs: Record<string, TruthDoc> = {
    a: {
      taiyaku_truth: 1,
      editions: {
        'raw-mono': {
          members: [{ sha256: 'aa' }],
          pages: {
            '0': [
              {
                x: 0.1,
                y: 0.1,
                w: 0.2,
                h: 0.2,
                axis: 'v',
                kind: 'dialogue',
                ignore: false,
              },
            ],
          },
        },
      },
    },
    b: { taiyaku_truth: 1, editions: {} },
  }
  const ports: TruthPorts = {
    works: () => Promise.resolve(['a', 'b']),
    sidecar: (work) => Promise.resolve(sidecars[work]),
    get: (work) => Promise.resolve({ doc: docs[work], etag: 'v1' }),
    put: (work, etag, sent) => {
      puts.push({ work, etag, doc: sent })
      docs[work] = sent
      return Promise.resolve({ doc: sent, etag: 'v2' })
    },
  }
  return { ports, puts }
}

const selects = (host: HTMLElement) =>
  [...host.querySelectorAll('select')] as HTMLSelectElement[]

/**
 * Send the labeller to a page the way a person does — by typing into the page
 * input.
 *
 * React tracks a controlled input's value through its own property setter, so
 * mutating `.value` directly leaves that tracker stale and the subsequent
 * `input` event a no-op. The native setter, taken off the prototype, is what a
 * real keystroke goes through instead.
 *
 * It matters here beyond convenience: choosing a page is exactly what
 * distinguishes a recorded observation from a page the labeller merely landed
 * on, so a test that reached `setPage` by any other route would not be
 * exercising the thing under test.
 */
function goToPage(host: HTMLElement, page: string) {
  const input = host.querySelector('input[type="number"]') as HTMLInputElement
  const nativeSetter = Object.getOwnPropertyDescriptor(
    window.HTMLInputElement.prototype,
    'value',
  )?.set
  act(() => {
    nativeSetter?.call(input, page)
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

/** `Annotations.test.tsx`'s `stubRect`/`stubNatural`, for the same reason:
 * jsdom has no layout engine, so a real `<img>`'s `getBoundingClientRect()`
 * and `naturalWidth`/`naturalHeight` are zero until stubbed. */
function stubRect(
  el: Element,
  rect: { left?: number; top?: number; width: number; height: number },
) {
  const full = {
    x: rect.left ?? 0,
    y: rect.top ?? 0,
    width: rect.width,
    height: rect.height,
    top: rect.top ?? 0,
    left: rect.left ?? 0,
    right: (rect.left ?? 0) + rect.width,
    bottom: (rect.top ?? 0) + rect.height,
    toJSON() {
      return this
    },
  }
  ;(el as HTMLElement).getBoundingClientRect = () => full as DOMRect
}

function stubNatural(img: HTMLImageElement, w: number, h: number) {
  Object.defineProperty(img, 'naturalWidth', { value: w, configurable: true })
  Object.defineProperty(img, 'naturalHeight', { value: h, configurable: true })
}

describe('Labeller with ports', () => {
  it('is the plain standalone editor, with no picker, when ports are not given', () => {
    const host = mount(<Labeller />)
    expect(host.querySelectorAll('select')).toHaveLength(0)
  })

  it('lists works and the sidecar-derived editions once loaded, hub first', async () => {
    const { ports } = fakePorts()
    const host = mount(<Labeller ports={ports} />)
    await flush()
    const [work, edition] = selects(host)
    expect(work.value).toBe('gotoubun')
    expect([...edition.options].map((o) => o.value)).toEqual([
      'raw-mono',
      'en-mono',
    ])
    expect(edition.value).toBe('raw-mono')
  })

  it('shows progress against the sidecar page count — landing on page 0 counts nothing', async () => {
    const { ports } = fakePorts()
    const host = mount(<Labeller ports={ports} />)
    await flush()
    // Landing is not labelling. The labeller opens on page 0 because it has to
    // open somewhere, and nobody chose it — so the progress readout starts at
    // zero and the document stays empty.
    expect(
      host.querySelector('[data-testid="truth-progress"]')?.textContent,
    ).toBe('0 / 3 pages labelled')
  })

  it('counts a page the labeller was actually sent to, even with no boxes on it', async () => {
    const { ports } = fakePorts()
    const host = mount(<Labeller ports={ports} />)
    await flush()
    goToPage(host, '2')
    await flush()
    // The other half of the same rule: a page reached on purpose and found to
    // hold no text is a real observation, and must still be recorded.
    expect(
      host.querySelector('[data-testid="truth-progress"]')?.textContent,
    ).toBe('1 / 3 pages labelled')
  })

  it('loads an already-labelled page boxes on arrival', async () => {
    const { ports } = fakePorts({
      get: () =>
        Promise.resolve({
          doc: {
            taiyaku_truth: 1,
            editions: {
              'raw-mono': {
                members: [{ sha256: 'aa' }],
                pages: {
                  '0': [
                    {
                      x: 0.1,
                      y: 0.1,
                      w: 0.2,
                      h: 0.2,
                      axis: 'v',
                      kind: 'dialogue',
                      ignore: false,
                    },
                  ],
                },
              },
            },
          },
          etag: 'v1',
        }),
    })
    const host = mount(<Labeller ports={ports} />)
    await flush()
    expect(host.querySelector('[data-testid="box-count"]')?.textContent).toBe(
      '1',
    )
  })

  it('writes a present-but-empty key for a page navigated to and left with no edits, on hide', async () => {
    const { ports, puts } = fakePorts()
    const host = mount(<Labeller ports={ports} />)
    await flush()
    goToPage(host, '1')
    await flush()
    act(() => {
      window.dispatchEvent(new Event('pagehide'))
    })
    await flush()
    expect(puts.length).toBeGreaterThan(0)
    const written = puts[puts.length - 1].doc
    expect(written.editions['raw-mono'].pages['1']).toEqual([])
  })

  it('writes nothing at all for a session that only ever landed on page 0', async () => {
    const { ports, puts } = fakePorts()
    mount(<Labeller ports={ports} />)
    await flush()
    act(() => {
      window.dispatchEvent(new Event('pagehide'))
    })
    await flush()
    // Not "wrote an empty document" — wrote *nothing*. A put here would carry a
    // `pages` key for page 0, and this is the write that put one phantom page
    // into each of the nine dev editions.
    expect(puts).toEqual([])
  })

  it('persists deleting the last box on a page, rather than silently keeping it', async () => {
    const { ports, puts } = fakePorts({
      get: () =>
        Promise.resolve({
          doc: {
            taiyaku_truth: 1,
            editions: {
              'raw-mono': {
                members: [{ sha256: 'aa' }],
                pages: {
                  '0': [
                    {
                      x: 0.1,
                      y: 0.1,
                      w: 0.2,
                      h: 0.2,
                      axis: 'v',
                      kind: 'dialogue',
                      ignore: false,
                    },
                  ],
                },
              },
            },
          },
          etag: 'v1',
        }),
    })
    const host = mount(<Labeller ports={ports} />)
    await flush()
    // Page 0 was landed on, not chosen, and removing the one box leaves it
    // empty — the exact shape the `deliberate` guard would swallow if it did
    // not also ask whether the document already keys this page.
    const box = host.querySelector('[data-testid^="region-"]') as HTMLElement
    act(() => {
      box.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    })
    act(() => {
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete' }))
    })
    await flush()
    act(() => {
      window.dispatchEvent(new Event('pagehide'))
    })
    await flush()
    expect(puts.length).toBeGreaterThan(0)
    expect(puts[puts.length - 1].doc.editions['raw-mono'].pages['0']).toEqual(
      [],
    )
  })

  it('never writes a key for a page nobody has navigated to', async () => {
    const { ports, puts } = fakePorts()
    const host = mount(<Labeller ports={ports} />)
    await flush()
    // Visit exactly one page, so there is a real write to inspect and the
    // absence of the other two keys means something. Asserting against a
    // document nothing ever wrote would pass no matter what this code did.
    goToPage(host, '1')
    await flush()
    act(() => {
      window.dispatchEvent(new Event('pagehide'))
    })
    await flush()
    const written = puts[puts.length - 1].doc
    expect(written.editions['raw-mono'].pages['1']).toEqual([])
    expect(written.editions['raw-mono'].pages['0']).toBeUndefined()
    expect(written.editions['raw-mono'].pages['2']).toBeUndefined()
  })

  it('saves an edit made to an already-labelled box', async () => {
    // Drag geometry needs real layout — jsdom has none, which is exactly why
    // `Labeller.browser.test.tsx` exists for that half. This exercises the
    // other half: select an already-loaded box by click (which does not
    // depend on layout) and edit it by key, the same path
    // `describe('Labeller keys')` above already drives.
    const { ports, puts } = fakePorts({
      get: () =>
        Promise.resolve({
          doc: {
            taiyaku_truth: 1,
            editions: {
              'raw-mono': {
                members: [{ sha256: 'aa' }],
                pages: {
                  '0': [
                    {
                      x: 0.1,
                      y: 0.1,
                      w: 0.2,
                      h: 0.2,
                      axis: 'v',
                      kind: 'dialogue',
                      ignore: false,
                    },
                  ],
                },
              },
            },
          },
          etag: 'v1',
        }),
    })
    const host = mount(<Labeller ports={ports} />)
    await flush()
    const box = host.querySelector('[data-testid^="region-"]') as HTMLElement
    act(() => {
      box.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    })
    act(() => {
      window.dispatchEvent(
        new KeyboardEvent('keydown', { key: '3', bubbles: true }),
      )
    })
    act(() => {
      window.dispatchEvent(new Event('pagehide'))
    })
    await flush()
    const written = puts[puts.length - 1].doc
    expect(written.editions['raw-mono'].pages['0']).toEqual([
      { x: 0.1, y: 0.1, w: 0.2, h: 0.2, axis: 'v', kind: 'sfx', ignore: false },
    ])
  })

  it('shows a notice, rather than saving silently, when the load carried no validator', async () => {
    const { ports, puts } = fakePorts({
      get: () =>
        Promise.resolve({
          doc: { taiyaku_truth: 1, editions: {} },
          etag: undefined,
        }),
    })
    const host = mount(<Labeller ports={ports} />)
    await flush()
    // A deliberate visit, because landing on page 0 no longer attempts a write
    // at all — and with nothing attempted there would be nothing for the
    // missing validator to refuse, so this test would pass without measuring.
    goToPage(host, '1')
    await flush()
    act(() => {
      window.dispatchEvent(new Event('pagehide'))
    })
    await flush()
    expect(puts).toEqual([])
    expect(
      host.querySelector('[data-testid="truth-notice"]')?.textContent,
    ).toMatch(/not being saved/)
  })

  it('reports a listWorks failure rather than leaving the picker silently empty', async () => {
    const { ports } = fakePorts({
      works: () => Promise.reject(new Error('listing works: 500 disk is full')),
    })
    const host = mount(<Labeller ports={ports} />)
    await flush()
    expect(
      host.querySelector('[data-testid="truth-notice"]')?.textContent,
    ).toMatch(/disk is full/)
  })

  it('resets to page 0 on an edition switch, and does not count that page as visited', async () => {
    const { ports, puts } = fakePorts()
    const host = mount(<Labeller ports={ports} />)
    await flush()
    // Choose a page in the first edition, so the switch below is crossing from
    // a deliberate visit rather than from the mount's own landing.
    goToPage(host, '2')
    await flush()
    const [, edition] = selects(host)
    act(() => {
      edition.value = 'en-mono'
      edition.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await flush()
    act(() => {
      window.dispatchEvent(new Event('pagehide'))
    })
    await flush()
    const page = host.querySelector('input[type="number"]') as HTMLInputElement
    expect(page.value).toBe('0')
    expect(
      host.querySelector('[data-testid="truth-progress"]')?.textContent,
    ).toBe('0 / 3 pages labelled')
    // The switch must not carry the previous page's deliberateness with it.
    // One of these per edition is what put nine phantom pages into a 45-page
    // dev set.
    const written = puts[puts.length - 1].doc
    expect(written.editions['en-mono']).toBeUndefined()
    expect(written.editions['raw-mono'].pages['2']).toEqual([])
  })

  it('clamps a page number typed past the edition’s last page', async () => {
    const { ports } = fakePorts()
    const host = mount(<Labeller ports={ports} />)
    await flush()
    const page = host.querySelector('input[type="number"]') as HTMLInputElement
    // React tracks a controlled input's value through its own property
    // setter, so mutating `.value` directly leaves that tracker stale and the
    // subsequent `input` event a no-op — the native setter, taken off the
    // prototype, is what a real keystroke goes through instead.
    const nativeSetter = Object.getOwnPropertyDescriptor(
      window.HTMLInputElement.prototype,
      'value',
    )?.set
    act(() => {
      nativeSetter?.call(page, '99')
      page.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await flush()
    expect(page.value).toBe('2') // pageCount 3, last index 2
  })

  it('writes a pending edit to the work it was made in, not the one switched to', async () => {
    // Draw in work A, switch to work B inside the 700 ms debounce, and the
    // timer used to fire with A's document, B's work id and A's etag.
    // Normally that is a 412 and a notice — but two never-labelled works
    // serialise to identical bytes and therefore identical ETags, so
    // `If-Match` *matches* and A's boxes land in B's `taiyaku.truth.json`.
    // That is a box on the wrong art, which `taiyaku-core`'s
    // `TruthEdition::is_stale_for` calls the sharpest form of the staleness
    // problem. The two works below hand back the same `v1` validator on
    // purpose, so nothing but the fix itself can stop the write.
    const { ports, puts } = twoWorkPorts()
    const host = mount(<Labeller ports={ports} />)
    await flush()

    // An edit in work A: select its one loaded box and tag it.
    const box = host.querySelector('[data-testid^="region-"]') as HTMLElement
    act(() => {
      box.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    })
    press('3')

    // ...and switch to work B before the debounce has run. No `flush()` here:
    // the point is that the save is still pending when the work changes.
    const [workSelect] = selects(host)
    act(() => {
      workSelect.value = 'b'
      workSelect.dispatchEvent(new Event('change', { bubbles: true }))
    })

    act(() => {
      window.dispatchEvent(new Event('pagehide'))
    })
    await flush()

    expect(puts).toHaveLength(1)
    expect(puts[0].work).toBe('a')
    expect(puts[0].doc.editions['raw-mono'].pages['0']).toEqual([
      { x: 0.1, y: 0.1, w: 0.2, h: 0.2, axis: 'v', kind: 'sfx', ignore: false },
    ])
    expect(
      puts[0].doc.editions['raw-mono'].members,
      'stamped from A’s sidecar, so the document is A’s throughout',
    ).toEqual([{ sha256: 'aa' }])
  })

  it('does not carry one work’s boxes onto the next work’s page', async () => {
    // The same pairing, seen from the editor rather than the wire. A work
    // switch used to reload `boxes` from the *previous* work's document —
    // `doc` was still the old one — so the new work's page 0 opened holding
    // art it has nothing to do with, and the next keystroke would have saved
    // it there.
    const { ports } = twoWorkPorts()
    const host = mount(<Labeller ports={ports} />)
    await flush()
    expect(host.querySelector('[data-testid="box-count"]')?.textContent).toBe(
      '1',
    )

    const [workSelect] = selects(host)
    act(() => {
      workSelect.value = 'b'
      workSelect.dispatchEvent(new Event('change', { bubbles: true }))
    })
    await flush()

    expect(host.querySelector('[data-testid="box-count"]')?.textContent).toBe(
      '0',
    )
  })

  it('positions the overlay over the painted image, not the whole page area, once the page decodes', async () => {
    const { ports } = fakePorts()
    const host = mount(<Labeller ports={ports} />)
    await flush()
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    stubRect(page, { width: 400, height: 400 })
    const img = host.querySelector('img') as HTMLImageElement
    stubRect(img, { width: 400, height: 400 })
    stubNatural(img, 200, 100) // 2:1 art, letterboxed top and bottom once fit into a 400x400 box.
    act(() => {
      img.dispatchEvent(new Event('load'))
    })
    const frame = host.querySelector('.region-overlay')
      ?.parentElement as HTMLElement
    expect(frame.style.top).toBe('100px')
    expect(frame.style.height).toBe('200px')
    expect(frame.style.left).toBe('0px')
    expect(frame.style.width).toBe('400px')
  })
})
