import { act } from 'react'
import { BrowserRouter, MemoryRouter, Route, Routes } from 'react-router'
import { describe, expect, it, onTestFinished } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { About } from '../src/About'
import '../src/index.css'
import { Licence } from '../src/Licence'
import { answerAbout, inShell } from './support/aboutPage'
import { focused } from './support/focused'
import { licenceTextsLanded } from './support/licence'
import { mount, rerender } from './support/mount'
import {
  areaOf,
  coarseRulesOn,
  fingerArea,
  near,
  overlapOf,
  pressArea,
} from './support/pointer'

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
   * **At a phone's 390 px every text wraps inside its own box**, so nothing
   * scrolls sideways: not a licence's text, not the shell's `<main>`, which
   * scrolls rather than widening the page, and not the page. A font
   * licence's first line runs its copyright notices and their addresses
   * together, and every text is written to wider lines than a box has here.
   */
  it('wrap inside their boxes at 390px, and nothing scrolls sideways', async () => {
    await page.viewport(390, 844)
    const host = await settle(inShell(<Licence />))
    const sections = [...host.querySelectorAll('.settings-section')]
    expect(
      sections.map((section) => [
        section.querySelector('h2')?.textContent,
        overflow(section.querySelector('.settings-box > pre')),
      ]),
    ).toEqual([
      ['taiyaku', 0],
      ['Shippori Mincho B1', 0],
      ['Zen Kaku Gothic New', 0],
      ['taiyaku marks', 0],
      ['libarchive', 0],
    ])
    expect(overflow(host.querySelector('main'))).toBe(0)
    expect(overflow(document.documentElement)).toBe(0)
  })

  /**
   * **At a desktop width the sections are About's width**, so the way down
   * to the licence page and back again keeps one measure, and a text
   * written to wider lines than that wraps inside its box, as at a phone's
   * width. About is drawn beside the licence page to be measured against,
   * once its request has answered, since until then it draws no section.
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
    const about = host.querySelector('.about:not(.licence) .settings-section')
    if (!about) throw new Error('no About section')
    const width = about.getBoundingClientRect().width
    const sections = [...host.querySelectorAll('.licence .settings-section')]
    expect(
      sections.map((section) => [
        section.querySelector('h2')?.textContent,
        section.getBoundingClientRect().width,
        overflow(section.querySelector('.settings-box > pre')),
      ]),
    ).toEqual([
      ['taiyaku', width, 0],
      ['Shippori Mincho B1', width, 0],
      ['Zen Kaku Gothic New', width, 0],
      ['taiyaku marks', width, 0],
      ['libarchive', width, 0],
    ])
    expect(overflow(host.querySelector('main'))).toBe(0)
    expect(overflow(document.documentElement)).toBe(0)
  })
})

/** The page's contents and the licences they lead to, by fragment and name. */
const CONTENTS = [
  ['#taiyaku', 'taiyaku'],
  ['#shippori-mincho-b1', 'Shippori Mincho B1'],
  ['#zen-kaku-gothic-new', 'Zen Kaku Gothic New'],
  ['#taiyaku-marks', 'taiyaku marks'],
  ['#libarchive', 'libarchive'],
] as const

/** How many page modules of their own this file has imported. */
let pagesOfTheirOwn = 0

/**
 * The licence page from a module of its own: the same file at an address
 * with a query no other import gives it, which the browser loads as a
 * module apart, with its own `loaded`.
 */
async function pageOfItsOwn(): Promise<typeof Licence> {
  pagesOfTheirOwn += 1
  const address = `../src/Licence.tsx?own=${pagesOfTheirOwn}`
  const own = (await import(/* @vite-ignore */ address)) as {
    Licence: typeof Licence
  }
  return own.Licence
}

/**
 * The licence page in the shell's `<main>`, under the router the app runs,
 * `BrowserRouter`, at this frame's own address with `fragment` on it, which
 * is taken off again when the test ends. `mount()`'s host gets the height
 * `#root` has in the app, so at a desktop width `<main>` is what scrolls,
 * before the page is drawn in it. The texts and the faces have landed when
 * it returns.
 *
 * `visit` is the tab's first visit to the page, whose texts land after it is
 * drawn, or a return, which draws the texts the first visit loaded in its
 * first render and puts the view where the fragment says as it draws them.
 * Each is a page module of its own, whose kept texts no earlier case
 * loaded, so a case runs the same path alone as in its file: imported at an
 * address of its own (`pageOfItsOwn`), since `vi.resetModules()` leaves the
 * browser's modules as they are.
 */
