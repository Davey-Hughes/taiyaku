import { afterEach, describe, expect, it } from 'vitest'
import { cdp } from 'vitest/browser'
// Required: the weight rule, the faces and the tokens all reach the page
// through `index.css`.
import '../src/index.css'
import { Chrome, type ChromeProps } from '../src/Chrome'
import { mount } from './support/mount'

/**
 * Ink's UI text at weight 500 (spec 2026-10-18 §3): light strokes on a dark
 * ground read thinner than the same strokes on paper. Ink is in force when it
 * is chosen, or when the choice is System and the device prefers dark; the
 * rule is `base.css`'s, on `.library` and `.login`, and reaches neither the
 * titles' face nor the reader.
 *
 * A computed weight says what was asked for, not what was drawn: with no 500
 * face the browser draws the nearest lighter one and `getComputedStyle`
 * still answers 500. So the first case asks the document for the UI face's
 * 500 files: none loaded before it draws the screens under Ink, and one
 * loaded once they are laid out.
 *
 * The device's preference is set over the DevTools session, the same one
 * `support/mouse.ts` sends input through, and cleared after each case: it is
 * the page's, not this frame's, and would outlive the file.
 */

type Theme = 'ink' | 'paper' | 'system'
type Preference = 'dark' | 'light'

async function prefer(preference: Preference | '') {
  await cdp().send('Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-color-scheme', value: preference }],
  })
}

afterEach(async () => {
  document.documentElement.removeAttribute('data-theme')
  await prefer('')
})

/** Sets the theme as `theme.ts` applies it: System is no attribute at all. */
async function under(theme: Theme, preference: Preference) {
  if (theme === 'system') document.documentElement.removeAttribute('data-theme')
  else document.documentElement.setAttribute('data-theme', theme)
  await prefer(preference)
  // The emulation is what every case below rests on; a session that ignored
  // it would leave each System case reading the browser's default.
  expect(matchMedia(`(prefers-color-scheme: ${preference})`).matches).toBe(true)
}

/** The library's and the sign-in screen's own text, and a title of each kind. */
function screens(): HTMLElement {
  return mount(
    <>
      <div className="library">
        <main>
          <h1>Library</h1>
          <h2>Libraries</h2>
          <p className="read">Fourteen volumes in two editions</p>
          <button type="button">Scan library</button>
          <input aria-label="Search" defaultValue="oshi" />
        </main>
      </div>
      <main className="login">
        <p className="read">Sign in to read</p>
      </main>
    </>,
  )
}

/** The UI text's computed weights, by what each probe is. */
function uiWeights(host: HTMLElement): Record<string, string> {
  const weight = (selector: string) => {
    const el = host.querySelector(selector)
    if (!el) throw new Error(`no ${selector} mounted`)
    return getComputedStyle(el).fontWeight
  }
  return {
    text: weight('.library .read'),
    button: weight('.library button'),
    input: weight('.library input'),
    login: weight('.login .read'),
  }
}

const at = (weight: string) => ({
  text: weight,
  button: weight,
  input: weight,
  login: weight,
})

/** The status of each of the UI face's files at 500 in this document. */
function uiFaceAt500(): string[] {
  const statuses: string[] = []
  document.fonts.forEach((face) => {
    if (
      face.family.replace(/"/g, '') === 'Zen Kaku Gothic New' &&
      face.weight === '500'
    ) {
      statuses.push(face.status)
    }
  })
  return statuses
}

// First in the file: `document.fonts` is the document's, and a case before
// this one that drew at 500 could have loaded the face already.
describe("Ink's UI face", () => {
  it('loads its own 500 file for the screens it draws', async () => {
    const before = uiFaceAt500()
    expect(before.length).toBeGreaterThan(0)
    expect(before.filter((status) => status !== 'unloaded')).toEqual([])
    await under('ink', 'light')
    const host = screens()
    // Layout first, so the text on screen has asked for its faces before
    // `ready` is awaited; awaited before layout, it can resolve at once.
    host.getBoundingClientRect()
    await document.fonts.ready
    expect(uiFaceAt500()).toContain('loaded')
  })
})

describe("Ink's UI text weight", () => {
  it.each([
    ['ink', 'light'],
    ['ink', 'dark'],
    ['system', 'dark'],
  ] as const)('is 500 under %s with a %s preference', async (theme, pref) => {
    await under(theme, pref)
    expect(uiWeights(screens())).toEqual(at('500'))
  })

  it.each([
    ['paper', 'dark'],
    ['paper', 'light'],
    ['system', 'light'],
  ] as const)('is 400 under %s with a %s preference', async (theme, pref) => {
    await under(theme, pref)
    expect(uiWeights(screens())).toEqual(at('400'))
  })

  it('leaves the titles at their face’s 600', async () => {
    await under('ink', 'light')
    const host = screens()
    for (const heading of ['h1', 'h2']) {
      const el = host.querySelector(heading)
      if (!el) throw new Error(`no ${heading} mounted`)
      expect(getComputedStyle(el).fontWeight, heading).toBe('600')
      expect(getComputedStyle(el).fontFamily, heading).toMatch(
        /^"Shippori Mincho B1"/,
      )
    }
  })
})

/** The reader's bars, up, as `Chrome.browser.test.tsx` mounts them. */
function readerBars(): HTMLElement {
  const slots = [{ page: 0 }, { page: 1 }, { page: 2 }]
  const props: ChromeProps = {
    visible: true,
    motion: { animate: false, ms: 0 },
    readouts: {
      position: true,
      time: false,
      chapter: false,
      health: false,
      notes: false,
    },
    position: 'slot 1',
    chapter: '',
    progress: 0.25,
    title: 'Go-Toubun no Hanayome',
    editionLine: 'Raw',
    mode: 'single',
    onMode: () => {},
    settingsOpen: false,
    onSettings: () => {},
    onLeave: () => {},
    reading: 'ltr',
    work: 'w',
    edition: 'e',
    slots,
    groups: slots.map((_, i) => ({ slots: [i] })),
    currentIndex: 0,
    onNavigate: () => {},
  }
  return mount(
    <div className="reader" style={{ position: 'fixed', inset: 0 }}>
      <Chrome {...props} />
    </div>,
  )
}

describe('the reader under Ink', () => {
  it('draws its bars as it did before the lift', async () => {
    await under('ink', 'light')
    const host = readerBars()
    const drawn = (selector: string) => {
      const el = host.querySelector(selector)
      if (!el) throw new Error(`no ${selector} in the bars`)
      const style = getComputedStyle(el)
      return [
        style.fontWeight,
        style.color,
        style.fontSize,
        style.fontFamily.split(',')[0],
      ].join(' ')
    }
    expect({
      work: drawn('.reader-title-work'),
      edition: drawn('.reader-title-edition'),
      readout: drawn('.reader-readout'),
      back: drawn('.reader-back'),
    }).toEqual({
      work: '600 rgb(236, 228, 212) 16px "Shippori Mincho B1"',
      edition: '400 rgb(166, 155, 135) 12px "taiyaku marks"',
      readout: '400 rgb(166, 155, 135) 12px "taiyaku marks"',
      back: '400 rgb(166, 155, 135) 14px "taiyaku marks"',
    })
  })
})
