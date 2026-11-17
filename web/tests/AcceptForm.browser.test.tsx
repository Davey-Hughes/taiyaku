import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { page } from 'vitest/browser'
import * as curation from '../src/curation'
import '../src/index.css'
import tokensSource from '../src/styles/tokens.css?raw'
import { ProposalRow } from '../src/ProposalRow'
import { mount } from './support/mount'

/**
 * The accept form, laid out — which is the one thing about it neither of the
 * other two projects can see.
 *
 * **Why this file exists.** The form's whole claim is that a person can read
 * the rule before it is written and widen it in place. Both halves are claims
 * about what fits on screen at a given width: `jsdom` returns zeroes from
 * `getBoundingClientRect`, so every assertion in `AcceptForm.test.tsx` would
 * pass just as happily against a pattern box measuring forty pixels, or
 * against one that pushed the panel wider than the window and took the
 * evidence above it along.
 *
 * **The values are the store's own**, not invented `reason` strings the server
 * never emits: the fixtures here are printed out of `proposals/tests.rs`'
 * `library_with_a_shared_publisher_directory` and the `format!` calls in
 * `proposals.rs` that word each field. The rule is what `offered_rule` derives
 * for that library, the pattern is the longest thing on the panel because a
 * real one is a `rel_path` prefix, and the refusal is the measured `Publisher/`
 * collateral in `refusals`' own sentence.
 *
 * **460 and 1100 for the reason `Proposals.browser.test.tsx` gives**: in the
 * phone layout the sidebar leaves the flow, and 460 − 2 × 16 gives `main`
 * 428px. The pattern keeps the form's whole width at both; what changes is how
 * many of the four short fields fit on the line beneath it, and whether the
 * prefix is readable end to end or scrolls.
 *
 * **Both colour schemes, set through `color-scheme` on the root.** Everything
 * below `.library` is drawn from the tokens in `styles/tokens.css`, each a
 * `light-dark()` pair that resolves against that property — so overriding it
 * is how a real Chromium is asked for the ink palette without a second browser
 * instance.
 *
 * CI does not run this project (see `vite.config.ts`: no Chromium), so
 * nothing here is a gate. It is the measurement, kept runnable.
 */

vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  proposalDetail: vi.fn(),
  dismissProposal: vi.fn(),
  resolution: vi.fn(),
  acceptProposal: vi.fn(),
}))

const LISTED: curation.ListedProposal = {
  id: 2,
  kind: 'edition-of',
  subject_id: 3,
  target_id: 4,
  subject_title: 'Go-Toubun no Hanayome - Full Color Edition',
  target_title: 'Go-Toubun no Hanayome',
  score: 2,
  reason:
    '"Go-Toubun no Hanayome - Full Color Edition" may be the "Full Color Edition" edition of "Go-Toubun no Hanayome" — the suffix matches the edition vocabulary; both hold 14 volume(s)',
  state: 'open',
  created_rule_id: null,
}

const RULE: curation.Rule = {
  root_id: 1,
  kind: 'prefix',
  pattern: 'Publisher/Go-Toubun no Hanayome - Full Color Edition/',
  match_on: 'path',
  set_series: 'Go-Toubun no Hanayome',
  set_edition_key: 'full-color-edition',
  set_kind: 'official',
  set_language: null,
  note: 'accepted from proposal 2: "Go-Toubun no Hanayome - Full Color Edition" is "Go-Toubun no Hanayome"',
}

