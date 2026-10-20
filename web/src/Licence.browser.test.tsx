import { act } from 'react'
import { BrowserRouter, MemoryRouter, Route, Routes } from 'react-router'
import { describe, expect, it, onTestFinished } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { About } from './About'
import './index.css'
import { Licence } from './Licence'
import { answerAbout, inShell } from './testing/aboutPage'
import { focused } from './testing/focused'
import { licenceTextsLanded } from './testing/licence'
import { mount } from './testing/mount'
import {
  coarseRulesOn,
  fingerArea,
  near,
  overlapOf,
  pressArea,
} from './testing/pointer'

/** Mounts `ui` and lets the page's texts land. */
async function settle(ui: React.ReactNode): Promise<HTMLElement> {
  const host = mount(<MemoryRouter>{ui}</MemoryRouter>)
  await licenceTextsLanded()
  return host
}

const overflow = (el: Element | null | undefined) =>
  el ? el.scrollWidth - el.clientWidth : Number.NaN

/** How the texts are laid out, in the browser project because jsdom lays nothing out. */
describe('Licence’s texts', () => {
  /**
   * **At a phone's 390 px every text wraps inside its own card**, so nothing
   * scrolls sideways: not a card's text, not the shell's `<main>`, which
   * scrolls rather than widening the page, and not the page. A font
   * licence's first line runs its copyright notices and their addresses
   * together, and every text is written to wider lines than a card has here.
   */
  it('wrap inside their cards at 390px, and nothing scrolls sideways', async () => {
    await page.viewport(390, 844)
    const host = await settle(inShell(<Licence />))
    const cards = [...host.querySelectorAll('.about-card')]
    expect(
      cards.map((card) => [
        card.querySelector('h2')?.textContent,
        overflow(card.querySelector('pre')),
      ]),
    ).toEqual([
      ['taiyaku', 0],
      ['Shippori Mincho B1', 0],
      ['Zen Kaku Gothic New', 0],
      ['taiyaku marks', 0],
    ])
    expect(overflow(host.querySelector('main'))).toBe(0)
    expect(overflow(document.documentElement)).toBe(0)
  })

  /**
   * **At a desktop width the cards are About's width**, so the way down to
   * the licence page and back again keeps one measure, and a text written
   * to wider lines than that wraps inside its card, as at a phone's width.
   * About is drawn beside the licence page to be measured against, once its
   * request has answered, since until then it draws no card.
   */
  it('are About’s width at a desktop width, wrap inside it, and nothing scrolls sideways', async () => {
    await page.viewport(1280, 800)
    answerAbout()
    const host = await settle(
      inShell(
        <>
          <About />
          <Licence />
        </>,
      ),
    )
    const about = host.querySelector('.about:not(.licence) .about-card')
    if (!about) throw new Error('no About card')
    const width = about.getBoundingClientRect().width
    const cards = [...host.querySelectorAll('.licence .about-card')]
    expect(
      cards.map((card) => [
        card.querySelector('h2')?.textContent,
        card.getBoundingClientRect().width,
        overflow(card.querySelector('pre')),
      ]),
    ).toEqual([
      ['taiyaku', width, 0],
      ['Shippori Mincho B1', width, 0],
      ['Zen Kaku Gothic New', width, 0],
      ['taiyaku marks', width, 0],
    ])
    expect(overflow(host.querySelector('main'))).toBe(0)
    expect(overflow(document.documentElement)).toBe(0)
  })
})

/** The page's contents and the cards they lead to, by fragment and name. */
const CONTENTS = [
  ['#taiyaku', 'taiyaku'],
  ['#shippori-mincho-b1', 'Shippori Mincho B1'],
  ['#zen-kaku-gothic-new', 'Zen Kaku Gothic New'],
  ['#taiyaku-marks', 'taiyaku marks'],
] as const

/**
 * The licence page in the shell's `<main>`, under the router the app runs,
 * `BrowserRouter`, at this frame's own address with `fragment` on it, which
 * is taken off again when the test ends. `mount()`'s host gets the height
 * `#root` has in the app, so at a desktop width `<main>` is what scrolls.
 * The texts and the faces have landed when it returns.
 */