async function loadedAt(
  width: number,
  fragment = '',
  visit: 'a first visit' | 'a return' = 'a first visit',
): Promise<HTMLElement> {
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
  const Page = await pageOfItsOwn()
  const host = mount(null)
  host.style.height = '100%'
  const drawn = <BrowserRouter>{inShell(<Page />)}</BrowserRouter>
  if (visit === 'a return') {
    rerender(drawn)
    await licenceTextsLanded()
    rerender(null)
  }
  rerender(drawn)
  // The path the case says: the texts in the first render on a return only.
  expect(
    host.querySelector('nav.licence-contents') !== null,
    'texts in the first render',
  ).toBe(visit === 'a return')
  await licenceTextsLanded()
  await document.fonts.ready
  return host
}

/**
 * Where the section headed `name` is drawn against the top of `<main>`,
 * which scrolls at this width: its heading's top, less the
 * `scroll-margin-top` that puts it there, and the heading's own top, which
 * is below `<main>`'s when the margin keeps the page's top padding above it.
 */
function landing(host: HTMLElement, name: string) {
  const main = host.querySelector('main')
  const heading = [...host.querySelectorAll('.settings-section h2')].find(
    (h2) => h2.textContent === name,
  )
  if (!main || !heading) throw new Error(`no section headed ${name}`)
  const top = main.getBoundingClientRect().top
  return {
    heading: Math.round(
      heading.getBoundingClientRect().top -
        top -
        Number.parseFloat(getComputedStyle(heading).scrollMarginTop),
    ),
    headingBelowTop: heading.getBoundingClientRect().top > top,
  }
}

/** Where a licence draws Back to top: its heading's line, or its box's foot. */
type Place = 'heading' | 'foot'

/** The section headed `name`. */
function sectionOf(host: HTMLElement, name: string): HTMLElement {
  const found = [
    ...host.querySelectorAll<HTMLElement>('.settings-section'),
  ].find((each) => each.querySelector('h2')?.textContent === name)
  if (!found) throw new Error(`no section headed "${name}"`)
  return found
}

/** The Back to top link at `place` in the section headed `name`. */
function toTop(host: HTMLElement, name: string, place: Place): HTMLElement {
  const part = sectionOf(host, name).querySelector(
    place === 'heading'
      ? ':scope > .settings-section-head'
      : ':scope > .settings-box',
  )
  const links = [...(part?.querySelectorAll<HTMLElement>('a') ?? [])].filter(
    (link) => link.textContent?.endsWith('Back to top'),
  )
  if (links.length !== 1) {
    throw new Error(`${links.length} Back to top links at "${name}"'s ${place}`)
  }
  return links[0] as HTMLElement
}

/** Where the content of `section`'s box ends, inside its edge and padding. */
function contentEnd(section: HTMLElement): number {
  const box = section.querySelector<HTMLElement>(':scope > .settings-box')
  if (!box) throw new Error('no box')
  const style = getComputedStyle(box)
  return (
    box.getBoundingClientRect().right -
    Number.parseFloat(style.borderRightWidth) -
    Number.parseFloat(style.paddingRight)
  )
}

