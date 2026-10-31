import { afterEach, describe, expect, it } from 'vitest'
// **Required**: `--font-ui`, the faces and `body`'s font all reach the page
// through `index.css`. Without it every span below is in the browser's
// default face and measures nothing about this app.
import '../src/index.css'

/**
 * O179: the seven spacing characters Zen Kaku Gothic New draws with no
 * advance, and the `"taiyaku marks"` faces in `styles/tokens.css` that give
 * them one. A server sentence quotes a name in backticks — ``no work `nope` ``
 * — and in Zen Kaku alone both backticks would be drawn over the letters
 * beside them.
 *
 * Each weight of both families is asked for by name with
 * `document.fonts.load` before `document.fonts.ready` is awaited and anything
 * is measured.
 *
 * CI does not run this project (see `vite.config.ts`: no Chromium);
 * `scripts/check-all.sh` does.
 */

const MARKS = ['`', '¨', '´', '¸', 'ˆ', '˚', '˜']
const WEIGHTS = [400, 500, 700]
/** The stack of `--font-ui` without the marks faces: the defect's own. */
const ZEN_KAKU_ALONE = '"Zen Kaku Gothic New", system-ui, sans-serif'

let host: HTMLElement | undefined
afterEach(() => {
  host?.remove()
  host = undefined
})

/** Each text's drawn width at `weight`, in `family` or else `body`'s face. */
async function widths(
  weight: number,
  texts: string[],
  family?: string,
): Promise<number[]> {
  host = document.createElement('div')
  if (family !== undefined) host.style.fontFamily = family
  document.body.append(host)
  const spans = texts.map((text) => {
    const span = document.createElement('span')
    span.textContent = text
    span.style.fontWeight = String(weight)
    span.style.whiteSpace = 'pre'
    host?.append(span, document.createElement('br'))
    return span
  })
  await document.fonts.load(
    `${weight} 16px "Zen Kaku Gothic New"`,
    texts.join(''),
  )
  await document.fonts.load(`${weight} 16px "taiyaku marks"`, MARKS.join(''))
  await document.fonts.ready
  return spans.map((span) => span.getBoundingClientRect().width)
}

describe('a quoted name in the interface face', () => {
  // The control: the defect is in this browser, in Zen Kaku alone. If Zen Kaku
  // did not load, or a later release gives the backtick its advance, this
  // fails, and the two below stop showing that the marks faces are what makes
  // the difference.
  it.each(WEIGHTS)(
    'draws a backtick with no width in Zen Kaku alone, at %i',
    async (weight) => {
      const [quoted, bare] = await widths(
        weight,
        ['`nope`', 'nope'],
        ZEN_KAKU_ALONE,
      )
      expect(bare).toBeGreaterThan(30)
      expect(Math.abs(quoted - bare)).toBeLessThan(0.5)
    },
  )

  it.each(WEIGHTS)(
    'gives both backticks a width in --font-ui, at %i',
    async (weight) => {
      const [quoted, bare] = await widths(weight, ['`nope`', 'nope'])
      // Each backtick is 5.6 to 6 px at 16 px.
      expect(quoted - bare).toBeGreaterThan(10)
    },
  )

  it.each(WEIGHTS)(
    'gives each of the seven a width in --font-ui, at %i',
    async (weight) => {
      const [plain, ...withMark] = await widths(weight, [
        'ab',
        ...MARKS.map((mark) => `a${mark}b`),
      ])
      const narrow = MARKS.filter(
        (_, i) => (withMark[i] ?? 0) - (plain ?? 0) < 4,
      )
      expect(narrow).toEqual([])
    },
  )

  // Which weight's file drew. The backtick cannot say — 5.6 to 6 px at every
  // weight — but `˚` is 7.95, 6.56 and 5.46 px at 400, 500 and 700, more than
  // a pixel apart. A face pointed at another weight's file draws another
  // weight's ring.
  it.each([
    [400, 7.95],
    [500, 6.56],
    [700, 5.46],
  ])(
    'draws ˚ in --font-ui at its own weight’s width, at %i',
    async (weight, px) => {
      const [plain, ring] = await widths(weight, ['ab', 'a˚b'])
      expect(Math.abs((ring ?? 0) - (plain ?? 0) - px)).toBeLessThan(0.6)
    },
  )
})
