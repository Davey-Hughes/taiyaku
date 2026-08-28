import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { AcceptForm } from './AcceptForm'
import * as curation from './curation'
import { mount, rerender } from './testing/mount'

// `ProposalRow.test.tsx`'s `vi.mock` shape, for the reason its own comment
// gives: `screen`/`userEvent` are not dependencies of this repository, so the
// assertions below drive real DOM elements through `querySelector`.
//
// `consequence` and `refusals` are deliberately *not* mocked. They are pure,
// `curation.test.ts` pins each of them line by line, and stubbing them here
// would leave this file unable to tell a form that calls them from one that
// prints its own sentence.
vi.mock('./curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./curation')>()),
  resolution: vi.fn(),
  acceptProposal: vi.fn(),
}))

/**
 * The rule `offered_rule` derives for the library
 * `proposals/tests.rs`'s `library_with_a_shared_publisher_directory` builds —
 * a root laid out `Publisher/<series>/<series> - v01/`, holding
 * `Go-Toubun no Hanayome`, its `Full Color Edition`, and one unrelated third
 * series. Printed from that fixture rather than invented: the pattern is
 * `parent_of` the subject's entries plus a slash, `full-color-edition` is the
 * slugified suffix, `official` is `implied_edition_kind`'s answer for
 * `Full Color Edition`, and `set_language` is never derived at all.
 *
 * **Every field differs in shape from every other**, which is what lets the
 * first test below tell five controls reading five fields apart from five
 * controls reading one: a path, a match kind, a slug, an edition kind and an
 * empty string.
 */
const rule = (over: Partial<curation.Rule> = {}): curation.Rule => ({
  root_id: 1,
  kind: 'prefix',
  pattern: 'Publisher/Go-Toubun no Hanayome - Full Color Edition/',
  match_on: 'path',
  set_series: 'Go-Toubun no Hanayome',
  set_edition_key: 'full-color-edition',
  set_kind: 'official',
  set_language: null,
  note: 'accepted from proposal 1: "Go-Toubun no Hanayome - Full Color Edition" is "Go-Toubun no Hanayome"',
  ...over,
})

/** The series being accepted *into* — the one the sentence has to name. */
const target = (over: Partial<curation.Side> = {}): curation.Side => ({
  series_id: 2,
  title: 'Go-Toubun no Hanayome',
  directories: ['/srv/manga/Publisher/Go-Toubun no Hanayome/'],
  volume_count: 14,
  volumes: [1, 2, 3],
  root_name: 'Publisher',
  first_volume: null,
  ...over,
})

/**
 * A resolution with all five refusals clear, overridden one at a time.
 *
 * **The two counts differ, and that is the point of the fixture.**
 * `consequence` reads `works_moved` and `refusals` reads `entries_matched`;
 * a fixture where the two agreed would pass whichever of them the form
 * happened to hand over. Fourteen volumes move, and a fifteenth entry is
 * matched without moving.
 *
 * Overriding one field at a time is `curation.test.ts`'s own shape for the
 * same reason it gives: a fixture that tripped several refusals at once could
 * not tell a form checking all five from one checking whichever came first.
 */
const clear = (
  over: Partial<curation.Resolution> = {},
): curation.Resolution => ({
  also_resolves: [],
  collateral: [],
  targets_reached: [],
  entries_matched: 15,
  subject_reached: true,
  works_moved: 14,
  ...over,
})

/**
 * Past `AcceptForm`'s debounce window, which every test but one below only
 * needs to be *past*. The window's actual length is pinned by `collapses a
 * burst of typing`, which walks the clock across it from both sides.
 */
const PAST_THE_WINDOW = 300

function control<T extends Element>(host: HTMLElement, selector: string): T {
  const el = host.querySelector<T>(selector)
  if (!el) throw new Error(`the form has no ${selector}`)
  return el
}

const pattern = (host: HTMLElement) =>
  control<HTMLInputElement>(host, '.field.pattern input')
const matchKind = (host: HTMLElement) =>
  control<HTMLSelectElement>(host, '.field.match-kind select')
const editionKey = (host: HTMLElement) =>
  control<HTMLInputElement>(host, '.field.edition-key input')
const editionKind = (host: HTMLElement) =>
  control<HTMLSelectElement>(host, '.field.edition-kind select')
const language = (host: HTMLElement) =>
  control<HTMLInputElement>(host, '.field.language input')
const accept = (host: HTMLElement) =>
  control<HTMLButtonElement>(host, 'button.accept')

const refusalLines = (host: HTMLElement) =>
  [...host.querySelectorAll('.refusals li')].map((li) => li.textContent)

