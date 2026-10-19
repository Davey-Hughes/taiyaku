import { describe, expect, it } from 'vitest'
import adminSource from './admin.css?raw'
// Aliased: three cases below bind `subject` as a loop variable, and a
// helper that only some of this file's blocks can see reads worse than one
// name that means the same thing everywhere.
import {
  rules,
  selectors,
  subject as lastCompound,
  withoutComments,
} from '../testing/css'
import { TILE_HEIGHT_PX } from '../tileSize'

/**
 * The proposals inbox read from its stylesheet — spec 2026-09-17 §12.
 *
 * Only facts that are *defects* if they regress live here, the rule
 * `admin-rules.css.test.ts` states for the Rules screen. How the screen looks
 * is the browser pass's job (§5 item 2).
 */

/** The bodies of every rule whose selector list *contains* `selector`. */
function bodiesFor(selector: string): string[] {
  return rules(adminSource)
    .filter(({ prelude }) =>
      prelude.split(',').some((part) => part.trim() === selector),
    )
    .map(({ body }) => body)
}

function declared(body: string, property: string): string | undefined {
  const m = body.match(new RegExp(`(?:^|;)\\s*${property}:\\s*([^;]+)`))
  return m?.[1].trim()
}

/**
 * **The two rows are drawn as two different kinds of control, and that is the
 * whole of O150.** Style pass ② found the failure this pins by shipping it:
 * the underlined tab look reached the inbox and drew "All" and "Hide weak
 * proposals" with the same accent underline side by side, so the look was
 * pulled back off the row (spec §7.2, decided 2026-09-18). ⑤c earns it back by
 * taking the independent toggle out of the row, and this is what stops the two
 * converging again.
 *
 * Asserted on the *declarations*, not on a class name: a test that only
 * checked `.proposal-tabs` exists would pass against a row with no rules at
 * all, which is exactly the state this screen was in before this PR.
 */
describe('the inbox draws a filter and a tab differently', () => {
  it('draws the state row as pills', () => {
    const [body] = bodiesFor('.proposal-states .state')
    expect(body).toBeDefined()
    expect(declared(body, 'border-radius')).toBe('999px')
    expect(declared(body, 'background')).toBe('var(--surface)')
  })

  it('draws the kind row as underlined tabs, sharing the rule with the preview strip', () => {
    const [body] = bodiesFor('.proposal-tabs button')
    expect(body).toBeDefined()
    expect(declared(body, 'background')).toBe('none')
    expect(declared(body, 'border')).toBe('0')

    // **One rule, two preludes** — the same argument as below, one level up
    // and the reason this title says *sharing*. `bodiesFor` is satisfied by a
    // standalone copy of the base rule exactly as well as by the shared one,
    // so the three assertions above cover the kind row's look and nothing at
    // all covers its sharing the preview strip's rule. Splitting the list
    // in two leaves identical bodies and no visible difference on the day;
    // what it costs is that the next edit to one row skips the other.
    const base = rules(adminSource).filter(({ prelude }) =>
      selectors(prelude).includes('.proposal-tabs button'),
    )
    expect(base).toHaveLength(1)
    expect(selectors(base[0].prelude).sort()).toEqual([
      '.preview-tabs button',
      '.proposal-tabs button',
    ])

    const currentBodies = bodiesFor(
      '.proposal-tabs button[aria-pressed="true"]',
    )
    expect(currentBodies).toHaveLength(1)
    expect(declared(currentBodies[0], 'box-shadow')).toBe(
      'inset 0 -2px 0 var(--accent)',
    )

    // **One rule, three preludes** — §12.3. Asserted on the prelude rather
    // than on the body, because three separate rules with identical bodies
    // pass every assertion above and are exactly what this is against: they
    // can drift apart one edit later, and each row marks its
    // chosen one in its own role (§4.1), so there is real pressure to split
    // them.
    // `filter` and a length check, not `find`: `find` returns the *first*
    // match, so a second copy written after the shared rule would leave it
    // pointing at the shared one and passing. The base-rule assertion above
    // is written the same way for the same reason.
    const current = rules(adminSource).filter(({ prelude }) =>
      selectors(prelude).includes('.proposal-tabs button[aria-pressed="true"]'),
    )
    expect(current).toHaveLength(1)
    expect(selectors(current[0].prelude).sort()).toEqual([
      '.config-nav a[aria-current="page"]',
      '.preview-tabs button[aria-selected="true"]',
      '.proposal-tabs button[aria-pressed="true"]',
    ])
  })

  it('draws a verdict as an outline, never as a filled pill', () => {
    // A filled `--accent` pill is what `.proposal-states`'s pressed control
    // wears. This rule was byte-identical to it until ⑤c's browser pass put
    // the two side by side. What must not come back is the fill; the border
    // and the text colour are shared with the pressed pill on purpose, since
    // both mean "this one" — it is the ground that says which is pressable.
    const [body] = bodiesFor('.proposals li.strong .score')
    expect(body).toBeDefined()
    expect(declared(body, 'background')).toBe('none')
    expect(declared(body, 'color')).toBe('var(--accent)')
  })

  it('gives the two rows no shared treatment', () => {
    // The regression ② shipped: one selector matching both rows, drawing
    // them alike. `.chips` was that selector and is gone from this app.
    // Comments are stripped first — this file's head comment tells the
    // story of `.chips` and must go on being allowed to.
    expect(withoutComments(adminSource)).not.toContain('.chips')
  })

  it('gives no selector both rows', () => {
    // The mutation that found this: adding `.proposal-tabs` to the pill
    // rule's selector list leaves every other assertion here green, because
    // both rules still exist and match. What goes wrong is that one element
    // takes both.
    for (const { prelude } of rules(adminSource)) {
      const parts = prelude.split(',').map((p) => p.trim())
      const pill = parts.some((p) => p.startsWith('.proposal-states'))
      const tab = parts.some((p) => p.startsWith('.proposal-tabs'))
      expect(pill && tab).toBe(false)
    }
  })
})

