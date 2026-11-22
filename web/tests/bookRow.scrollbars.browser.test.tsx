import { act } from 'react'
import { afterAll, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page } from 'vitest/browser'
import { BookReader } from '../src/BookReader'
import '../src/index.css'
import { mount } from './support/mount'
import { pointerRulesAs } from './support/pointer'

vi.mock('../src/Account', () => ({ Account: () => null }))

/**
 * The book's readout row at 320 px with Back drawn, beside scrollbars that
 * take room, as classic ones do (O425). Its buttons are wider than the window
 * there, so the row scrolls sideways; a scrollbar drawn in it would take the
 * height the buttons need. `bookTaps.browser.test.tsx` follows each button
 * into view where scrollbars take none.
 *
 * **Run only in the launch that shows scrollbars**, `vite.config.ts`'s
 * second `browser` instance: Playwright's headless Chromium hides them
 * otherwise. Each case first asserts that a scrollbar here takes room.
 */

const WORDS = Array.from(
  { length: 40 },
  (_, i) => `word${i} the quick brown fox.`,
).join(' ')

/** Six blocks, a link in the first to the fifth, which a jump draws Back for. */
const book = {
  title: 'Linked',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: Array.from({ length: 6 }, (_, i) => {
        const text = `Block ${i} opens. ${WORDS}`
        return {
          text,
          heading: false,
          sentences: [[0, text.length]],
          ...(i === 0
            ? {
                marks: [
                  { kind: 'link', start: 6, end: 7, block: 4, note: false },
                ],
              }
            : {}),
        }
      }),
    },
  },
  alignments: [],
}

async function settled() {
  for (let i = 0; i < 5; i++) {
    await act(() => new Promise((done) => setTimeout(done, 20)))
  }
}

/** The book at 320×568 with Back drawn by a jump from the keyboard; its readout row. */
async function rowWithBack(): Promise<HTMLElement> {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: string, init?: RequestInit) => {
      if (url === '/api/works/w/book') return Response.json(book)
      if (url === '/api/works/w/notes') {
        return Response.json(
          { work: 'w', notes: [] },
          { headers: { etag: '"n"' } },
        )
      }
      if (init?.method === 'PUT') return new Response(null, { status: 204 })
      return Response.json({ positions: [] })
    }),
  )
  await page.viewport(320, 568)
  const host = mount(
    <div style={{ position: 'fixed', inset: 0 }}>
      <BookReader work="w" onLeave={() => {}} />
    </div>,
  )
  for (let i = 0; i < 100 && !host.querySelector('[data-link-block]'); i++) {
    await act(() => new Promise((done) => setTimeout(done, 10)))
  }
  const link = host.querySelector('[data-link-block]') as HTMLElement
  act(() => {
    link.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }),
    )
  })
  await settled()
  await document.fonts.ready
  const back = [...host.querySelectorAll('.reader-bottom-bar button')].some(
    (b) => b.textContent === 'Back',
  )
  expect(back, 'Back drawn').toBe(true)
  return host.querySelector('.reader-readout') as HTMLElement
}

/** How much room a scrollbar takes in this launch: none where they are hidden. */
function scrollbarRoom(): number {
  const probe = document.createElement('div')
  probe.style.cssText = 'overflow: scroll; width: 100px; height: 100px'
  document.body.append(probe)
  const room = probe.offsetHeight - probe.clientHeight
  probe.remove()
  return room
}

describe('the book’s readout row beside scrollbars that take room (O425)', () => {
  afterAll(() => vi.unstubAllGlobals())

  // A tablet's with a trackpad is a finger's fullest row: a touch screen
  // alone draws no Help.
  for (const device of ['mouse', 'tablet with trackpad'] as const) {
    it(`scrolls to each button at 320×568 with none cut short top or bottom, ${device}`, async () => {
      expect(scrollbarRoom()).toBeGreaterThan(0)
      onTestFinished(pointerRulesAs(device))
      const row = await rowWithBack()
      const at = row.getBoundingClientRect()
      const shownBottom = at.top + row.clientTop + row.clientHeight
      const fits = [...row.querySelectorAll<HTMLElement>('button')].map(
        (button) => {
          button.scrollIntoView({ block: 'nearest', inline: 'nearest' })
          const box = button.getBoundingClientRect()
          return [
            button.getAttribute('aria-label') ?? button.textContent,
            box.top >= at.top && box.bottom <= shownBottom,
            box.left >= 0 && box.right <= window.innerWidth,
          ]
        },
      )
      expect(fits).toEqual(
        ['Contents', 'Search', 'Look up words', 'Back', 'Help'].map((name) => [
          name,
          true,
          true,
        ]),
      )
    })
  }
})