/** Every licence's name, and both places it draws Back to top. */
const EVERY_TOP = CONTENTS.flatMap(([, name]) =>
  (['heading', 'foot'] as const).map((place) => [name, place] as const),
)

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
        ['libarchive (BSD-2-Clause)', true],
      ])
      expect(overflow(host.querySelector('nav[aria-label="Contents"]'))).toBe(0)
      expect(overflow(main)).toBe(0)
      expect(overflow(document.documentElement)).toBe(0)
    },
  )

  /**
   * **A link followed by a real click moves the view to its licence and
   * focus to its heading**, under the router the app runs, which leaves the
   * fragment in the address. The heading lands at the top of the view,
   * below the page's top, where its `scroll-margin-top` puts it. Focus is
   * put on the way up first, so the click is not the frame's first input.
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
      expect(landing(host, name)).toEqual({ heading: 0, headingBelowTop: true })
    },
  )

  /**
   * **A load of the page with a licence's fragment lands on that licence**
   * once the texts have drawn it, as following its link does.
   */
  it.each(CONTENTS)(
    'loaded with %s, puts %s at the top of the view and focus on its heading',
    async (fragment, name) => {
      const host = await loadedAt(1280, fragment)
      expect(focused()).toBe(name)
      expect(landing(host, name)).toEqual({ heading: 0, headingBelowTop: true })
    },
  )

  /**
   * **A return to the page with a licence's fragment lands on it too**, as
   * it draws the texts the tab has kept, in its first render.
   */
  it('loaded with #taiyaku-marks on a return, puts taiyaku marks at the top of the view and focus on its heading', async () => {
    const host = await loadedAt(1280, '#taiyaku-marks', 'a return')
    expect(focused()).toBe('taiyaku marks')
    expect(landing(host, 'taiyaku marks')).toEqual({
      heading: 0,
      headingBelowTop: true,
    })
  })
})

/**
 * **Back to top, on the heading's line or at the box's foot, pressed by a
 * click or by Enter, puts the page's top in view and focus on its title**,
 * from a load that landed on the last licence, far down the page. Both
 * scrollers are back at their tops and the address keeps no fragment. The
 * title is drawn landed on as a licence's heading is when a contents link,
 * pressed the same way, lands on it: with the same ring, or none, whose
 * look is read off that heading once the link has been followed.
 */