async function loadedAt(width: number, fragment = ''): Promise<HTMLElement> {
  const before = location.href
  onTestFinished(() => {
    history.replaceState(null, '', before)
  })
  history.replaceState(
    null,
    '',
    `${location.pathname}${location.search}${fragment}`,
  )
  await page.viewport(width, 844)
  const host = mount(<BrowserRouter>{inShell(<Licence />)}</BrowserRouter>)
  host.style.height = '100%'
  await licenceTextsLanded()
  await document.fonts.ready
  return host
}

/**
 * Where the card headed `name` is drawn against the top of `<main>`, which
 * scrolls at this width: its heading's top, less the `scroll-margin-top`
 * that puts it there, and the card's own top, which is in view when it is
 * not above `<main>`'s.
 */
function landing(host: HTMLElement, name: string) {
  const main = host.querySelector('main')
  const heading = [...host.querySelectorAll('.about-card h2')].find(
    (h2) => h2.textContent === name,
  )
  const card = heading?.closest('.about-card')
  if (!main || !heading || !card) throw new Error(`no card headed ${name}`)
  const top = main.getBoundingClientRect().top
  return {
    heading: Math.round(
      heading.getBoundingClientRect().top -
        top -
        Number.parseFloat(getComputedStyle(heading).scrollMarginTop),
    ),
    cardInView: card.getBoundingClientRect().top >= top,
  }
}

describe('Licence’s contents', () => {
  /**
   * **At a phone's 390 px every link is inside the page**, and nothing
   * scrolls sideways, with either pointer: a coarse one draws each link as
   * a box of its own.
   */
  it.each(['coarse', 'fine'] as const)(
    'stay inside the page at 390px with a %s pointer, and nothing scrolls sideways',
    async (pointer) => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      await page.viewport(390, 844)
      const host = await settle(inShell(<Licence />))
      const main = host.querySelector('main')
      if (!main) throw new Error('no main')
      const right =
        main.getBoundingClientRect().right -
        Number.parseFloat(getComputedStyle(main).paddingRight)
      expect(
        [...host.querySelectorAll('nav[aria-label="Contents"] a')].map(
          (link) => [
            link.textContent,
            link.getBoundingClientRect().right <= right,
          ],
        ),
      ).toEqual([
        ['taiyaku (GPL-3.0-or-later)', true],
        ['Shippori Mincho B1 (OFL-1.1)', true],
        ['Zen Kaku Gothic New (OFL-1.1)', true],
        ['taiyaku marks (OFL-1.1)', true],
      ])
      expect(overflow(host.querySelector('nav[aria-label="Contents"]'))).toBe(0)
      expect(overflow(main)).toBe(0)
      expect(overflow(document.documentElement)).toBe(0)
    },
  )

  /**
   * **A link followed by a real click moves the view to its card and focus
   * to its heading**, under the router the app runs, which leaves the
   * fragment in the address. The card lands whole at the top of the view,
   * its heading where its `scroll-margin-top` puts it. Focus is put on the
   * way up first, so the click is not the frame's first input.
   */
  it.each(CONTENTS)(
    'followed to %s by a click, puts %s at the top of the view and focus on its heading',
    async (fragment, name) => {
      const host = await loadedAt(1280)
      act(() => {
        host.querySelector<HTMLElement>('.back a')?.focus()
      })
      const link = host.querySelector(
        `nav[aria-label="Contents"] a[href="${fragment}"]`,
      )
      if (!link) throw new Error(`no link to ${fragment}`)
      await act(async () => {
        await userEvent.click(link)
      })
      await expect.poll(focused).toBe(name)
      expect(location.hash).toBe(fragment)
      expect(landing(host, name)).toEqual({ heading: 0, cardInView: true })
    },
  )

  /**
   * **A load of the page with a card's fragment lands on that card** once
   * the texts have drawn it, as following its link does.
   */
  it.each(CONTENTS)(
    'loaded with %s, puts %s at the top of the view and focus on its heading',
    async (fragment, name) => {
      const host = await loadedAt(1280, fragment)
      expect(focused()).toBe(name)
      expect(landing(host, name)).toEqual({ heading: 0, cardInView: true })
    },
  )
})

