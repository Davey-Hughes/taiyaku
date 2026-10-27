import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { describe, expect, it, onTestFinished } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { About } from './About'
import type { About as AboutAnswer } from './about'
import './index.css'
import { ABOUT } from './testing/about'
import { answerAbout, inShell } from './testing/aboutPage'
import { clipboardContext } from './testing/clipboard'
import { mount } from './testing/mount'
import {
  coarseRulesOn,
  fingerArea,
  near,
  overlapOf,
  pressArea,
} from './testing/pointer'

/**
 * About at `width` × `height`, inside the shell's `<main>` (`inShell`), once
 * `/api/about` has answered with `answer` and the faces have loaded: a face
 * arriving between two measurements moves what the second one reads.
 */
async function aboutAt(
  width: number,
  height: number,
  answer: AboutAnswer = ABOUT,
): Promise<HTMLElement> {
  await page.viewport(width, height)
  answerAbout(answer)
  const host = mount(<MemoryRouter>{inShell(<About />)}</MemoryRouter>)
  await act(async () => {
    await new Promise((resolve) => {
      setTimeout(resolve, 0)
    })
  })
  await document.fonts.ready
  return host
}

/** The term `term` in the build's facts, and the value beside it. */
function fact(host: HTMLElement, term: string): [HTMLElement, HTMLElement] {
  const dt = [...host.querySelectorAll('.about-facts dt')].find(
    (el) => el.textContent === term,
  )
  const dd = dt?.nextElementSibling
  if (!(dt instanceof HTMLElement) || !(dd instanceof HTMLElement)) {
    throw new Error(`no fact named "${term}"`)
  }
  return [dt, dd]
}

/** The one element `selector` finds inside `el`. */
function one(el: Element, selector: string): HTMLElement {
  const found = el.querySelectorAll(selector)
  if (found.length !== 1 || !(found[0] instanceof HTMLElement)) {
    throw new Error(`${found.length} of "${selector}", not one`)
  }
  return found[0]
}

/**
 * **The catalog's Copy address button is a finger's 44 px under a coarse
 * pointer** (spec 2026-09-17 §7.1), from `admin.css`'s floor for the
 * settings area's buttons. In the browser project because jsdom computes no
 * heights, and with the coarse blocks switched on in place because this
 * project's Chromium has a fine pointer. The fine case is what shows the
 * floor, and not the button's own box, is what reaches 44.
 */
describe('About’s Copy address button', () => {
  /**
   * **What a screen reader is told of it**, as vitest's locator and
   * matchers compute it in the page from the DOM and its computed styles,
   * not as Chromium's accessibility tree holds it: the name says what it
   * copies, and the description is the address itself.
   */
  it('is named for what it copies and described by the address', async () => {
    await aboutAt(1280, 900)
    const button = page.getByRole('button', {
      name: 'Copy address',
      exact: true,
    })
    expect(button.elements()).toHaveLength(1)
    await expect.element(button).toHaveAccessibleName('Copy address')
    await expect
      .element(button)
      .toHaveAccessibleDescription(`${location.origin}${ABOUT.catalog}`)
  })

  // Coarse first: the fine case after it is what shows the coarse rules
  // were switched back.
  it.each([
    ['coarse', true],
    ['fine', false],
  ] as const)('with a %s pointer', async (pointer, floored) => {
    if (pointer === 'coarse') onTestFinished(coarseRulesOn())
    const host = await aboutAt(390, 844)
    // This project's Chromium serves the test page from localhost, a secure
    // context with a clipboard, so the button is drawn without a stub.
    const buttons = [...host.querySelectorAll('.about-facts button')]
    expect(buttons.map((button) => button.textContent)).toEqual([
      'Copy address',
    ])
    const height = buttons[0]?.getBoundingClientRect().height ?? 0
    if (floored) expect(height).toBeGreaterThanOrEqual(44)
    else expect(height).toBeLessThan(44)
  })
})

/**
 * The clipboard a press of Copy address writes to, with each write's
 * promise kept, so `press` can wait inside `act` for the write to settle:
 * `writeText` answers each write, or, left out, this browser's own clipboard
 * does, and headless Chromium refuses it.
 */
function clipboardFor(
  writeText?: (text: string) => Promise<void>,
): Promise<void>[] {
  const own = navigator.clipboard
  const writes: Promise<void>[] = []
  clipboardContext(true, {
    writeText: (text: string) => {
      const write = writeText ? writeText(text) : own.writeText(text)
      writes.push(write)
      return write
    },
  })
  return writes
}

