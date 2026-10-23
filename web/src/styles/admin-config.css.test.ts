import { describe, expect, it } from 'vitest'
import adminSource from './admin.css?raw'
import settingsSource from './settings.css?raw'
import { rules, selectors, subject, withoutComments } from '../testing/css'

/**
 * ⑤b empties `admin.css` of the two pre-token idioms the style pass replaced
 * everywhere else: a border mixed from `currentColor`, and `opacity` standing
 * in for a quiet colour. Both are invisible to `colourLiterals.test.ts`,
 * which only fails on a literal — `color-mix(in srgb, currentColor 25%,
 * transparent)` names no colour at all, and `opacity: 0.8` names none either.
 *
 * Asserted here rather than left to review, because "the sweep is complete"
 * is a claim that decays the moment somebody adds a rule.
 */
describe('admin.css holds no pre-token idiom', () => {
  it('uses opacity for no quiet text', () => {
    // A disabled control is the one exemption the spec's §4 allows, and
    // `admin.css` has none today. If one is added, name it here rather than
    // widening the regex.
    const found = rules(adminSource)
      .filter(({ body }) => /(?:^|;)\s*opacity:/.test(body))
      .map(({ prelude }) => prelude)
    expect(found).toEqual([])
  })

  // The companion case: Task 6 replaced the last two `color-mix(in srgb,
  // currentColor …)` borders — `.remove-root` and `.remove-user` — with
  // `--danger`, closing the sweep Task 3 left these two out of.
  it('mixes no border from currentColor', () => {
    const found = [
      ...withoutComments(adminSource).matchAll(
        /color-mix\(in srgb, currentColor[^)]*\)/g,
      ),
    ].map((m) => m[0])
    expect(found).toEqual([])
  })
})

/**
 * I2 of the whole-branch review over ⑤b: nothing asserted that `--danger`
 * actually reaches the two removal blocks. Deleting `box-shadow: inset 3px 0
 * 0 var(--danger)` from `.remove-root, .remove-user`, or either
 * `var(--danger)` declaration from `.remove-root .confirm, .remove-user
 * .confirm`, left the whole suite green — it was the only one of ⑤b's five
 * decisions with no test. Static rather than measured, like the sibling
 * describe block below: this is cheaper than a browser assertion and the
 * token's own contrast is already covered by `index.css.test.ts`.
 */
describe('admin.css marks the removal blocks with --danger', () => {
  it('puts the token on the strip and on the confirm button', () => {
    const strip = rules(adminSource).find(
      ({ prelude }) =>
        prelude === '.remove-root, .remove-user, .remove-library',
    )
    expect(
      strip,
      '.remove-root, .remove-user, .remove-library must exist',
    ).toBeTruthy()
    expect(strip?.body).toMatch(
      /(?:^|;)\s*box-shadow:\s*inset 3px 0 0 var\(--danger\)/,
    )

    const confirm = rules(adminSource).find(
      ({ prelude }) =>
        prelude ===
        '.remove-root .confirm, .remove-user .confirm, .remove-library .confirm',
    )
    expect(
      confirm,
      '.remove-root .confirm, .remove-user .confirm, .remove-library .confirm must exist',
    ).toBeTruthy()
    expect(confirm?.body).toMatch(/(?:^|;)\s*border-color:\s*var\(--danger\)/)
    expect(confirm?.body).toMatch(/;\s*color:\s*var\(--danger\)/)
  })
})

/**
 * A move's box is the removal's without the danger strip: a strip would say
 * this step is graver than it is. The box is the dialog's, drawn by
 * `settings.css`'s `.settings-dialog`, and `admin.css` styles what is in
 * it by `.move-root`, so both sheets are read for both names.
 */
describe('admin.css and settings.css give the move dialog no danger strip', () => {
  it('sets a box-shadow on no rule in either sheet that names .settings-dialog or .move-root', () => {
    const shadowed = [adminSource, settingsSource]
      .flatMap((source) => rules(source))
      .filter(({ body }) => /(?:^|;)\s*box-shadow:/.test(body))
      .flatMap(({ prelude }) => selectors(prelude))
      .filter(
        (selector) =>
          selector.includes('.settings-dialog') ||
          selector.includes('.move-root'),
      )
    expect(shadowed).toEqual([])
  })
})