/**
 * **O148.** `TILE_HEIGHT_PX` is the box `.proposal-detail .pages img` draws,
 * restated in TypeScript so `requestedTileEdge` can pick a rung for it. The
 * reader's filmstrip carries the same restatement and has been pinned against
 * its own stylesheet since ③ (`styles/reader.css.test.ts`); the inbox got the
 * restatement without the guard, so changing the CSS left the constant
 * silently wrong and every test green. Setting it to `1` passed the whole
 * suite, because `devicePixelRatio` is 1 in jsdom and every height from 1 to
 * 160 rounds to the same bottom rung there.
 *
 * **The box has two heights and this asserts the right one.** The rule
 * carries `border: 1px solid` with no `box-sizing`, so the *content* box is
 * 72×108 and the *border* box is 74×110 — both measured in Chromium. The
 * constant is the content height, which is correct: that is the box the
 * artwork is scaled into and the number `requestedTileEdge` chooses a rung
 * for. A test that derived from the drawn box would assert 110 and fail
 * against right code; one that ignored the border would pass either way.
 *
 * **One case, not two.** A second `it()` here asserted `object-fit` and
 * `display`, and its comment said those guard the derivation's two inputs —
 * a `width` that stops being a length, or a dropped `aspect-ratio`. Neither
 * property affects the box's dimensions at all, and both hazards are already
 * carried by the assertions below: mutating `width` to `100%` reddens the
 * `toMatch(/^[\d.]+rem$/)` while that second case stayed **green**, which is
 * how it was found (⑤c's Task 4 review). It was decoration with a misleading
 * comment, and deleting it was better than writing a truer comment around
 * assertions that still proved nothing.
 */
describe('the inbox tile height follows the stylesheet', () => {
  it('derives TILE_HEIGHT_PX from the rule that draws the tile', () => {
    const [body] = bodiesFor('.proposal-detail .pages img')
    expect(body).toBeDefined()

    const width = declared(body, 'width')
    expect(width).toMatch(/^[\d.]+rem$/)
    const ratio = declared(body, 'aspect-ratio')
    expect(ratio).toBe('2 / 3')

    // The content box: `width` is a content width, and the 1px border sits
    // outside it. 4.5rem × 16 = 72; 72 × 3 ÷ 2 = 108.
    const contentWidthPx = Number.parseFloat(width!) * 16
    const [w, h] = ratio!.split('/').map((n) => Number(n.trim()))
    expect(TILE_HEIGHT_PX).toBe((contentWidthPx * h) / w)
  })
})