/** A real click on `button`, and the one write it starts, settled. */
async function press(button: HTMLElement, writes: Promise<void>[]) {
  const before = writes.length
  await act(async () => {
    await userEvent.click(button)
    await Promise.allSettled(writes)
  })
  expect(writes).toHaveLength(before + 1)
}

/**
 * The words painted on `button`: each run of its text, and each generated
 * content inside it, that is laid out, not `visibility: hidden`, not under
 * an `opacity` of 0 and not under a clip, which on this button clips to
 * nothing. What a sighted person reads on it.
 */
function drawnWords(button: HTMLElement): string[] {
  const unclipped = (el: Element) => {
    for (let at: Element | null = el; at; at = at.parentElement) {
      if (getComputedStyle(at).clipPath !== 'none') return false
      if (at === button) return true
    }
    throw new Error('not inside the button')
  }
  const drawn: string[] = []
  for (const el of [button, ...button.querySelectorAll<HTMLElement>('*')]) {
    const shown = el.checkVisibility({ opacityProperty: true }) && unclipped(el)
    for (const pseudo of ['::before', '::after']) {
      const style = getComputedStyle(el, pseudo)
      if (
        shown &&
        style.content.startsWith('"') &&
        style.display !== 'none' &&
        style.visibility === 'visible' &&
        style.opacity !== '0' &&
        style.clipPath === 'none'
      ) {
        drawn.push(style.content.slice(1, -1))
      }
    }
    const text = [...el.childNodes]
      .filter((node) => node.nodeType === Node.TEXT_NODE)
      .map((node) => node.textContent ?? '')
      .join('')
      .trim()
    if (
      text !== '' &&
      shown &&
      el.checkVisibility({ visibilityProperty: true })
    ) {
      drawn.push(text)
    }
  }
  return drawn
}

/** About's section headed `heading`. */
function section(host: HTMLElement, heading: string): HTMLElement {
  const found = [
    ...host.querySelectorAll<HTMLElement>('.settings-section'),
  ].find((el) => el.querySelector('h2')?.textContent === heading)
  if (!found) throw new Error(`no ${heading} section`)
  return found
}

/**
 * The catalog's line on About: the address and Copy address in their row,
 * and the live region that says what a press did.
 */
function catalogLine(host: HTMLElement) {
  const [, dd] = fact(host, 'OPDS catalog')
  return {
    row: one(dd, '.catalog-address'),
    address: one(dd, '.catalog-address-text'),
    button: one(dd, 'button'),
    region: one(dd, '[role="status"]'),
  }
}

/**
 * How wide `words` are laid out alone on one line in `el`'s font: drawn in
 * a box of their own inside `el`, out of its flow, so `el` keeps its own
 * width while they are measured.
 */
function alone(el: HTMLElement, words: string): number {
  const probe = document.createElement('span')
  probe.style.position = 'absolute'
  probe.style.whiteSpace = 'nowrap'
  probe.textContent = words
  el.append(probe)
  const width = probe.getBoundingClientRect().width
  probe.remove()
  return width
}

/**
 * An answer whose catalog address, at this frame's origin and whatever its
 * port, is 36 characters long: at 1280 px, in the address's monospace face,
 * it shares its line with Copy address and leaves less room after the
 * button than the gap a third item on the line would need. The line is the
 * This build box's, whose width the settings components set.
 */
const fillingTheLine = (): AboutAnswer => ({
  ...ABOUT,
  catalog: `/opds/${'v'.repeat(36 - location.origin.length - '/opds/'.length)}`,
})

/**
 * **A press changes the word on Copy address and moves nothing.** With an
 * address that fills its line (`fillingTheLine`), anything beside the
 * button in the row's flow, even an empty live region, wraps onto a line of
 * its own and pushes Formats and Licence down. The row is the button's
 * height before the press and after it, and the button keeps its width
 * while it says what the press did. Headless Chromium refuses the write,
 * which says "Not copied"; a clipboard that takes it says "Copied".
 */