/**
 * Set a controlled field's value the way a keystroke does.
 *
 * Through the prototype's own setter, not the element's: React installs a
 * value tracker on the instance and skips an event whose value it believes it
 * already has, so assigning `el.value` directly fires nothing. `Search.test.tsx`
 * carries the same helper for the same reason.
 */
async function typeInto(el: HTMLInputElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set?.call(el, value)
    el.dispatchEvent(new Event('input', { bubbles: true }))
    await Promise.resolve()
  })
}

async function choose(el: HTMLSelectElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      HTMLSelectElement.prototype,
      'value',
    )?.set?.call(el, value)
    el.dispatchEvent(new Event('change', { bubbles: true }))
    await Promise.resolve()
  })
}

/**
 * Move the clock without sending anything through — the gap between two
 * keystrokes, or a slice of the window being walked across.
 */
async function tick(ms: number) {
  await act(async () => {
    vi.advanceTimersByTime(ms)
    await Promise.resolve()
  })
}

/** Let the debounce fire and the sweep it sends settle. */
async function settle() {
  await act(async () => {
    vi.advanceTimersByTime(PAST_THE_WINDOW)
    await Promise.resolve()
    await Promise.resolve()
  })
}

const click = async (el: Element) => {
  await act(async () => {
    ;(el as HTMLElement).click()
    await Promise.resolve()
    await Promise.resolve()
  })
}

interface Mounted {
  host: HTMLElement
  /** One entry per `onAccepted` call. */
  accepted: number[]
}

/**
 * Mount the form under the panel class its styles are scoped by, so a
 * selector this file relies on is the one `index.css` writes rules for.
 */
async function open(
  over: Partial<curation.Rule> = {},
  id = 1,
): Promise<Mounted> {
  const accepted: number[] = []
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <div className="proposal-detail">
        <AcceptForm
          id={id}
          rule={rule(over)}
          target={target()}
          onAccepted={() => accepted.push(1)}
        />
      </div>,
    )
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return { host, accepted }
}

/** Mount and let the first sweep land. */
async function ready(
  over: Partial<curation.Rule> = {},
  id = 1,
): Promise<Mounted> {
  const mounted = await open(over, id)
  await settle()
  return mounted
}