/**
 * **The coarse-pointer floor, asserted as a floor rather than as a rule that
 * exists.** A test that only checked `admin.css` now contains a `(pointer:
 * coarse)` block would pass against a block covering one selector, so this
 * asserts the *lines*: every selector the floor is made of, each of which
 * goes missing under a one-line deletion.
 *
 * **Lines and not the nine kinds of control they cover**, which is what the
 * task brief asked for and what this file cannot have. Naming
 * `.proposal-tabs button` inside the floor is a red test two describes
 * above — `draws the kind row as underlined tabs` pins that selector to
 * exactly one rule, the one the preview strip shares — and naming it beside
 * `.proposal-states .state` is a second red, `gives no selector both rows`,
 * which is O150's guard. Both were watched failing against a floor that
 * named all nine. So the inbox's pills, kind tabs and Dismiss/Reopen are
 * covered by the `button` line here, as `<button>`s inside `.proposals`, and
 * the assertion that covers them is the one on that line's text.
 *
 * **The box line is the one exception to "lines, not controls", and it is
 * deliberate.** It is asserted twice — once for the text it now has and once
 * for the text it must not have — because the defect it closed was not a
 * missing line but a line scoped one container too narrowly, which no
 * `toContain` over the narrow text could have caught.
 *
 * **Every selector below is anchored to the `{` of the rule whose prelude
 * holds it**, and that anchor is what makes a case red when a selector is
 * narrowed until it matches nothing in the app. A bare `toContain`, and
 * `<selector>[^{}]*\{` equally, span a *suffix* appended to the selector as
 * happily as they span the rest of a selector list — so the two link
 * selectors rewritten to `.editor-actions .exports` and
 * `.backup .backup-list a.download`, classes no component writes, left the
 * floor reaching neither link and this file 27 of 27 green. Watched, and
 * then watched one step further: `.nope` appended to all ten remaining
 * subjects as well, a floor reaching nothing at all on any of the six
 * screens, and 26 of the 27 were **still** green — every case but `gives the
 * grant list the same centred 44px row`, the one that already wrote its
 * selector against its own brace. Anchored, that same pair reddens three
 * cases and the whole narrowing reddens nineteen. `inRule` below lets only
 * whitespace — or, for a selector that is not last in its list, a `,` and
 * the rest of the prelude — stand between a selector and its brace, which is
 * every place a selector list can put one and nowhere a suffix can sit.
 *
 * **What these cases still cannot see is effect rather than text**, and that
 * is a known limit rather than an oversight. Each pins a declaration's
 * presence on the rule its selector names, so a *later* rule inside this
 * same block setting `min-height: 0` over the same subjects would take the
 * floor away with every case here green. Holding that wants a computed
 * height, which is the browser pass's to give and not available here for the
 * reason the next paragraph states.
 *
 * The heights themselves belong to the browser pass. jsdom computes none, and
 * this project's `browser` Chromium is `pointer: fine` so the media query
 * never engages there — the blind spot O151 hid in, and the reason §12.4a's
 * figures were taken with a fine pointer, where an absent rule makes the two
 * identical. A Chromium *context* with `hasTouch` and `isMobile` does match
 * `(pointer: coarse)`, and this floor's later figures were read in one; the
 * `browser` project's instance is not configured that way, so none of it is
 * assertable here.
 */