describe('About’s Copy address, pressed', () => {
  it.each([
    ['this browser’s own clipboard', undefined, 'Not copied'],
    ['a clipboard that takes the write', () => Promise.resolve(), 'Copied'],
  ] as const)(
    'moves nothing at 1280 px with %s',
    async (_, writeText, said) => {
      const writes = clipboardFor(writeText)
      const host = await aboutAt(1280, 900, fillingTheLine())
      const { row, address, button, region } = catalogLine(host)
      const formats = section(host, 'Formats')
      const room =
        row.getBoundingClientRect().right - button.getBoundingClientRect().right
      // The case's premise: the button shares the address's line, with too
      // little room after it for anything else.
      expect(button.getBoundingClientRect().top).toBeLessThan(
        address.getBoundingClientRect().bottom,
      )
      expect(room).toBeLessThan(
        Number.parseFloat(getComputedStyle(row).columnGap),
      )
      const measure = () => ({
        row: row.getBoundingClientRect().height,
        button: button.getBoundingClientRect().height,
        width: button.getBoundingClientRect().width,
        formats: formats.getBoundingClientRect().top,
      })
      const before = measure()
      expect.soft(before.row, 'the row before the press').toBe(before.button)

      await press(button, writes)
      expect.soft(region.textContent).toBe(said)
      expect.soft(drawnWords(button)).toEqual([said])
      expect.soft(measure(), 'after the press').toEqual(before)
    },
  )

  /**
   * **As wide in every state as the widest of its words**, so the button
   * does not narrow under the address when it says "Copied", or move what
   * is beside it. Its content box is measured against each word laid out
   * alone on one line in the button's font, so a width set on it, which
   * would hold in every state too, is not taken for its words'.
   */
  it('keeps its width at rest, saying "Copied" and saying "Not copied"', async () => {
    let refuse = false
    const writes = clipboardFor(() =>
      refuse
        ? Promise.reject(new DOMException('Refused.', 'NotAllowedError'))
        : Promise.resolve(),
    )
    const host = await aboutAt(1280, 900)
    const { button } = catalogLine(host)
    const widths: Record<string, number> = {}
    const read = () => {
      widths[drawnWords(button).join(' + ')] =
        button.getBoundingClientRect().width
    }
    read()
    await press(button, writes)
    read()
    refuse = true
    await press(button, writes)
    read()
    const rest = widths['Copy address'] ?? 0
    expect(rest).toBeGreaterThan(0)
    expect(widths).toEqual({
      'Copy address': rest,
      Copied: rest,
      'Not copied': rest,
    })
    const style = getComputedStyle(button)
    const content =
      rest -
      [
        style.paddingLeft,
        style.paddingRight,
        style.borderLeftWidth,
        style.borderRightWidth,
      ].reduce((sum, length) => sum + Number.parseFloat(length), 0)
    const widest = Math.max(
      ...['Copy address', 'Copied', 'Not copied'].map((words) =>
        alone(button, words),
      ),
    )
    expect(content).toBeCloseTo(widest, 1)
  })

  /**
   * **What a screen reader is told, while the button says "Copied"**: its
   * name is still what it copies, as vitest's `toHaveAccessibleName`
   * computes it in the page from the DOM and its computed styles, and the
   * result is the live region's text, which is laid out nowhere, so its
   * text arriving moves nothing.
   */
  it('keeps its name and says the result in a region out of the flow', async () => {
    const writes = clipboardFor(() => Promise.resolve())
    const host = await aboutAt(1280, 900)
    const { button, region } = catalogLine(host)
    await press(button, writes)
    expect(drawnWords(button)).toEqual(['Copied'])
    // Read once, not polled: a poll would pass once the word had gone back
    // to "Copy address", two seconds on.
    expect(button).toHaveAccessibleName('Copy address')
    expect(drawnWords(button)).toEqual(['Copied'])
    expect(region.textContent).toBe('Copied')
    const box = region.getBoundingClientRect()
    expect({
      position: getComputedStyle(region).position,
      width: box.width <= 1,
      height: box.height <= 1,
    }).toEqual({ position: 'absolute', width: true, height: true })
  })
})

/**
 * **Every link on About that stands alone is a finger's 44 px under a
 * coarse pointer** (spec 2026-09-17 §7.1): the commit and Source, the
 * Licence section's two links, and Copy address beside them. Under a coarse
 * pointer each target, measured by hit testing along its middle row and
 * column (`pressArea`), is its box grown about its middle to 44 px
 * (`fingerArea`): so no other target is drawn over any of it on those
 * lines, and each is at least 44 each way. Those grown boxes are then
 * checked not to overlap at all, corners included. Down to 320, the
 * narrowest phone, where a sentence wrapped before its second link would
 * stack the two licence links' targets. A window tall enough to hold the
 * whole page, so every area is measured where it is drawn and none of them
 * scrolls. The fine case shows it is the coarse rules that reach 44.
 */