/**
 * The move's confirm is marked as the action as the removals' are, with the
 * token a move's weight calls for: `--accent`, not `--danger`.
 */
describe('admin.css marks the move confirm as the action', () => {
  it('takes --accent on its border and its text, and a heavier weight', () => {
    const confirm = rules(adminSource).find(
      ({ prelude }) => prelude === '.move-root .confirm',
    )
    expect(confirm, '.move-root .confirm must exist').toBeTruthy()
    expect(confirm?.body).toMatch(/(?:^|;)\s*border-color:\s*var\(--accent\)/)
    expect(confirm?.body).toMatch(/(?:^|;)\s*color:\s*var\(--accent\)/)
    expect(confirm?.body).toMatch(/(?:^|;)\s*font-weight:\s*600/)
    expect(confirm?.body).not.toMatch(/--danger/)
  })
})

/**
 * Task 2's own rule: one `--config-measure` on the page, not a width on each
 * control inside it. Static rather than measured — the browser pass
 * (`Config.browser.test.tsx`) is what proves the measure actually reaches
 * 640px in a real layout; this is what stops a later edit from adding a
 * second, competing width beside it.
 */
describe('admin.css sets the settings-area measure once', () => {
  it('sets the measure once rather than per control', () => {
    const widths = rules(adminSource)
      .filter(({ prelude }) =>
        /\.(add-user|add-root|library-table|new-library|user-table|new-account|library-page|dir-page|add-library|dir-picker|move-root)\b/.test(
          prelude,
        ),
      )
      .filter(({ body }) => /(?:^|;)\s*width:/.test(body))
      .map(({ prelude }) => prelude)
    // A `width` on a control in the settings area is the drift §11.4 refuses:
    // a figure per control is a figure to re-measure per review round.
    expect(widths).toEqual([])
  })
})

/**
 * O153: every `.why` on `/config/users` is one kind of sentence, an
 * explanation beneath what it is about, so one rule quiets them all. They
 * are an account's page's two, inside its Libraries box: that the account
 * sees no library, and the grants the list of libraries does not hold. The
 * rule reaches them as descendants of the screen rather than of a box, so a
 * `.why` drawn anywhere on it is quiet without a rule of its own.
 */
describe('admin.css quiets every .why on the users screen', () => {
  it('reaches .users .why, every one on the screen', () => {
    // Found by its body rather than its prelude, because the prelude is
    // what this case is about.
    const quiet = rules(adminSource).filter(
      ({ body }) =>
        /(?:^|;)\s*color:\s*var\(--muted\)/.test(body) &&
        /(?:^|;)\s*font-size:\s*var\(--text-meta\)/.test(body),
    )
    expect(
      quiet.filter(({ prelude }) => selectors(prelude).includes('.users .why')),
    ).toHaveLength(1)
  })
})

/**
 * O177. `.scan-report > section > dl` makes a grid of the report's two
 * top-level lists and of nothing below them. As `.scan-report dl` it also
 * reached `UnreadableCauses`'s nested `<dl>` of causes and made that a grid
 * too — one hard column at 390px — which `Config.browser.test.tsx`'s "keeps the scan
 * report's list of causes inside its row" measures. That case is in the
 * browser project, which CI does not run; this holds the same rule where CI
 * does, read off the stylesheet rather than measured.
 */
describe("admin.css makes a grid of the scan report's top-level lists only", () => {
  it('gives grid columns to no dl but a section’s own', () => {
    const grids = rules(adminSource)
      .filter(({ body }) => /(?:^|;)\s*grid-template-columns:/.test(body))
      .flatMap(({ prelude }) => selectors(prelude))
      .filter(
        (selector) =>
          selector.startsWith('.scan-report') && subject(selector) === 'dl',
      )
    expect(grids).toEqual(['.scan-report > section > dl'])
  })
})