describe('admin.css carries a coarse-pointer floor', () => {
  /**
   * The body of the **last** `@media (pointer: coarse)` block, matched by
   * counting braces rather than by a regex.
   *
   * `admin.css` holds two such blocks once this lands — the shared `.field`
   * block's companion and this floor — so *which* one a test reads is the
   * whole question. A greedy `\{([\s\S]*)\}` anchored to the file's end
   * matches from the **first** block, so it would report a floor that exists
   * whether or not this one was ever written. That version of this helper was
   * caught in review before it shipped; this is the replacement.
   */
  const coarse = (() => {
    const src = withoutComments(adminSource)
    const at = src.lastIndexOf('@media (pointer: coarse)')
    if (at === -1) return ''
    const open = src.indexOf('{', at)
    let depth = 0
    for (let i = open; i < src.length; i++) {
      if (src[i] === '{') depth += 1
      else if (src[i] === '}') {
        depth -= 1
        if (depth === 0) return src.slice(open + 1, i)
      }
    }
    return ''
  })()

  /**
   * A literal selector, escaped for use inside a regular expression — and
   * whitespace-tolerant, because Biome wraps a long prelude across lines and
   * a selector written here with one space has to match a newline and an
   * indent in the file.
   */
  const escaped = (selector: string) =>
    selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replace(/\s+/g, '\\s+')

  /**
   * `selector` anchored to the `{` of the rule whose prelude holds it, and —
   * when one is given — to a `declaration` in that rule's own body.
   *
   * **The anchor carries the whole of this file's coverage of the floor.**
   * `[^{}]*\{` between a selector and its brace, which these cases used
   * before, matches a suffix appended to that selector just as well as it
   * matches the rest of a selector list, so narrowing a selector until it
   * reaches no element satisfied both halves of every case at once. Here
   * only whitespace, or a `,` and the remainder of the prelude, may stand
   * between the two, and neither leaves anywhere for a suffix to sit. The
   * `[^}]*` then cannot leave the rule's own body, so a declaration has to
   * be on the rule the selector names rather than anywhere later in the
   * block.
   *
   * `declaration` is a regular-expression fragment; `selector` is a literal.
   */
  const inRule = (selector: string, declaration = '') =>
    new RegExp(
      `(?:^|[},])\\s*${escaped(selector)}\\s*(?:,[^{}]*)?\\{[^}]*${declaration}`,
    )

  it('has one, and it is the last of exactly two', () => {
    expect(coarse).not.toBe('')
    // Two: the shared `.field` block's companion, and this floor. A third
    // would mean this helper is reading something other than what it thinks,
    // which is the failure mode the brace counter above exists for.
    const blocks = withoutComments(adminSource).match(
      /@media\s*\(pointer:\s*coarse\)/g,
    )
    expect(blocks).toHaveLength(2)
  })

  for (const subject of [
    // Every button on the six screens, which is how the inbox's pills, its
    // kind tabs and its Dismiss/Reopen are reached.
    ':where(.proposals, .account, .config) button:not(:where(.proposal-tabs, .preview-tabs) > button)',
    // Every labelled control on them.
    ':where(.proposals, .account, .config) .field :where(input, select, textarea)',
    // And the one that no label wraps: `/account`'s key-name box, which the
    // line above cannot reach and which would otherwise be the single input
    // on that screen left at its desktop 35.6.
    '.account .account-row input',
    '.account-wide',
    // The two link-shaped actions, which the `button` line cannot reach and
    // which the speculative `a.button` this list used to name reached
    // neither of: `/config/rules`'s Export, beside an Import button already
    // at 44, and the download on `/config/backup`, that screen's only
    // per-row action.
    '.editor-actions .export',
    '.backup .backup-list a',
    // The `<details>` toggles, which no other rule styles.
    ':where(.proposals, .account, .config) summary',
    // The checkbox rows. Named at (0, 2, 1) rather than scoped with
    // `:where()`, because they carry `align-items` as well and the rule they
    // have to beat for that is (0, 2, 1). **All three**: before this list
    // named them, deleting the root row's and the user row's took the 44px
    // row and the centring off `/config/users`'s three permission checkboxes
    // and the root row's one — the controls §12.4a measured at 13×13 and the
    // reason this block exists — with every case here still green.
    '.root-row .checks label',
    '.user-row .checks label',
    '.proposals .checks label',
    // The account page's per-library grant rows, which none of the
    // `.checks` lines can reach: `AccountPage` draws one `<li><label>` per
    // library inside a `.grant-list`, and none of the `.checks` lines names
    // that label.
    '.user-row .grant-list label',
    // The two radios above them, a box and its words as well.
    '.user-row .grants > label',
    // The box itself, scoped to the three roots rather than to `.checks`.
    // `.checks` holds five of these boxes and never held all of them — see
    // `scopes the box to the three roots` below, which is the half of this
    // that a second, narrower copy cannot satisfy.
    ':where(.proposals, .account, .config) input[type="checkbox"]',
  ]) {
    it(`names ${subject}`, () => {
      expect(coarse).toMatch(inRule(subject))
    })
  }

  it('gives every subject of the first rule the 44px row itself', () => {
    // The same shape as the two Criticals below, one rule up: `names` above
    // is satisfied by any of these four sitting in a rule that declares
    // nothing. They share one rule today, and `inRule` reaches that rule's
    // own `{` wherever in the list a selector sits — and reaches a
    // *different* rule's if one of them is ever split out, which is how a
    // split that drops the declaration is caught.
    for (const subject of [
      ':where(.proposals, .account, .config) button:not(:where(.proposal-tabs, .preview-tabs) > button)',
      ':where(.proposals, .account, .config) .field :where(input, select, textarea)',
      '.account .account-row input',
      '.account-wide',
    ]) {
      expect(coarse).toMatch(inRule(subject, 'min-height:\\s*44px'))
    }
  })

  /**
   * O157: `RuleList`'s enable box has no row to take the 44, because it has
   * no visible label, so it gets a wordless `<label>` of its own and this
   * rule sizes the label. The box inside keeps the 24px the checkbox rule
   * gives it. 44px on the box itself would draw the button-shaped checkbox
   * this block refuses. `display: inline-grid` is asserted with the rest
   * because it is what centres the box across: in the flex row the label
   * is blockified and takes its 44×44 either way, and without the grid the
   * box sits at the label's left edge.
   */
  it('gives the rule-enable box a 44px label of its own', () => {
    for (const declaration of [
      'display:\\s*inline-grid',
      'min-inline-size:\\s*44px',
      'min-block-size:\\s*44px',
      'place-items:\\s*center',
    ]) {
      expect(coarse).toMatch(inRule('.rule-row .enable', declaration))
    }
  })

  /**
   * O157: the three underlined one-of-N rows take their 44px from a box
   * with no paint centred on each item, not from their own height, which
   * the floor's `:not()` keeps off them on §12.8's measurement. Both halves
   * are asserted. The `::after` grows the target, and `position: relative`
   * anchors it to its own item. Without that, every tab's target would be
   * placed against the nearest positioned ancestor, one box for the whole
   * row.
   */
  for (const item of [
    '.config-nav a',
    '.proposal-tabs > button',
    '.preview-tabs > button',
  ]) {
    it(`grows ${item} to 44px to the touch without drawing it taller`, () => {
      expect(coarse).toMatch(inRule(item, 'position:\\s*relative'))
      // Every declaration the box needs to land centred on its item: the
      // two insets and the translate together are the centring, and a box
      // missing either inset sits at its static position shifted up and
      // left by half its own size.
      for (const declaration of [
        'content:\\s*""',
        'position:\\s*absolute',
        'inset-block-start:\\s*50%',
        'inset-inline-start:\\s*50%',
        'inline-size:\\s*max\\(100%,\\s*44px\\)',
        'block-size:\\s*max\\(100%,\\s*44px\\)',
        'translate:\\s*-50%\\s+-50%',
      ]) {
        expect(coarse).toMatch(inRule(`${item}::after`, declaration))
      }
      // The item's own box is left alone. A height here is the redesign
      // §12.8 measured and refused.
      expect(coarse).not.toMatch(inRule(item, 'min-height'))
      expect(coarse).not.toMatch(inRule(item, 'min-block-size'))
    })
  }

  /**
   * **The 44 has to be a floor on the rendered box, and `min-height` alone
   * is not.** In standards mode the UA stylesheet gives `<select>` and
   * `<button>` `border-box` and leaves `<input>` and `<textarea>` at
   * `content-box`, and this project sets no global reset — so before
   * `box-sizing` joined the rule, the same line rendered the selects on
   * these screens at 44 or more and every text box beside them at **54**:
   * 44 of content, plus the shared reset's `var(--space-1)` padding top and
   * bottom and its 1px borders. Measured at 390px with a coarse pointer on
   * 2026-09-25, on `/account`, the add-account and add-root forms, the
   * expanded root and user rows, the rule form and the proposals accept
   * form alike.
   *
   * `border-box` and not merely "declares `box-sizing`": `content-box` is
   * the value the defect had, spelled out.
   */
  it('measures that 44 on the rendered box rather than on its content', () => {
    for (const subject of [
      ':where(.proposals, .account, .config) button:not(:where(.proposal-tabs, .preview-tabs) > button)',
      ':where(.proposals, .account, .config) .field :where(input, select, textarea)',
      '.account .account-row input',
      '.account-wide',
    ]) {
      expect(coarse).toMatch(inRule(subject, 'box-sizing:\\s*border-box'))
    }
  })

  /**
   * **And the same of any rule this floor grows later**, which the case
   * above cannot say: its four selectors are a hand-kept list, so a fifth
   * text box floored in a rule of its own would leave it green at 54. The
   * subject is what decides — a `<label>`, a `<summary>` or a checkbox row
   * carries no padding from the shared reset and is not the shape this is
   * about — so the filter reads each selector's last compound rather than
   * its scope.
   */
  it('holds for every rule in the block that floors a text box', () => {
    const floored = rules(coarse).filter(({ body }) =>
      /min-height:\s*44px/.test(body),
    )
    const textBoxes = floored.filter(({ prelude }) =>
      selectors(prelude).some((s) =>
        /\b(?:input|textarea)\b/.test(lastCompound(s)),
      ),
    )
    // **Not vacuous, and by membership rather than by a count.** A `coarse`
    // that read the wrong block, or a `lastCompound` that stopped matching,
    // would leave this empty and the loop below would pass over nothing. A
    // count would say the same and one thing more: it would redden for a
    // rule split in two for some reason of its own, sending the next reader
    // to bump a number rather than to the loop that carries the meaning.
    expect(textBoxes.flatMap(({ prelude }) => selectors(prelude))).toEqual(
      expect.arrayContaining([
        ':where(.proposals, .account, .config) .field :where(input, select, textarea)',
        '.account .account-row input',
      ]),
    )
    for (const rule of textBoxes) {
      expect(rule.body, rule.prelude).toMatch(/box-sizing:\s*border-box/)
    }
  })

  it('is scoped to the three admin roots, not to .library', () => {
    // The defect is a second floor over the library home, search, the series
    // page and the sidebar, which `library.css` and `shell.css` already
    // answer for — whether it arrives as `:where(.library) button`, as a
    // bare `.library button`, or by widening a selector already here. The
    // first version of this asserted `/:where\(\.library[,)]/`, which is one
    // spelling of it: `.library button { min-height: 44px }` added inside
    // the block left it green, watched. **Any** mention of `.library` in
    // this block is the defect, and none of the three roots contains the
    // substring, so the whole class is one match. The positive half — that
    // the scoped text is still there — is the `names` cases above, not
    // repeated here.
    expect(coarse).not.toMatch(/\.library/)
  })

  it('gives all three checkbox rows a centred 44px row', () => {
    // `.checks label` is `align-items: baseline` above, which puts a
    // checkbox's bottom edge on the sentence's baseline: right at the 13px
    // box the browser draws, and at 24px it lifts the box 6.5px above the
    // sentence's centre. Measured in Chromium at 390px on 2026-09-24 — 3px
    // above the row's top and 17px below, against 10 and 10 once centred.
    //
    // **Per row and per declaration**, because `names` above is satisfied by
    // the selector sitting in a rule with nothing in it. The `{` each of
    // these reaches is the one belonging to the rule whose prelude holds
    // that selector, wherever in the list it sits.
    for (const row of ['.root-row', '.user-row', '.proposals']) {
      const label = `${row} .checks label`
      expect(coarse).toMatch(inRule(label, 'min-height:\\s*44px'))
      expect(coarse).toMatch(inRule(label, 'align-items:\\s*center'))
    }
  })

  it('gives the two link-shaped actions a centred 44px row', () => {
    // §7.1 is about links *and* controls and the first draft of this floor
    // covered only the controls: measured at 390px, Export was 47×24 beside
    // an Import button at 72×44, and the backup download 196×24 as the only
    // per-row action on that screen.
    //
    // All three declarations, because the first two are what make the third
    // do anything to a link that is not a flex item — and because
    // `min-height` alone, which these two do take today, left the word at
    // the top of the 44px box (0 above, 20 below) rather than centred.
    for (const link of ['.editor-actions .export', '.backup .backup-list a']) {
      expect(coarse).toMatch(inRule(link, 'display:\\s*inline-flex'))
      expect(coarse).toMatch(inRule(link, 'align-items:\\s*center'))
      expect(coarse).toMatch(inRule(link, 'min-height:\\s*44px'))
    }
  })

  it('gives the disclosure toggles a 44px row without taking their marker', () => {
    // The `<details>` toggles — `RuleEditor`'s save-derive, the
    // removal-derives of `RemoveRoot`, `RemoveLibrary` and `MoveRoot`, and
    // `MoveRoot`'s move-volumes — which no other rule styles.
    const summary = ':where(.proposals, .account, .config) summary'
    expect(coarse).toMatch(inRule(summary, 'min-height:\\s*44px'))
    // **`align-content`, not `align-items`, and that is the whole finding.**
    // A `<summary>` is `display: list-item`, so it takes `min-height` as it
    // stands but leaves its text at the top of the box. `display: flex` with
    // `align-items: center` does centre it and deletes the disclosure
    // triangle: measured in Chromium at 390px, the content's inline-start
    // offset falls from 16.94px to 0. This negative is what stops that
    // being the fix, and it is brace-scoped so the `display: flex` on the
    // grant-list rule two rules down cannot satisfy it.
    expect(coarse).toMatch(inRule(summary, 'align-content:\\s*center'))
    expect(coarse).not.toMatch(/summary[^{}]*\{[^}]*display:\s*(inline-)?flex/)
  })

  it('scopes the box to the three roots, not to .checks', () => {
    // The `names` case above is satisfied by a `.checks`-scoped copy sitting
    // beside the widened rule just as well as by the widened rule alone, and
    // a copy is how this regressed the first time: the rule reached five of
    // the boxes while its comment claimed every checkbox. `.grant-list`'s
    // box and `.rule-list`'s bare one were the two it missed.
    expect(coarse).not.toMatch(/\.checks\s+input\[type="checkbox"\]/)
  })

  it('gives the two radios of the grants the same centred 44px row', () => {
    const radios = '.user-row .grants > label'
    expect(coarse).toMatch(inRule(radios, 'display:\\s*flex'))
    expect(coarse).toMatch(inRule(radios, 'align-items:\\s*center'))
    expect(coarse).toMatch(inRule(radios, 'min-height:\\s*44px'))
  })

  it('gives the grant list the same centred 44px row', () => {
    // `display: flex` is not decoration here and is the reason this is three
    // assertions rather than one. `.checks label` is already flex from its
    // own rule above, so its floor adds only the height and the centring; a
    // `.grant-list` label is flex through its own base rule, which is why the
    // floor can name `min-height` at all: it does nothing to an inline box.
    const grant = '.user-row .grant-list label'
    expect(coarse).toMatch(inRule(grant, 'display:\\s*flex'))
    expect(coarse).toMatch(inRule(grant, 'align-items:\\s*center'))
    expect(coarse).toMatch(inRule(grant, 'min-height:\\s*44px'))
  })

  /**
   * The box rule's own three declarations.
   *
   * **All three were `[\s\S]*?` across the whole block body before**, which
   * is not an assertion about the checkbox rule at all: it matches the
   * selector, then anything, then the declaration *anywhere after it*.
   * Watched — with the checkbox rule's declarations replaced by
   * `outline-offset: 2px` and the three moved onto an unrelated later rule
   * in the same block, so that no checkbox anywhere got a box, all three of
   * the old expressions still matched and the file was green. Through
   * `inRule` the `[^}]*` cannot leave the rule's own body, so the
   * declaration has to be on the rule the selector names.
   *
   * The positives carry the full scoped selector, because a second, narrower
   * copy of the rule satisfies a bare `input[type="checkbox"]`. The negative
   * deliberately does not, and is deliberately not `inRule` either: *any*
   * checkbox rule in this block carrying a 44px row is the defect it is for,
   * whatever it is scoped to, so it wants the loose match the positives must
   * not have.
   */
  const boxRule = ':where(.proposals, .account, .config) input[type="checkbox"]'

  it('gives the radios the ticks’ 24px box, in the one rule', () => {
    // A radio beside a tick drawn at 13px against 24 reads as a different
    // kind of row, and the grants hold both. They share the selector list, so
    // a copy of the rule for one cannot satisfy this.
    const radio = ':where(.proposals, .account, .config) input[type="radio"]'
    expect(coarse).toMatch(
      new RegExp(
        `${escaped(boxRule)}\\s*,\\s*${escaped(radio)}\\s*\\{[^}]*inline-size:\\s*24px`,
      ),
    )
  })

  it('keeps the 24px box from shrinking in the row it sits in', () => {
    // A flex item's base size is its max-content width, so a root path on
    // one line puts the grant row's free space well under zero and the
    // default `flex-shrink: 1` takes part of it out of the box: measured
    // 20.27×24 in Chromium at 390px with this declaration removed, against a
    // rule that says 24.
    expect(coarse).toMatch(inRule(boxRule, 'flex-shrink:\\s*0'))
  })

  it('centres the pill text optically against the floor it sits in', () => {
    // Reported from a phone. A button centres its content box, so the floor's
    // slack is split evenly — but the text inside it is not symmetric about
    // its own centre, and all three pills sat with their cap-top 1.5px below
    // the pill's optical centre. `padding-block: 2.5px 5.5px` keeps the total
    // padding at 8px and moves the content up 1.5px, which takes that offset
    // to 0.0 on all three; `Dismissed`, the one word with no descender, lands
    // 16.5/16.5. Measured in Chromium at 390x844 with `hasTouch` against the
    // built stylesheet — `admin.css`'s own comment carries the table, and why
    // the first version of this rule (`0 6px`) overcorrected by reading a
    // descender as skew.
    //
    // Pinned as the asymmetry, not as the literal: what must not come back is
    // a symmetric `padding-block`, which is what the floor inherits without
    // this rule. Both halves — that the rule exists inside the coarse block,
    // and that its two values differ — are needed, because `padding-block: 4px`
    // satisfies a presence check and restores the defect.
    const rule = inRule('.proposal-states .state', 'padding-block')
    expect(coarse).toMatch(rule)
    const m = coarse.match(
      /\.proposal-states \.state\s*(?:,[^{}]*)?\{[^}]*padding-block:\s*([^;}]+)/,
    )
    expect(m).not.toBeNull()
    const [top, bottom] = m![1].trim().split(/\s+/)
    expect(bottom).toBeDefined()
    expect(bottom).not.toBe(top)
  })

  it('gives a checkbox a box rather than a row', () => {
    // A checkbox is not a row: 44px beside a 14px sentence draws something
    // that looks like a button. 24px is the size `library.css` gives the two
    // small controls on a tile, and the tappable target is the `<label>`,
    // which takes the 44.
    expect(coarse).toMatch(inRule(boxRule, 'inline-size:\\s*24px'))
    expect(coarse).toMatch(inRule(boxRule, 'block-size:\\s*24px'))
    // Order-independent now: the old unscoped form reddened on any rule
    // *after* the checkbox rule that carried a 44px row, so moving the
    // `summary` rule from above it to below broke this for a non-reason.
    // Watched both ways — green here, and the old expression matched.
    expect(coarse).not.toMatch(
      /input\[type="checkbox"\][^{}]*\{[^}]*min-height:\s*44px/,
    )
  })
})