const DETAIL: curation.Detail = {
  proposal: {
    id: 2,
    kind: 'edition-of',
    subject_id: 3,
    target_id: 4,
    score: 2,
    reason: LISTED.reason,
    state: 'open',
    created_rule_id: null,
  },
  subject: {
    series_id: 3,
    title: 'Go-Toubun no Hanayome - Full Color Edition',
    directories: [
      '/srv/manga/Publisher/Go-Toubun no Hanayome - Full Color Edition/',
    ],
    volume_count: 14,
    volumes: [1, 2, 3, 4, 5],
    root_name: 'Publisher',
    first_volume: {
      work_slug: 'go-toubun-no-hanayome-full-color-edition-v01',
      edition_key: 'full-color-edition',
      page_count: 196,
      formats: ['dir'],
      interior: [19, 98],
    },
  },
  target: {
    series_id: 4,
    title: 'Go-Toubun no Hanayome',
    directories: ['/srv/manga/Publisher/Go-Toubun no Hanayome/'],
    volume_count: 14,
    volumes: [1, 2, 3, 4, 5],
    root_name: 'Publisher',
    first_volume: {
      work_slug: 'go-toubun-no-hanayome-v01',
      edition_key: 'raw',
      page_count: 194,
      formats: ['dir'],
      interior: [19, 97],
    },
  },
  reasons: [
    {
      text: 'the suffix "Full Color Edition" matches the edition word "Color Edition"',
      sign: '+',
    },
    { text: 'both hold 14 volume(s)', sign: '+' },
  ],
  rule: RULE,
}

/** The narrow rule's own answer: it decides for its subject and nothing else. */
const NARROW: curation.Resolution = {
  also_resolves: [],
  collateral: [],
  targets_reached: [],
  entries_matched: 14,
  subject_reached: true,
  works_moved: 14,
}

/** The measured `Publisher/` bug — the refusal a widened pattern runs into. */
const COLLATERAL: curation.Resolution = {
  also_resolves: [],
  collateral: ['Kaguya-sama wa Kokurasetai'],
  targets_reached: [],
  entries_matched: 41,
  subject_reached: true,
  works_moved: 41,
}

const noop = () => {}

