import { afterEach, describe, expect, it, vi } from 'vitest'
import indexHtml from '../index.html?raw'
import tokensCss from '../src/styles/tokens.css?raw'
import { rules } from './support/css'
import { fakeStorage } from './support/storage'
import {
  applyTheme,
  loadTheme,
  saveTheme,
  THEME_CHOICES,
  type ThemeChoice,
} from '../src/theme'

/** An element that records only what `applyTheme` writes. */
function fakeRoot() {
  const attributes = new Map<string, string>()
  return {
    attributes,
    setAttribute: (name: string, value: string) => {
      attributes.set(name, value)
    },
    removeAttribute: (name: string) => {
      attributes.delete(name)
    },
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('theme', () => {
  it('follows the OS until a reader chooses', () => {
    vi.stubGlobal('localStorage', fakeStorage())
    expect(loadTheme()).toBe('system')
  })

  it('remembers a choice under taiyaku.prefs.theme', () => {
    const storage = fakeStorage()
    vi.stubGlobal('localStorage', storage)
    saveTheme('ink')
    // The literal key, not one built from `NAMESPACE` and `THEME_PREF_ID`:
    // `index.html`'s pre-paint script spells it out, so a renamed id that
    // still agreed with itself would strand every stored choice.
    expect(storage.getItem('taiyaku.prefs.theme')).toBe('{"choice":"ink"}')
    expect(loadTheme()).toBe('ink')
  })

  it('reads a stored choice it does not know as the OS default', () => {
    const storage = fakeStorage()
    storage.setItem('taiyaku.prefs.theme', '{"choice":"sepia"}')
    vi.stubGlobal('localStorage', storage)
    expect(loadTheme()).toBe('system')
  })

  it('pins paper and ink as data-theme, and leaves system to the OS', () => {
    const root = fakeRoot()
    applyTheme('ink', root)
    expect(root.attributes.get('data-theme')).toBe('ink')
    applyTheme('paper', root)
    expect(root.attributes.get('data-theme')).toBe('paper')
    applyTheme('system', root)
    expect(root.attributes.has('data-theme')).toBe(false)
  })
})

describe("index.html's pre-paint script", () => {
  const script = indexHtml.match(/<script>([\s\S]*?)<\/script>/)?.[1]

  const STORED: Array<[string, string | null]> = [
    ['nothing', null],
    ['paper', '{"choice":"paper"}'],
    ['ink', '{"choice":"ink"}'],
    ['system', '{"choice":"system"}'],
    ['a choice it does not know', '{"choice":"sepia"}'],
    ['a value that is not JSON', '{not json'],
    ['JSON that is not an object', '"ink"'],
  ]

  it.each(STORED)(
    'agrees with loadTheme and applyTheme when %s is stored',
    (_, stored) => {
      if (script === undefined) throw new Error('index.html has no <script>')
      const storage = fakeStorage()
      if (stored !== null) storage.setItem('taiyaku.prefs.theme', stored)

      const painted = fakeRoot()
      new Function('localStorage', 'document', script)(storage, {
        documentElement: painted,
      })

      vi.stubGlobal('localStorage', storage)
      const applied = fakeRoot()
      applyTheme(loadTheme(), applied)

      expect(painted.attributes).toEqual(applied.attributes)
    },
  )

  it('comes before the app module, so it runs before first paint', () => {
    const inline = indexHtml.indexOf('<script>')
    expect(inline).toBeGreaterThan(-1)
    expect(inline).toBeLessThan(indexHtml.indexOf('<script type="module"'))
  })
})

describe("index.html's theme-color", () => {
  it("is each theme's --bg", () => {
    const bg = tokensCss.match(
      /--bg:\s*light-dark\((#[0-9a-f]{6}),\s*(#[0-9a-f]{6})\)/,
    )
    const meta = (scheme: string) =>
      indexHtml.match(
        new RegExp(
          `<meta name="theme-color" content="(#[0-9a-f]{6})" media="\\(prefers-color-scheme: ${scheme}\\)"`,
        ),
      )?.[1]
    expect(bg).not.toBeNull()
    expect(meta('light')).toBe(bg?.[1])
    expect(meta('dark')).toBe(bg?.[2])
  })
})

/**
 * **`tokens.css` is what turns the choice into a palette**: `applyTheme` only
 * writes `data-theme`, and a rule per pinned choice turns that into
 * `color-scheme`, which every `light-dark()` token resolves against. So this
 * reads the rules themselves.
 */
describe("tokens.css's color-scheme rules", () => {
  type Pinned = Exclude<ThemeChoice, 'system'>

  /** The half each pinned choice must pick. A new choice fails to compile. */
  const SCHEME: Record<Pinned, string> = {
    paper: 'light',
    ink: 'dark',
  }

  /** Every `color-scheme` declared in a rule whose selector is `selector`. */
  function colorSchemes(selector: string): string[] {
    return rules(tokensCss)
      .filter((rule) => rule.prelude === selector)
      .flatMap((rule) =>
        [...rule.body.matchAll(/(?:^|;)\s*color-scheme:\s*([^;]+)/g)].map((m) =>
          m[1].trim(),
        ),
      )
  }

  it('follows the OS when nothing is pinned', () => {
    expect(colorSchemes(':root')).toEqual(['light dark'])
  })

  const pinned = THEME_CHOICES.filter(
    (choice): choice is Pinned => choice !== 'system',
  )

  it('runs for paper and ink, not for an empty list', () => {
    expect(pinned).toEqual(['paper', 'ink'])
  })

  it.each(pinned)('pins data-theme="%s" to its own half', (choice) => {
    expect(colorSchemes(`:root[data-theme="${choice}"]`)).toEqual([
      SCHEME[choice],
    ])
  })
})