/**
 * **The browser's own scroll restoration is off for the page's entries and
 * back on for the entry a link leaves the page for**: the page puts its
 * view in place on Back and Forward itself (`Shell.browser.test.tsx`), and
 * the setting is an entry's, which an entry the browser makes for a
 * fragment, and one a link pushes, takes from the entry it is made from.
 * Under `BrowserRouter`, at this frame's own address, which is put back
 * when the test ends; the way up leads to a route of its own here, so the
 * page is left as it is in the app.
 */
describe('Licence’s entries', () => {
  it('restore no scroll of the browser’s own, and the entry it leaves for does', async () => {
    const before = location.href
    onTestFinished(() => {
      history.replaceState(null, '', before)
    })
    history.replaceState(null, '', `${location.pathname}${location.search}`)
    await page.viewport(1280, 844)
    const host = mount(
      <BrowserRouter>
        <Routes>
          <Route path="/about" element={<p>About</p>} />
          <Route path="*" element={<Licence />} />
        </Routes>
      </BrowserRouter>,
    )
    await licenceTextsLanded()
    const modes = [history.scrollRestoration]
    const link = host.querySelector('nav[aria-label="Contents"] a')
    const up = host.querySelector('.back a')
    if (!link || !up) throw new Error('no links')
    await act(async () => {
      await userEvent.click(link)
    })
    modes.push(history.scrollRestoration)
    await act(async () => {
      await userEvent.click(up)
    })
    expect([location.pathname, host.textContent]).toEqual(['/about', 'About'])
    modes.push(history.scrollRestoration)
    await act(async () => {
      const popped = new Promise((resolve) => {
        window.addEventListener('popstate', resolve, { once: true })
      })
      history.back()
      await popped
    })
    modes.push(history.scrollRestoration)
    expect(modes).toEqual(['manual', 'manual', 'auto', 'manual'])
  })
})

/**
 * **Every link on the page is a finger's 44 px under a coarse pointer**
 * (spec 2026-09-17 §7.1): the way up to About and each of the contents.
 * Measured by hit testing along its middle row and column (`pressArea`),
 * each target is its box grown about its middle to 44 px (`fingerArea`),
 * and no two of those grown boxes overlap, corners included. The phone
 * bar above the way up is the shell's, and `Shell.browser.test.tsx`
 * measures the two together. The fine case shows it is the coarse rules
 * that reach 44.
 */
describe('Licence’s links', () => {
  // Coarse first at 1280: the fine case after it is what shows the coarse
  // rules were switched back.
  it.each([
    [1280, 'coarse'],
    [1280, 'fine'],
    [390, 'coarse'],
  ] as const)('at %i px with a %s pointer', async (width, pointer) => {
    if (pointer === 'coarse') onTestFinished(coarseRulesOn())
    await page.viewport(width, 844)
    const host = await settle(inShell(<Licence />))
    await document.fonts.ready
    const links = [
      ...host.querySelectorAll('.back a, nav[aria-label="Contents"] a'),
    ]
    expect(links.map((link) => link.textContent)).toEqual([
      '‹ About',
      'taiyaku (GPL-3.0-or-later)',
      'Shippori Mincho B1 (OFL-1.1)',
      'Zen Kaku Gothic New (OFL-1.1)',
      'taiyaku marks (OFL-1.1)',
    ])
    if (pointer === 'fine') {
      for (const link of links) {
        expect
          .soft(pressArea(link).height, `${link.textContent}: height`)
          .toBeLessThan(44)
      }
      return
    }
    for (const link of links) {
      expect
        .soft(pressArea(link), link.textContent ?? '')
        .toEqual(near(fingerArea(link)))
    }
    for (const [i, link] of links.entries()) {
      for (const other of links.slice(i + 1)) {
        expect
          .soft(
            overlapOf(fingerArea(link), fingerArea(other)),
            `${link.textContent}, ${other.textContent}`,
          )
          .toBe(0)
      }
    }
  })
})