describe('About’s links', () => {
  // Coarse first at 1280: the fine case after it is what shows the coarse
  // rules were switched back.
  it.each([
    [1280, 'coarse'],
    [1280, 'fine'],
    [390, 'coarse'],
    [360, 'coarse'],
    [320, 'coarse'],
  ] as const)('at %i px with a %s pointer', async (width, pointer) => {
    if (pointer === 'coarse') onTestFinished(coarseRulesOn())
    const host = await aboutAt(width, 2400)
    // The case's premise: the whole page is in the window, so every target
    // is measured where it is drawn.
    expect(host.getBoundingClientRect().bottom).toBeLessThanOrEqual(innerHeight)
    const licence = [...section(host, 'Licence').querySelectorAll('a')]
    expect(licence.map((link) => link.textContent)).toEqual([
      'GPL-3.0-or-later',
      'OFL-1.1',
    ])
    const links = {
      Commit: one(fact(host, 'Commit')[1], 'a'),
      Source: one(fact(host, 'Source')[1], 'a'),
      'GPL-3.0-or-later': licence[0] as HTMLElement,
      'OFL-1.1': licence[1] as HTMLElement,
    }
    if (pointer === 'fine') {
      for (const [name, link] of Object.entries(links)) {
        expect.soft(pressArea(link).height, `${name}: height`).toBeLessThan(44)
      }
      return
    }
    const targets = Object.entries({
      ...links,
      'Copy address': one(fact(host, 'OPDS catalog')[1], 'button'),
    })
    for (const [name, el] of targets) {
      expect.soft(pressArea(el), name).toEqual(near(fingerArea(el)))
    }
    for (const [i, [name, el]] of targets.entries()) {
      for (const [other, its] of targets.slice(i + 1)) {
        expect
          .soft(overlapOf(fingerArea(el), fingerArea(its)), `${name}, ${other}`)
          .toBe(0)
      }
    }
  })
})

/**
 * Where the first line of `el` is drawn: a box of no size put at its start
 * sits on that line's baseline, so its bottom edge is the baseline.
 */
function baselineOf(el: Element): number {
  const probe = document.createElement('span')
  probe.style.display = 'inline-block'
  el.prepend(probe)
  const at = probe.getBoundingClientRect().bottom
  probe.remove()
  return at
}

/**
 * **Each of the build's facts is on the line of the word that names it**,
 * the catalog's address as well as the links. Where the line has room the
 * address shares it with Copy address, which is taller than the address,
 * so the address is lower in its row than a value alone would be; at 390
 * the button wraps under the address. Its baseline is measured
 * inside the `code`, because the value holds the address in a flex box
 * rather than in a line of its own; the others in the value's own line,
 * which a link grown to a finger's height sits on. At 390 as well as 1280,
 * because there Source and the catalog's address each wrap onto a second
 * line: a label centred on its row, or set on its value's last baseline,
 * is on its value's line only where every value is one line. Both
 * pointers, since a coarse one grows Copy address and the links, and makes
 * each link one box with one baseline however many lines it wraps to.
 */
describe('About’s facts', () => {
  it.each([
    [1280, 'coarse'],
    [1280, 'fine'],
    [390, 'coarse'],
    [390, 'fine'],
  ] as const)(
    'sit on their labels’ lines at %i px with a %s pointer',
    async (width, pointer) => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      const host = await aboutAt(width, 1600)
      // The 390 cases' premise: Source and the catalog's address each take
      // more than one line there.
      if (width === 390) {
        const source = one(fact(host, 'Source')[1], 'a')
        const address = one(
          fact(host, 'OPDS catalog')[1],
          '.catalog-address-text',
        )
        expect({
          source: textOf(source).lines > 1,
          address: textOf(address).lines > 1,
        }).toEqual({ source: true, address: true })
      }
      for (const term of ['Commit', 'Source', 'OPDS catalog']) {
        const [dt, dd] = fact(host, term)
        const value =
          term === 'OPDS catalog' ? one(dd, '.catalog-address-text') : dd
        expect
          .soft(
            Math.abs(baselineOf(dt) - baselineOf(value)),
            `${term}: label against value`,
          )
          .toBeLessThanOrEqual(1)
      }
    },
  )
})