/**
 * O178 and O180: the rules that keep a path or a name inside its page on the
 * libraries and users screens. `Config.browser.test.tsx` measures each of
 * them at 390px — a report naming a path longer than its row, a removal
 * report, a page's heading, a table row's name, path or list of libraries, a
 * directory's `watch` fact, the removal's confirm button and the sentences the
 * screen says in a paragraph — in the browser project, which CI does not
 * run. This holds that the declarations are there, where CI does; what they
 * do is the browser cases' to say.
 */
describe('admin.css lets a path break and its list drop under its label', () => {
  const declares = (
    selector: string,
    declaration: RegExp,
    source: string = adminSource,
  ) =>
    rules(source).some(
      ({ prelude, body }) =>
        selectors(prelude).includes(selector) && declaration.test(body),
    )

  it.each([
    '.scan-report .paths li',
    '.derived .paths li',
    '.libraries h2',
    '.config-table td.lead',
    '.remove-root .confirm',
    '.remove-library .confirm',
    '.move-volumes li',
    '.libraries p',
    '.users p',
    '.config-table td.wraps',
    '.remove-user .confirm',
    '.dir-browser .where',
    '.dir-browser button[name="up"]',
    '.dir-list button',
  ])('breaks a long path in %s', (selector) => {
    expect(declares(selector, /(?:^|;)\s*overflow-wrap:\s*anywhere/)).toBe(true)
  })

  // The page title, a directory's path; the help under a grant's library,
  // its directories' paths; and the `watch` fact under a directory's path.
  it.each([
    '.page-header .page-title',
    '.settings-row-help',
    '.page-description .regime dd',
  ])('breaks a long path in %s, in settings.css', (selector) => {
    expect(
      declares(selector, /(?:^|;)\s*overflow-wrap:\s*anywhere/, settingsSource),
    ).toBe(true)
  })

  it('wraps a row that holds a list, and no other row', () => {
    const wrapping = rules(adminSource)
      .filter(({ body }) => /(?:^|;)\s*flex-wrap:\s*wrap/.test(body))
      .flatMap(({ prelude }) => selectors(prelude))
      .filter(
        (selector) =>
          selector.startsWith('.scan-report') || selector.includes('.paths'),
      )
    expect(wrapping).toEqual(['.scan-report .paths:not([data-zero])'])
  })

  // A wrapped row shares its spare height between its lines unless told not
  // to, and in the two-column grid a row has spare height whenever the row
  // beside it is taller: the list then starts far below its label.
  it('keeps a dropped list at the top of a row taller than it needs', () => {
    expect(
      declares(
        '.scan-report .paths:not([data-zero])',
        /(?:^|;)\s*align-content:\s*flex-start/,
      ),
    ).toBe(true)
  })
})

// The one `.field` label on the libraries screen is the directory picker's
// under a library's table: New library hands the picker its row's ids, and
// draws no `.field`.
describe('admin.css styles the field labels of the libraries screen', () => {
  it('scopes the field rules to .add-root', () => {
    const preludes = rules(adminSource).flatMap(({ prelude }) =>
      selectors(prelude),
    )
    expect(preludes).toContain('.add-root .field')
    expect(preludes).toContain('.add-root .field input')
  })
})

/**
 * The directory browser while a listing is read. `Config.browser.test.tsx`
 * measures that the list holds still, in the browser project, which CI does
 * not run; this holds the two rules for what is drawn meanwhile.
 */
describe('admin.css draws the directory browser while a listing is read', () => {
  const bodyOf = (selector: string) =>
    rules(adminSource)
      .filter(({ prelude }) => selectors(prelude).includes(selector))
      .map(({ body }) => body)
      .join(';')

  it('hides the browser while it has nothing to draw', () => {
    expect(bodyOf('.dir-browser:empty')).toMatch(/(?:^|;)\s*display:\s*none/)
  })

  it('dims the listing being left once the read is slow', () => {
    expect(bodyOf('.dir-browser:has(> .reading) > :not(.reading)')).toMatch(
      /(?:^|;)\s*color:\s*var\(--muted\)/,
    )
  })
})