async function panelAt(
  width: number,
  scheme: 'light' | 'dark',
  answer: curation.Resolution = NARROW,
): Promise<HTMLElement> {
  await page.viewport(width, 900)
  document.body.replaceChildren()
  document.documentElement.style.colorScheme = scheme
  vi.mocked(curation.proposalDetail).mockResolvedValue(DETAIL)
  vi.mocked(curation.resolution).mockResolvedValue(answer)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <div className="library" style={{ height: '900px' }}>
        <div className="sidebar-column" />
        <main>
          <div className="proposals">
            <ul className="proposal-list">
              <ProposalRow
                proposal={LISTED}
                expanded={true}
                onToggle={noop}
                onChanged={noop}
              />
            </ul>
          </div>
        </main>
      </div>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  // Past the form's own debounce, with real timers.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 400))
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

const box = (host: HTMLElement, selector: string): DOMRect => {
  const el = host.querySelector(selector)
  if (!el) throw new Error(`no ${selector} on the panel`)
  return el.getBoundingClientRect()
}

/** How much of an element's content the box actually shows. */
const cut = (host: HTMLElement, selector: string): boolean => {
  const el = host.querySelector(selector)
  if (!el) throw new Error(`no ${selector} on the panel`)
  return el.scrollWidth > Math.ceil(el.getBoundingClientRect().width)
}

/** Nothing on this panel may push the list wider than the column holding it. */
const fitsItsColumn = (host: HTMLElement) => {
  expect(
    host.querySelector('.proposal-list')?.scrollWidth ?? 0,
  ).toBeLessThanOrEqual(host.querySelector('main')?.clientWidth ?? 0)
}

describe('the accept form, in a browser', () => {
  it('gives the pattern a box worth reading at 1100px, on one row of fields', async () => {
    const host = await panelAt(1100, 'light')

    // The pattern has the form's width, and at this one that is the whole
    // `rel_path` prefix with nothing scrolled out of sight, which is the claim
    // the form makes by putting it on screen. Sharing the row with the short
    // fields would cut it.
    const form = box(host, '.accept-form')
    expect(box(host, '.field.pattern input').width).toBeGreaterThan(
      form.width * 0.9,
    )
    expect(cut(host, '.field.pattern input')).toBe(false)

    // The root-scope line takes the row directly under the box it is about,
    // rather than becoming a sixth item on the row of fields — where it would
    // read as another thing to fill in, and would push a field off the line.
    // It is read, so it keeps the 14px floor every line of reading text
    // holds to (spec 2026-10-18 §3), and is quiet by colour instead: `--muted`
    // where the sentence over the button is `--text`.
    const scope = box(host, '.accept-form .scope')
    expect(scope.top).toBeGreaterThan(box(host, '.field.pattern').top)
    expect(scope.width).toBeGreaterThan(form.width * 0.9)
    const scopeText = host.querySelector('.accept-form .scope')
    const sentence = host.querySelector('.consequence')
    if (!scopeText || !sentence) throw new Error('no scope line on the panel')
    expect(
      parseFloat(getComputedStyle(scopeText).fontSize),
    ).toBeGreaterThanOrEqual(14)
    const painted = (value: string) => {
      const probe = document.createElement('span')
      probe.style.color = value
      host.append(probe)
      const colour = getComputedStyle(probe).color
      probe.remove()
      return colour
    }
    // The two tokens differ, or the two lines below could not tell them apart.
    expect(painted('var(--muted)')).not.toBe(painted('var(--text)'))
    expect(getComputedStyle(scopeText).color).toBe(painted('var(--muted)'))
    expect(getComputedStyle(sentence).color).toBe(painted('var(--text)'))

    // And the four short fields share the line under it, so what the rule
    // sets is read as one row rather than as a column of boxes.
    const second = box(host, '.field.match-kind').top
    expect(second).toBeGreaterThan(scope.top)
    for (const field of [
      '.field.edition-key',
      '.field.edition-kind',
      '.field.language',
    ]) {
      expect(box(host, field).top).toBe(second)
    }

    // And the panel is still inside its column — a wide input laid out at
    // its content width would take the evidence above it with it.
    fitsItsColumn(host)
  })

  it('wraps the fields at 460px rather than crushing the pattern', async () => {
    const host = await panelAt(460, 'light')

    // The pattern still has the whole row, and the short fields wrap under
    // it rather than beside it. No `cut` assertion: at this width the pattern
    // does not hold 52 monospace characters at any layout, so what matters
    // here is that the box scrolls instead of the panel growing, which
    // `fitsItsColumn` below says.
    const form = box(host, '.accept-form')
    expect(box(host, '.field.pattern input').width).toBeGreaterThan(
      form.width * 0.9,
    )
    expect(box(host, '.field.match-kind').top).toBeGreaterThan(
      box(host, '.field.pattern').top,
    )

    fitsItsColumn(host)
  })

  it('wraps the consequence sentence rather than letting it out of the panel', async () => {
    // The sentence is long by construction — it names the edition, the kind,
    // the series, a count of works and what is *not* touched — and it is the
    // thing a person reads before pressing the button under it.
    const host = await panelAt(460, 'light')

    expect(cut(host, '.consequence')).toBe(false)
    expect(box(host, '.consequence').width).toBeLessThanOrEqual(
      box(host, '.accept-form').width,
    )

    // The line under it — what the accept leaves standing — is longer still,
    // and it names an open item at the end of the sentence, which is the part
    // a clipped box loses first.
    expect(cut(host, '.the-fold')).toBe(false)
    expect(box(host, '.the-fold').width).toBeLessThanOrEqual(
      box(host, '.accept-form').width,
    )
    expect(box(host, '.the-fold').top).toBeGreaterThanOrEqual(
      box(host, '.consequence').top,
    )

    fitsItsColumn(host)
  })

  it('draws the refusal and a disabled Accept, in both colour schemes', async () => {
    for (const scheme of ['light', 'dark'] as const) {
      const host = await panelAt(460, scheme, COLLATERAL)

      const control = host.querySelector<HTMLButtonElement>('button.accept')
      if (!control) throw new Error('no accept control rendered')
      expect(control.disabled).toBe(true)
      expect(host.querySelector('.refusals li')?.textContent).toContain(
        'Kaguya-sama wa Kokurasetai',
      )
      expect(host.querySelector('.consequence')).toBeNull()

      // **Both of them at full contrast.** Taking WCAG's inactive-component
      // exemption would give the control a colour that differs from the
      // refusal's, and `GrayText` on `ButtonFace` is 1.35:1 in dark: a button
      // with no word on it in the state the form sits in for the whole of every
      // widening. The refusal and the control share the reader's text colour;
      // that Accept is unavailable is said below, by the ground and the border.
      // `var(--text)` resolves to real pixels here and in `jsdom` it does not
      // resolve at all, which is why this lives in this project.
      const refusal = host.querySelector('.refusals li')
      if (!refusal) throw new Error('no refusal line rendered')
      const bodyText = getComputedStyle(document.body).color
      expect(getComputedStyle(refusal).color).toBe(bodyText)
      expect(getComputedStyle(control).color).toBe(bodyText)

      fitsItsColumn(host)

      // And it is still not drawn as the live control — measured against a
      // live one in the same scheme rather than against a remembered value.
      // Read off the page before the next `panelAt`, which empties the body:
      // `getComputedStyle` of a detached element answers `''` for everything,
      // which is not a value that fails any comparison honestly.
      const refused = {
        background: getComputedStyle(control).backgroundColor,
        border: getComputedStyle(control).borderStyle,
      }
      const live = await panelAt(460, scheme)
      const enabled = live.querySelector<HTMLButtonElement>('button.accept')
      if (!enabled) throw new Error('no accept control rendered')
      expect(enabled.disabled).toBe(false)
      expect(refused.background).not.toBe(
        getComputedStyle(enabled).backgroundColor,
      )
      expect(refused.border).toBe('dashed')
      expect(getComputedStyle(enabled).borderStyle).toBe('solid')
    }
  })

  it('paints the tokens `index.css.test.ts` computes its ratios from', async () => {
    // That file cannot render, so it parses `tokens.css`'s `light-dark()` pairs
    // and recomputes WCAG's math from them, assuming paper under `light` and
    // ink under `dark`. This is the check that a real Chromium agrees: every
    // pair, resolved in each scheme, paints the half the model took. What
    // fails here while the model goes on passing is a rule elsewhere in the
    // stylesheet redefining a token, and Chromium disagreeing with the model
    // about which half `color-scheme` picks. A pair written the wrong way
    // round does *not* fail here — this reads the same pair the model does
    // and expects the same half — and `index.css.test.ts` catches it instead:
    // its floors for most pairs, and for `--line` and `--line-strong`, whose
    // reversals clear every floor, its literal table of Ink's lifted halves.
    //
    // Every mismatch is collected before the assertion, as that file's floors
    // test does, so a token redefined for both schemes is named in both.
    const pairs = [
      ...tokensSource.matchAll(
        /--([a-z-]+):\s*light-dark\(\s*(#[0-9a-fA-F]{6})\s*,\s*(#[0-9a-fA-F]{6})\s*\)/g,
      ),
    ]
    // Exactly spec §2's twelve, so an unread or empty file fails here rather
    // than passing a loop that ran no iterations.
    expect(pairs).toHaveLength(12)
    const rgb = (hex: string) => {
      const n = Number.parseInt(hex.slice(1), 16)
      return `rgb(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255})`
    }

    const mismatches: string[] = []
    for (const scheme of ['light', 'dark'] as const) {
      document.documentElement.style.colorScheme = scheme
      const probe = document.createElement('span')
      document.body.append(probe)
      try {
        for (const [, name, paper, ink] of pairs) {
          probe.style.color = `var(--${name})`
          const painted = getComputedStyle(probe).color
          const modelled = rgb(scheme === 'light' ? paper : ink)
          if (painted !== modelled) {
            mismatches.push(
              `${scheme} --${name}: painted ${painted}, modelled ${modelled}`,
            )
          }
        }
      } finally {
        probe.remove()
      }
    }
    expect(mismatches).toEqual([])
  })
})