describe('AcceptForm', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.clearAllMocks()
    vi.mocked(curation.resolution).mockResolvedValue(clear())
    vi.mocked(curation.acceptProposal).mockResolvedValue({ rule_id: 91 })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('shows the rule it is about to write, defaulted narrow', async () => {
    const { host } = await ready()

    expect(pattern(host).value).toBe(
      'Publisher/Go-Toubun no Hanayome - Full Color Edition/',
    )
    expect(matchKind(host).value).toBe('prefix')
    expect(editionKey(host).value).toBe('full-color-edition')
    expect(editionKind(host).value).toBe('official')
    // Never derived — `implied_edition_kind`'s own comment says so — so the
    // form offers it empty rather than guessing one.
    expect(language(host).value).toBe('')

    // The words beside each control, and that each is a label of exactly one
    // of them: a field nobody can name is one nobody edits, and five boxes
    // in a row are indistinguishable without them.
    expect(
      [...host.querySelectorAll('.field > span')].map((s) => s.textContent),
    ).toEqual([
      'Pattern',
      'Match as',
      'Edition key',
      'Edition kind',
      'Language',
    ])
    for (const el of [
      pattern(host),
      matchKind(host),
      editionKey(host),
      editionKind(host),
      language(host),
    ]) {
      expect(el.labels).toHaveLength(1)
    }
  })

  it('says the pattern is confined to one root, under the box that is widened', async () => {
    // `common_directory_pattern` scopes the offered rule to the root its
    // subject sits under and nothing in this form unscopes it — so the widest
    // pattern anybody can type still stops at that root, and nothing in the
    // pattern itself shows it. The panel prints `root` for each side well
    // above the form, which says where the material is, not what the rule is
    // limited to.
    const { host } = await ready()

    expect(control(host, '.scope').textContent).toMatch(/one root only/i)
    // Under the pattern rather than after the four short fields: it is a
    // fact about that box, and `.field.pattern` is the row above it.
    expect(
      [...control(host, '.rule-fields').children].map((el) => el.className),
    ).toEqual([
      'field pattern',
      'scope',
      'field match-kind',
      'field edition-key',
      'field edition-kind',
      'field language',
    ])
  })

  it('says the opposite where the rule is tied to no root at all', async () => {
    // `Rule.root_id` is nullable and `accept_with` takes a NULL one today, so
    // a rule written by hand can arrive unscoped. The line above would be a
    // false promise about it, and this is the one field on the form whose
    // value is never editable here — the sentence is the only thing that can
    // change.
    const { host } = await ready({ root_id: null })

    expect(control(host, '.scope').textContent).toMatch(/every one/i)
    expect(control(host, '.scope').textContent).not.toMatch(/one root only/i)
  })

  it('shows a language and a fan kind the rule already carries', async () => {
    // The other side of the two fields the test above sees at their
    // defaults. `fan` is `implied_edition_kind`'s second answer, and a
    // language only ever arrives from a rule somebody wrote by hand — both
    // have to survive the trip through the controls rather than being reset
    // to the value the common case happens to hold.
    const { host } = await ready({ set_kind: 'fan', set_language: 'en' })

    expect(editionKind(host).value).toBe('fan')
    expect(language(host).value).toBe('en')
  })

  it('offers an empty key and kind for a rule that sets neither', async () => {
    // What `offered_rule` writes for a `same-series` proposal over these same
    // two series: that claim is about identity, not about a second edition,
    // so it derives no edition key and no kind. Both controls have to show
    // that as empty rather than as the word "null", and `consequence` has its
    // own `(none)` for the sentence.
    const { host } = await ready({ set_edition_key: null, set_kind: null })

    expect(editionKey(host).value).toBe('')
    expect(editionKind(host).value).toBe('')
    expect(control(host, '.consequence').textContent).toContain(
      'add edition (none)',
    )
  })

  it('keeps a value the edition-kind list does not hold, rather than rewriting it', async () => {
    // A `<select>` whose `value` matches no `<option>` shows the first option
    // instead, and the next edit to any other field would then submit that
    // substitute — the rule quietly rewritten by the control drawn to show
    // it. The offered value has to stay selectable and selected.
    const { host } = await ready({ set_kind: 'sepia' })

    expect(editionKind(host).value).toBe('sepia')
    await typeInto(language(host), 'ja')
    await settle()

    expect(vi.mocked(curation.resolution).mock.calls.at(-1)?.[1].set_kind).toBe(
      'sepia',
    )
  })

  it('asks what the offered rule would do, once, without being asked to', async () => {
    // The panel opens on a rule nobody has touched, and the sentence over
    // the button is the whole reason the rule is on screen — so the first
    // sweep is not waiting for an edit.
    await ready({}, 7)

    expect(curation.resolution).toHaveBeenCalledTimes(1)
    expect(curation.resolution).toHaveBeenCalledWith(7, rule())
  })

  it('collapses a burst of typing into the one sweep still standing', async () => {
    // `/resolution` compiles every rule, reads every root and resolves every
    // `scan_entry` row. Three keystrokes are three sweeps of the library
    // without this, and the first two are answers nobody will read.
    const { host } = await ready()
    expect(curation.resolution).toHaveBeenCalledTimes(1)

    // **A hundred milliseconds between them, which is the half of this the
    // window's length actually depends on.** Three edits dispatched with no
    // time advanced between them are one task, and a window of 0ms collapses
    // those exactly as well as one of 250ms — while in a browser each
    // keystroke is its own task and a 0ms window collapses nothing at all. So
    // the clock moves at a fast typist's rhythm here, and a debounce shorter
    // than that rhythm fails this line rather than passing it.
    for (const typed of ['Publisher/Go', 'Publisher/Go-', 'Publisher/']) {
      await typeInto(pattern(host), typed)
      await tick(100)
    }
    expect(curation.resolution).toHaveBeenCalledTimes(1)

    // And the window itself, walked across from both sides: 240ms of quiet
    // after the last keystroke is not enough, 260ms is. Together with the
    // rhythm above that pins the constant rather than bounding it — the
    // `PAST_THE_WINDOW` all the other tests use only ever says "no longer
    // than 300".
    await tick(140)
    expect(curation.resolution).toHaveBeenCalledTimes(1)
    await tick(20)
    expect(curation.resolution).toHaveBeenCalledTimes(2)
    expect(curation.resolution).toHaveBeenLastCalledWith(
      1,
      rule({ pattern: 'Publisher/' }),
    )
  })

  it('sweeps for the rule as edited, and sends back untouched every field it does not show', async () => {
    // C3 §7 is "the client shows the rule, the server writes what it was
    // shown". `root_id`, `match_on`, `set_series` and `note` have no control
    // here, and a form that rebuilt the rule from its five boxes would drop
    // all four — the accepted material would land in no named series, under
    // every root, with nothing recording which proposal wrote it.
    const { host } = await ready()

    await typeInto(
      pattern(host),
      '^Publisher/(?<series>.+) - Full Color Edition/',
    )
    await choose(matchKind(host), 'regex')
    await typeInto(editionKey(host), 'colour')
    await choose(editionKind(host), 'fan')
    await typeInto(language(host), 'ja')
    await settle()

    expect(curation.resolution).toHaveBeenLastCalledWith(1, {
      root_id: 1,
      kind: 'regex',
      pattern: '^Publisher/(?<series>.+) - Full Color Edition/',
      match_on: 'path',
      set_series: 'Go-Toubun no Hanayome',
      set_edition_key: 'colour',
      set_kind: 'fan',
      set_language: 'ja',
      note: rule().note,
    })
  })

  it('sends null rather than an empty string for a field emptied out', async () => {
    // `set_*` are nullable columns and `''` is not their null: an edition key
    // of `""` is `TEXT NOT NULL` taking an empty string happily, which is the
    // very state `offered_rule` refuses a slugless suffix to avoid.
    const { host } = await ready()

    await typeInto(editionKey(host), '')
    await choose(editionKind(host), '')
    await settle()

    expect(vi.mocked(curation.resolution).mock.calls.at(-1)?.[1]).toMatchObject(
      {
        set_edition_key: null,
        set_kind: null,
        set_language: null,
      },
    )
  })

  it('says how many more a widened pattern resolves', async () => {
    // The reason the pattern is editable at all: C3 §3 chose a list over a
    // triage loop so one widened rule can answer the questions 37 rows are
    // asking, and "resolves 6 more" is that turned into a preview rather
    // than a surprise.
    vi.mocked(curation.resolution).mockResolvedValue(
      clear({
        also_resolves: [2, 3, 4, 5, 6, 7],
        entries_matched: 60,
        works_moved: 54,
      }),
    )
    const { host } = await ready()

    await typeInto(
      pattern(host),
      '^Publisher/(?<series>.+) - Full Color Edition/',
    )
    await choose(matchKind(host), 'regex')
    await settle()

    const line = control(host, '.consequence').textContent
    expect(line).toMatch(/resolves 6 more/i)
    // Which count the sentence reads, not merely that it holds a number:
    // 54 works move and 60 entries matched, and only one of them is what a
    // person is being asked to approve.
    expect(line).toContain('54 works')
    expect(line).not.toContain('60')
    expect(line).toContain('to series Go-Toubun no Hanayome')
    expect(accept(host).disabled).toBe(false)
    expect(host.querySelector('.refusals')).toBeNull()
  })

  it('describes the rule as edited, not the one it was offered', async () => {
    // The sentence over the button is the thing being approved, so it has to
    // be about the rule the button will send. Every field `consequence`
    // reads is moved off its offered value here — the key away from the
    // slug, the kind to its other value, the language from unset to set,
    // which is the branch that prints nothing at all when it is null.
    const { host } = await ready()

    await typeInto(editionKey(host), 'colour')
    await choose(editionKind(host), 'fan')
    await typeInto(language(host), 'ja')
    await settle()

    const line = control(host, '.consequence').textContent
    expect(line).toContain('add edition colour')
    expect(line).toContain('kind fan')
    expect(line).toContain('language ja')
    expect(line).not.toContain('full-color-edition')
    expect(line).not.toContain('official')
  })

  it('names the collateral and refuses to submit rather than warning', async () => {
    // The measured `Publisher/` bug, on the surface a person actually uses:
    // the pattern is correct about the subject and also matches every other
    // series under `Publisher/`, whose volumes would be re-derived into the
    // accepted series' page sequence. `accept_with` refuses this with a 409,
    // so an enabled button beside the warning would make the guard look like
    // a suggestion.
    vi.mocked(curation.resolution).mockResolvedValue(
      clear({ collateral: ['Kaguya-sama wa Kokurasetai'] }),
    )
    const { host } = await ready()

    await typeInto(pattern(host), 'Publisher/')
    await settle()

    expect(refusalLines(host)).toHaveLength(1)
    expect(refusalLines(host)[0]).toContain('Kaguya-sama wa Kokurasetai')
    expect(refusalLines(host)[0]).toMatch(/nobody asked about/i)
    expect(accept(host).disabled).toBe(true)
    // "Accepting will:" is not true of a rule the server would refuse, so
    // the refusal replaces the sentence rather than sitting under it.
    expect(host.querySelector('.consequence')).toBeNull()
  })

  it('attaches the reason to the control it disables, in a region that announces', async () => {
    // A disabled button is not focusable, so the refusal beside it is a
    // sentence a keyboard or screen-reader user meets with no way to tell
    // what it belongs to — and `refusals` is the highest-stakes text on the
    // panel. `aria-describedby` binds the two, and the block is a live region
    // declared *statically*: the four states replace one another as separate
    // elements, so a region declared on the message itself would be created
    // at the moment its own content appeared and announce nothing.
    vi.mocked(curation.resolution).mockResolvedValue(
      clear({ collateral: ['Kaguya-sama wa Kokurasetai'] }),
    )
    const { host } = await ready()

    const why = control(host, '.why')
    expect(why.id).not.toBe('')
    expect(accept(host).getAttribute('aria-describedby')).toBe(why.id)
    expect(why.getAttribute('aria-live')).toBe('polite')
    // The refusal is *inside* the described element, not beside it: an id on
    // an empty box describes nothing.
    expect(why.querySelector('.refusals')).not.toBeNull()
    expect(why.textContent).toContain('Kaguya-sama wa Kokurasetai')
    expect(accept(host).disabled).toBe(true)
  })

  it('describes the enabled control with the sentence it is about to act on', async () => {
    // The same binding on the other side, which is what makes it a
    // description of the control rather than a wrapper for refusals: when
    // Accept can be pressed, what it is described by is what pressing it
    // does.
    const { host } = await ready()

    const why = control(host, '.why')
    expect(accept(host).getAttribute('aria-describedby')).toBe(why.id)
    expect(why.querySelector('.consequence')).not.toBeNull()
    expect(why.textContent).toContain('Accepting will:')
    expect(accept(host).disabled).toBe(false)
  })

  it('says what the accept also does, inside what describes the button', async () => {
    // **O73 named this line until the fold that closes it shipped.** The
    // consequence sentence says a new edition is added; what it cannot say
    // inside "Accepting will:" is that the works the material comes from are
    // folded away on the next scan, carrying a reader's own position, notes
    // and marked pages onto the new edition. That is a real consequence of
    // pressing this control, so it goes inside the described block rather
    // than somewhere else on the panel — a screen reader on Accept has to
    // reach it.
    const { host } = await ready()

    const why = control(host, '.why')
    expect(why.querySelector('.the-fold')).not.toBeNull()
    expect(why.textContent).toContain('folds these works together')
    // Beneath the sentence, not instead of it: the accept is offered and the
    // edition it creates is correct.
    expect(why.querySelector('.consequence')).not.toBeNull()
  })

  it('reaches the qualifications and the discards, not only the promise', async () => {
    // **The whole line has to be inside `.why`, not just its first clause.**
    // A screen reader on Accept reads this block and nothing else, and the
    // three things the fold does *not* carry — the marks the surviving
    // edition already holds, the older of two places in one edition, and the
    // tags, reading-list places and hand-corrected alignments the `DELETE`
    // takes (O76) — are the parts a person deciding cannot find out any
    // other way. Asserted here rather than only in `curation.test.ts`
    // because that file pins the string and this one pins that the string
    // reaches the control.
    const { host } = await ready()

    const why = control(host, '.why')
    expect(why.textContent).toContain('wherever they can')
    expect(why.textContent).toContain('only the newer survives')
    expect(why.textContent).toContain('alignment corrected by hand')
  })

  it('says nothing about what is left standing when the rule is refused', async () => {
    // A refusal replaces the consequence, and this line is a clause of the
    // consequence — an accept that will not happen leaves nothing standing.
    vi.mocked(curation.resolution).mockResolvedValue(
      clear({ collateral: ['Kaguya-sama wa Kokurasetai'] }),
    )
    const { host } = await ready()

    await typeInto(pattern(host), 'Publisher/')
    await settle()

    expect(host.querySelector('.the-fold')).toBeNull()
    expect(host.querySelector('.consequence')).toBeNull()
  })

  it('refuses on a target it would stamp, not only on collateral', async () => {
    // The second of the five, and one a form watching `collateral` alone
    // would offer an Accept for. A rule applies every `set_*` column to
    // everything it matches, so an `edition-of` rule reaching its own target
    // makes the base its own colour edition — and the target is deliberately
    // not counted as collateral, because it is the series being accepted
    // *into*.
    vi.mocked(curation.resolution).mockResolvedValue(
      clear({ targets_reached: ['Go-Toubun no Hanayome'] }),
    )
    const { host } = await ready()

    expect(refusalLines(host)).toHaveLength(1)
    expect(refusalLines(host)[0]).toMatch(/its own volumes/i)
    expect(refusalLines(host)[0]).toContain('Go-Toubun no Hanayome')
    expect(accept(host).disabled).toBe(true)
  })

  it('refuses a rule that would decide nothing', async () => {
    // Matched nothing at all — a regex typed while `Match as` still says
    // `prefix` lands here, as does a pattern for a directory that has been
    // renamed since the proposal was raised.
    vi.mocked(curation.resolution).mockResolvedValue(
      clear({ entries_matched: 0 }),
    )
    const { host } = await ready()

    expect(refusalLines(host)).toHaveLength(1)
    expect(refusalLines(host)[0]).toMatch(/matches nothing/i)
    expect(accept(host).disabled).toBe(true)
  })

  it('refuses a rule that matches plenty and never reaches its own subject', async () => {
    // The fourth. A pattern widened past the subject entirely still matches
    // and still moves works — it just answers a different question than the
    // proposal asked, and accepting it would record that rule as this
    // proposal's answer.
    vi.mocked(curation.resolution).mockResolvedValue(
      clear({ subject_reached: false }),
    )
    const { host } = await ready()

    expect(refusalLines(host)).toHaveLength(1)
    expect(refusalLines(host)[0]).toMatch(/subject/i)
    expect(accept(host).disabled).toBe(true)
  })

  it('refuses a rule that reaches its subject and changes nothing', async () => {
    // The subtlest of the five: a `(?<series>…)` capture overrides
    // `set_series` and hands the subject back the series it already has.
    // An entry matched, the subject reached, nothing collateral — and
    // accepting writes a rule that changes not one row.
    vi.mocked(curation.resolution).mockResolvedValue(clear({ works_moved: 0 }))
    const { host } = await ready()

    expect(refusalLines(host)).toHaveLength(1)
    expect(refusalLines(host)[0]).toMatch(/would change nothing/i)
    expect(accept(host).disabled).toBe(true)
  })

  it('will not accept on an answer that measured the previous rule', async () => {
    // The window between an edit and the sweep answering it. What is on
    // screen was measured for the *old* pattern, so a person who widens
    // `Publisher/Go-Toubun…/` to `Publisher/` and presses Accept inside the
    // quarter-second would accept on a clean bill of health for a rule they
    // have already replaced.
    const { host } = await ready()
    expect(accept(host).disabled).toBe(false)

    await typeInto(pattern(host), 'Publisher/')
    expect(accept(host).disabled).toBe(true)
    expect(control(host, '.checking').textContent).toBe(
      'Working out what this rule would do…',
    )
    expect(host.querySelector('.consequence')).toBeNull()

    await settle()
    expect(accept(host).disabled).toBe(false)
  })

  it('discards the sweep already in flight when the pattern is widened under it', async () => {
    // The other half of the window above, and the one that was not covered:
    // the first sweep has been *sent* and has not answered yet. The mount
    // sweep goes out a quarter-second after the panel opens, so this is the
    // ordinary timeline rather than a contrived one — widen at t=300 and the
    // narrow rule's answer arrives at t=400 for a pattern nobody is looking
    // at any more. Clearing `preview` does not touch it: only bumping the
    // sequence does, which is why the effect does both.
    //
    // What this refuses is the inverse of C3 §3's headline case. A widening
    // whose extra matches are the subjects of other open proposals is
    // `also_resolves`, not `collateral` — no refusal fires, the server
    // accepts it, and the sentence approved said "move 14 works" for a rule
    // that moves 54 and closes six other proposals.
    let answerNarrow: (r: curation.Resolution) => void = () => {}
    vi.mocked(curation.resolution)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            answerNarrow = resolve
          }),
      )
      .mockResolvedValueOnce(
        clear({ also_resolves: [2, 3, 4, 5, 6, 7], works_moved: 54 }),
      )

    const { host } = await ready()
    // Sent, and still out: nothing has resolved it.
    expect(curation.resolution).toHaveBeenCalledTimes(1)
    expect(control(host, '.checking')).toBeTruthy()

    await typeInto(pattern(host), 'Publisher/')
    await act(async () => {
      answerNarrow(clear())
      await Promise.resolve()
      await Promise.resolve()
    })

    expect(accept(host).disabled).toBe(true)
    expect(host.querySelector('.consequence')).toBeNull()
    expect(control(host, '.checking').textContent).toBe(
      'Working out what this rule would do…',
    )

    // And the form is not merely stuck: the sweep for the widened pattern
    // lands, and it is that answer's numbers the button is enabled against.
    await settle()
    expect(control(host, '.consequence').textContent).toContain('54 works')
    expect(accept(host).disabled).toBe(false)
  })

  it('drops the failure the previous rule earned when the pattern is edited', async () => {
    // The `previewFailure` half of the same clearing, and the state it
    // matters in: a 400 naming a pattern that will not compile is a sentence
    // about a pattern that is no longer in the box the moment it is edited.
    // Left standing it reads as a verdict on what is being typed now.
    vi.mocked(curation.resolution)
      .mockRejectedValueOnce(
        new Error(
          '/api/library/proposals/1/resolution: 400 this pattern will not compile: regex parse error: unclosed group',
        ),
      )
      .mockResolvedValueOnce(clear())

    const { host } = await ready()
    expect(control(host, '.preview-failure').textContent).toContain(
      'unclosed group',
    )

    await typeInto(pattern(host), 'Publisher/Go-Toubun no Hanayome/')
    expect(host.querySelector('.preview-failure')).toBeNull()
    expect(control(host, '.checking')).toBeTruthy()

    await settle()
    expect(control(host, '.consequence')).toBeTruthy()
  })

  it('keeps the answer to the last rule edited, not the last one to arrive', async () => {
    // Two sweeps of a library of thousands do not have to return in the
    // order they were sent. The slow one is sent first and answers last, so
    // a form without a sequence number shows a refusal for a pattern nobody
    // is looking at any more.
    let releaseSlow: (r: curation.Resolution) => void = () => {}
    vi.mocked(curation.resolution)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            releaseSlow = resolve
          }),
      )
      .mockResolvedValueOnce(clear({ works_moved: 41 }))

    const { host } = await ready()
    await typeInto(pattern(host), 'Publisher/')
    await settle()
    expect(control(host, '.consequence').textContent).toContain('41 works')

    await act(async () => {
      releaseSlow(clear({ collateral: ['Kaguya-sama wa Kokurasetai'] }))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.refusals')).toBeNull()
    expect(control(host, '.consequence').textContent).toContain('41 works')
    expect(accept(host).disabled).toBe(false)
  })

  it('keeps a good answer when a stale sweep rejects after it', async () => {
    // The `catch` side of the same sequence number. `.then` and `.catch`
    // carry their own guard, and mutating one leaves the test above green —
    // so a 500 from a sweep nobody is waiting for would replace a sentence
    // that is correct with an alert that is not.
    let rejectSlow: (err: unknown) => void = () => {}
    vi.mocked(curation.resolution)
      .mockImplementationOnce(
        () =>
          new Promise((_resolve, reject) => {
            rejectSlow = reject
          }),
      )
      .mockResolvedValueOnce(clear({ works_moved: 41 }))

    const { host } = await ready()
    await typeInto(pattern(host), 'Publisher/')
    await settle()
    expect(control(host, '.consequence').textContent).toContain('41 works')

    await act(async () => {
      rejectSlow(
        new Error('/api/library/proposals/1/resolution: 500 reading the roots'),
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(control(host, '.consequence').textContent).toContain('41 works')
    expect(accept(host).disabled).toBe(false)
  })

  it('drops a pending sweep when the row is closed inside the window', async () => {
    // Collapsing the row unmounts this form, and `debounce.ts` reserves
    // `cancel` for exactly that: there is no longer a screen for the answer
    // to land on, and `/resolution` is the heaviest route in the API.
    const { host } = await ready()
    await typeInto(pattern(host), 'Publisher/')

    // `rerender`, not a second `mount`: rendering nothing onto the same root
    // is what runs the effect cleanup. A fresh mount would orphan this one
    // instead, so the pending timer would survive and the test would pass on
    // a form that cancels nothing.
    await act(async () => {
      rerender(<div className="proposal-detail" />)
      await Promise.resolve()
    })
    await settle()

    expect(curation.resolution).toHaveBeenCalledTimes(1)
  })

  it('says why the dry run itself failed, and refuses to accept on it', async () => {
    // A 400 from the route, in the server's own words. This is not a rule
    // the engine refused to *apply* — it is one it could not compile at all,
    // so there is no `Resolution` to read refusals out of and the button has
    // nothing to stand on.
    vi.mocked(curation.resolution).mockRejectedValue(
      new Error(
        '/api/library/proposals/1/resolution: 400 this pattern will not compile: regex parse error: unclosed group',
      ),
    )
    const { host } = await ready()

    expect(control(host, '[role="alert"]').textContent).toBe(
      '/api/library/proposals/1/resolution: 400 this pattern will not compile: regex parse error: unclosed group',
    )
    expect(accept(host).disabled).toBe(true)
    expect(host.querySelector('.checking')).toBeNull()
    expect(host.querySelector('.consequence')).toBeNull()
  })

  it('stringifies a dry run rejection that is not an Error, rather than losing it', async () => {
    vi.mocked(curation.resolution).mockRejectedValue('offline')
    const { host } = await ready()

    expect(control(host, '[role="alert"]').textContent).toBe('offline')
  })

  it('writes exactly the rule shown, and asks the screen to re-read the inbox', async () => {
    // The rule the sweep approved is the rule the accept sends — not the one
    // the server offered before it was edited. Accepting a different rule
    // from the one previewed is the one thing C3 §7's contract forbids.
    let finish: (accepted: curation.Accepted) => void = () => {}
    vi.mocked(curation.acceptProposal).mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    const { host, accepted } = await ready({}, 7)
    await typeInto(editionKey(host), 'colour')
    await settle()

    expect(accept(host).textContent).toBe('Accept')
    await click(accept(host))

    expect(curation.acceptProposal).toHaveBeenCalledTimes(1)
    expect(curation.acceptProposal).toHaveBeenCalledWith(
      7,
      rule({ set_edition_key: 'colour' }),
    )
    // In flight: a second press would write a second rule, and `accept_with`
    // records only the last one it wrote.
    expect(accept(host).disabled).toBe(true)
    expect(accepted).toHaveLength(0)

    await act(async () => {
      finish({ rule_id: 91 })
      await Promise.resolve()
      await Promise.resolve()
    })
    // Both the list and the tab counts moved — the same reload dismissing
    // asks for.
    expect(accepted).toHaveLength(1)
  })

  it('stays disabled once the accept succeeds, through the reload it starts', async () => {
    // **The window is after the POST resolves and before anything unmounts,
    // and it is as long as a network round trip.** `onAccepted` is
    // `Proposals`' `reload`, which returns the moment it has bumped a
    // counter — the list it re-asks for is still in flight, and this row
    // stays on screen for the whole of it. A form that re-enables on success
    // therefore puts a live Accept back under the pointer for a proposal the
    // server has already moved to `accepted`; the second POST answers 409,
    // and the reward for a successful accept is a red `role="alert"`. This
    // is the one destructive control on the screen.
    //
    // Re-ordering `setAccepting(false)` and `onAccepted()` does not fix it
    // and this test says so: the second click below is made *after* both
    // have run, in either order, with nothing unmounted.
    const { host, accepted } = await ready()

    await click(accept(host))

    expect(accepted).toHaveLength(1)
    expect(curation.acceptProposal).toHaveBeenCalledTimes(1)
    expect(accept(host).disabled).toBe(true)

    // The form is still mounted and still showing a live consequence
    // sentence, so nothing but `accepting` is holding the button down.
    expect(control(host, '.consequence').textContent).not.toBe('')

    await click(accept(host))
    expect(curation.acceptProposal).toHaveBeenCalledTimes(1)
    expect(host.querySelector('.accept-failure')).toBe(null)
  })

  it('names the series a refused accept would have moved, in the server’s own words', async () => {
    // The 409 body, which `sendJson` carries into the error it throws
    // deliberately. Reduced to "409" it says a rule was refused; as it
    // stands it says which series stopped it, which is the only part of it
    // anybody can act on. The client can reach this state despite mirroring
    // every refusal: the library is swept twice, and something can move
    // between the dry run and the write.
    vi.mocked(curation.acceptProposal).mockRejectedValue(
      new Error(
        "/api/library/proposals/1/accept: 409 proposal 1's rule also decides for Kaguya-sama wa Kokurasetai, so accepting it would move that material too. Narrow the pattern or write the rule by hand.",
      ),
    )
    const { host, accepted } = await ready()

    await click(accept(host))

    expect(control(host, '.accept-failure').textContent).toBe(
      "/api/library/proposals/1/accept: 409 proposal 1's rule also decides for Kaguya-sama wa Kokurasetai, so accepting it would move that material too. Narrow the pattern or write the rule by hand.",
    )
    expect(control(host, '.accept-failure').getAttribute('role')).toBe('alert')
    expect(accepted).toHaveLength(0)
    // And it can be tried again — the rule is still on screen to be narrowed.
    expect(accept(host).disabled).toBe(false)
  })

  it('stringifies an accept rejection that is not an Error, rather than losing it', async () => {
    vi.mocked(curation.acceptProposal).mockRejectedValue('offline')
    const { host } = await ready()

    await click(accept(host))

    expect(control(host, '.accept-failure').textContent).toBe('offline')
  })
})