/**
 * **O157's label, outside the coarse block.** `RuleList` wraps the enable box
 * in a wordless `<label className="enable">`, and the coarse block's rule is
 * the only one that styled it — so at a fine pointer the label drew the box
 * in a line box, 3px lower than it sat with no label (measured at 1440px,
 * `docs/notes.md` "Admin follow-ups"). A flex box outside every at-rule puts
 * it back, and the coarse rule, coming later at the same specificity, still
 * makes it the 44px grid there.
 *
 * **Outside every at-rule, asserted, and not only first.** `rules()` reads
 * the rules nested in an `@media` block as readily as the top-level ones, so
 * "the first of two declares `flex`" alone passes against a `flex` rule moved
 * into a block that a desk never matches, which is the defect back.
 */
describe('the rule-enable label leaves the desktop box where it was', () => {
  /** `css` with every at-rule block taken out, braces counted. */
  const unconditional = (css: string) => {
    let out = ''
    let i = 0
    while (i < css.length) {
      const at = css.indexOf('@', i)
      if (at === -1) return out + css.slice(i)
      const open = css.indexOf('{', at)
      const semi = css.indexOf(';', at)
      if (open === -1 || (semi !== -1 && semi < open)) {
        out += css.slice(i, semi === -1 ? css.length : semi + 1)
        i = semi === -1 ? css.length : semi + 1
        continue
      }
      out += css.slice(i, at)
      let depth = 0
      let j = open
      for (; j < css.length; j++) {
        if (css[j] === '{') depth += 1
        else if (css[j] === '}') {
          depth -= 1
          if (depth === 0) break
        }
      }
      i = j + 1
    }
    return out
  }

  const enable = rules(adminSource).filter(
    ({ prelude }) => prelude === '.rule-row .enable',
  )

  it('is styled by exactly two rules', () => {
    expect(enable).toHaveLength(2)
  })

  it('is a flex box in the first, and that one applies at every pointer', () => {
    expect(declared(enable[0].body, 'display')).toBe('flex')
    const everywhere = rules(
      unconditional(withoutComments(adminSource)),
    ).filter(({ prelude }) => prelude === '.rule-row .enable')
    expect(everywhere).toHaveLength(1)
    expect(declared(everywhere[0].body, 'display')).toBe('flex')
  })

  it('is the coarse block’s inline grid in the last', () => {
    expect(declared(enable[enable.length - 1].body, 'display')).toBe(
      'inline-grid',
    )
  })
})