describe('Licence’s Back to top', () => {
  /** The ring `el` is drawn with, as its outline. */
  const ringOf = (el: Element) => {
    const style = getComputedStyle(el)
    return [
      style.outlineStyle,
      style.outlineWidth,
      style.outlineColor,
      style.outlineOffset,
    ].join(' ')
  }

  it.each([
    ['heading', 'a click'],
    ['heading', 'Enter'],
    ['foot', 'a click'],
    ['foot', 'Enter'],
  ] as const)(
    'at the %s, pressed by %s at 1280 px, puts the page’s top in view and focus on its title, with a contents link’s landing ring and no fragment',
    async (place, way) => {
      const host = await loadedAt(1280, '#libarchive')
      const main = host.querySelector('main')
      const title = host.querySelector('h1')
      if (!main || !title) throw new Error('no main or no title')
      const press = async (link: HTMLElement) => {
        if (way === 'a click') {
          await act(async () => {
            await userEvent.click(link)
          })
        } else {
          await act(async () => link.focus())
          await act(async () => {
            await userEvent.keyboard('{Enter}')
          })
        }
      }
      // The case's premise: the load landed far down the page, so `<main>`,
      // which scrolls at this width, is scrolled when the link is pressed.
      expect(main.scrollTop).toBeGreaterThan(0)
      await press(toTop(host, 'libarchive', place))
      const landed = {
        focus: document.activeElement === title,
        scrolled: [main.scrollTop, Math.round(scrollY)],
        fragment: location.hash,
        ring: ringOf(title),
      }

      const contents = host.querySelector<HTMLElement>(
        'nav[aria-label="Contents"] a[href="#taiyaku"]',
      )
      if (!contents) throw new Error('no contents link to #taiyaku')
      await press(contents)
      await expect.poll(focused).toBe('taiyaku')
      const heading = document.activeElement as Element
      const ring = ringOf(heading)
      // A landing is ringed where the key that made it was the keyboard's,
      // as the browser draws `:focus-visible`; after a click neither is.
      expect(getComputedStyle(heading).outlineStyle !== 'none').toBe(
        way === 'Enter',
      )
      expect(landed).toEqual({
        focus: true,
        scrolled: [0, 0],
        fragment: '',
        ring,
      })
    },
  )

  it('names each link by its words, without the arrow drawn before them', async () => {
    await page.viewport(1280, 844)
    await settle(inShell(<Licence />))
    const links = page.getByRole('link', { name: 'Back to top', exact: true })
    expect(links.elements()).toHaveLength(10)
    for (const link of links.elements()) {
      await expect.element(link).toHaveAccessibleName('Back to top')
    }
  })

  /**
   * **Each is a finger's 44 px under a coarse pointer**, as the page's other
   * links are: measured by hit testing along its middle row and column
   * (`pressArea`), its target is its box grown about its middle to 44 px
   * (`fingerArea`). Each is scrolled to the middle of the view first, since
   * the texts above them are longer than the window. The fine case shows
   * it is the coarse rules that reach 44.
   */
  // Coarse first at 1280: the fine case after it is what shows the coarse
  // rules were switched back.
  it.each([
    [1280, 'coarse'],
    [1280, 'fine'],
    [390, 'coarse'],
  ] as const)(
    'are a finger’s 44 px at %i px with a %s pointer',
    async (width, pointer) => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      await page.viewport(width, 844)
      const host = await settle(inShell(<Licence />))
      await document.fonts.ready
      for (const [name, place] of EVERY_TOP) {
        const link = toTop(host, name, place)
        link.scrollIntoView({ block: 'center' })
        const said = `${name}, ${place}`
        if (pointer === 'fine') {
          expect
            .soft(pressArea(link).height, `${said}: height`)
            .toBeLessThan(44)
        } else {
          expect.soft(pressArea(link), said).toEqual(near(fingerArea(link)))
        }
      }
    },
  )

  /**
   * **The heading's Back to top sits at the end of the heading's line**,
   * where the box's content ends below it, beside the heading's words and
   * never over them, and never breaks them
   * to make room: each heading's words take one line, as each name is
   * shorter than the line. Neither the link nor the 44 px a finger has of
   * it under a coarse pointer (`fingerArea`, its box grown about its
   * middle) meets the words. At 390 and 1280 every name shares its line
   * with the link, "Zen Kaku Gothic New", the longest, with the least room;
   * at 320 the two longest do not fit beside it, so the link goes under the
   * words, still at the end.
   */
  it.each([320, 390, 1280])(
    'sits at the end of each heading’s line, beside its words and not over them, at %i px',
    async (width) => {
      await page.viewport(width, 844)
      const host = await settle(inShell(<Licence />))
      await document.fonts.ready
      expect(
        CONTENTS.map(([, name]) => {
          const section = sectionOf(host, name)
          const heading = section.querySelector('h2')
          if (!heading) throw new Error(`no heading for ${name}`)
          const link = toTop(host, name, 'heading')
          const words = document.createRange()
          words.selectNodeContents(heading)
          const lines = new Set(
            [...words.getClientRects()].map((rect) => Math.round(rect.top)),
          ).size
          const drawn = areaOf(words.getBoundingClientRect())
          const box = link.getBoundingClientRect()
          return {
            name,
            lines,
            over: overlapOf(drawn, areaOf(box)),
            reach: overlapOf(drawn, fingerArea(link)),
            atEnd: Math.abs(box.right - contentEnd(section)) <= 1,
          }
        }),
      ).toEqual(
        CONTENTS.map(([, name]) => ({
          name,
          lines: 1,
          over: 0,
          reach: 0,
          atEnd: true,
        })),
      )
    },
  )
})

/**
 * **A licence's two Back to top links end on one edge**, the one on the
 * heading's line and the one at the box's foot, a heading's length apart
 * where one licence meets the next: the box's content's end, where every
 * row of a box ends.
 */
describe('Licence’s two Back to top links', () => {
  it.each([390, 1920])('end on one edge, at %i px', async (width) => {
    const was = [window.innerWidth, window.innerHeight] as const
    onTestFinished(() => page.viewport(...was))
    await page.viewport(width, 844)
    const host = await settle(inShell(<Licence />))
    await document.fonts.ready
    expect(
      CONTENTS.map(([, name]) => {
        const [heading, foot] = (['heading', 'foot'] as const).map((place) =>
          Math.round(toTop(host, name, place).getBoundingClientRect().right),
        )
        return { name, heading, foot }
      }),
    ).toEqual(
      CONTENTS.map(([, name]) => {
        const end = Math.round(contentEnd(sectionOf(host, name)))
        return { name, heading: end, foot: end }
      }),
    )
  })
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
      'libarchive (BSD-2-Clause)',
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