/** The box the text inside `node` is drawn in, and the lines it takes. */
function textOf(node: Node): { box: DOMRect; lines: number } {
  const range = document.createRange()
  range.selectNodeContents(node)
  const tops = new Set(
    [...range.getClientRects()].map((rect) => Math.round(rect.top)),
  )
  return { box: range.getBoundingClientRect(), lines: tops.size }
}

/** The colour `value` computes to inside `el`, where `el`'s own scheme applies. */
function colourIn(el: HTMLElement, value: string): string {
  const probe = document.createElement('span')
  probe.style.color = value
  el.append(probe)
  const colour = getComputedStyle(probe).color
  probe.remove()
  return colour
}

/**
 * **Each format's sentence is drawn under its name, quiet by colour and at
 * the 14px floor, and wraps rather than widening the page.** `--muted` is
 * the colour whose floors `index.css.test.ts` holds in both themes, so the
 * sentence keeps them by being that colour. At 390, where a sentence is
 * longer than the box is wide, so the wrap is what keeps the page from
 * scrolling sideways; at 1280 too, where the name and the sentence share
 * a wider cell beside the version.
 */
describe('About’s formats', () => {
  it.each([390, 1280])(
    'draw each sentence under its name, muted at 14 px, at %i px',
    async (width) => {
      const host = await aboutAt(width, 900)
      const formats = section(host, 'Formats')
      const right = formats.getBoundingClientRect().right
      const rows = [...formats.querySelectorAll('tbody tr')]
      expect(rows).toHaveLength(ABOUT.formats.length)
      let wrapped = 0
      for (const row of rows) {
        const lead = one(row, 'td.lead')
        const sentence = one(lead, '.about-format-description')
        const name = one(lead, '.about-format-name')
        const said = name.textContent
        const style = getComputedStyle(sentence)
        const text = textOf(sentence)
        if (text.lines > 1) wrapped++
        expect
          .soft(Number.parseFloat(style.fontSize), `${said}: size`)
          .toBeGreaterThanOrEqual(14)
        expect
          .soft(style.color, `${said}: colour`)
          .toBe(colourIn(lead, 'var(--muted)'))
        expect
          .soft(text.box.top, `${said}: under its name`)
          .toBeGreaterThanOrEqual(textOf(name).box.bottom)
        expect
          .soft(text.box.right, `${said}: inside the box`)
          .toBeLessThanOrEqual(right)
      }
      expect
        .soft(document.documentElement.scrollWidth, 'the page’s width')
        .toBeLessThanOrEqual(document.documentElement.clientWidth)
      // The case's premise at a phone's width: at least one sentence is
      // longer than its line, so it has to wrap to fit.
      if (width === 390)
        expect.soft(wrapped, 'sentences wrapped').toBeGreaterThan(0)
    },
  )
})

/**
 * **Each format's name is drawn at 600 in both themes, and its sentence at
 * the weight the page's text has in each**: 400 in Paper, and in Ink the
 * 500 that `base.css` sets on `.library` for the sentence to inherit. Under
 * Ink the name's own rule is what holds it at 600, since a weight set on an
 * element wins over the one it would inherit. A computed weight is what was
 * asked for: the UI face has no 600 file, so 600 draws its 700 one, as the
 * row links' 600 does.
 */
describe('About’s format names', () => {
  it.each([
    ['paper', '400'],
    ['ink', '500'],
  ] as const)(
    'are 600 in %s, and their sentences %s',
    async (theme, sentence) => {
      document.documentElement.setAttribute('data-theme', theme)
      onTestFinished(() =>
        document.documentElement.removeAttribute('data-theme'),
      )
      const host = await aboutAt(1280, 900)
      const rows = [...section(host, 'Formats').querySelectorAll('tbody tr')]
      expect(
        rows.map((row) => {
          const lead = one(row, 'td.lead')
          const name = one(lead, '.about-format-name')
          return [
            name.textContent,
            getComputedStyle(name).fontWeight,
            getComputedStyle(one(lead, '.about-format-description')).fontWeight,
          ]
        }),
      ).toEqual(ABOUT.formats.map((format) => [format.name, '600', sentence]))
    },
  )
})
