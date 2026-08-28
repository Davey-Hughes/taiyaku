import { describe, expect, it, vi } from 'vitest'
import { Unauthenticated } from './api'
import {
  acceptProposal,
  consequence,
  dismissProposal,
  INBOX_MIN_SCORE,
  inboxTotal,
  kindLabel,
  THE_FOLD,
  listProposals,
  proposalDetail,
  refusals,
  relation,
  resolution,
  strength,
  unacceptProposal,
  type Detail,
  type Resolution,
  type Rule,
} from './curation'

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  })

const rule = (over: Partial<Rule> = {}): Rule => ({
  root_id: null,
  kind: 'prefix',
  pattern: 'Monster - The Perfect Edition/',
  match_on: 'path',
  set_series: null,
  set_edition_key: 'en-color',
  set_kind: 'official',
  set_language: null,
  note: '',
  ...over,
})

const clearResolution = (over: Partial<Resolution> = {}): Resolution => ({
  also_resolves: [],
  collateral: [],
  targets_reached: [],
  entries_matched: 9,
  subject_reached: true,
  works_moved: 9,
  ...over,
})

describe('strength', () => {
  it('calls two signals strong and one weak', () => {
    // The scale is {0,1,2,3}: `edition-of` adds one per signal and the two
    // certain kinds sit at 3. Strong is 2 and above.
    expect(strength(2)).toBe('strong')
    expect(strength(3)).toBe('strong')
    expect(strength(1)).toBe('weak')
    expect(strength(0)).toBe('weak')
  })
})

describe('kindLabel', () => {
  it('names every kind this store raises, each with its own words', () => {
    // Asserted one by one rather than as a spot check: a map whose entries
    // all pointed at one label, or a lookup that returned the same value for
    // every key, would satisfy any single assertion here.
    expect(kindLabel('edition-of')).toBe('Edition Of')
    expect(kindLabel('same-series')).toBe('Same Series')
    expect(kindLabel('attach-chapters')).toBe('Attach Chapters')
    expect(kindLabel('align')).toBe('Align')
  })

  it('passes a kind it has never heard of through unchanged', () => {
    // The tabs are built from the server's own `counts` keys, so a kind
    // added to the store before this map hears about it still has to name
    // itself rather than render as nothing.
    expect(kindLabel('future-kind')).toBe('future-kind')
  })
})

describe('relation', () => {
  it('words each kind\u2019s claim about its two series in its own terms', () => {
    // One by one, for `kindLabel`'s reason: a map whose entries all pointed
    // at one phrase satisfies any single assertion. These two are the only
    // kinds this store raises *with* a target, and they claim different
    // things — one series being an edition of another is not the same claim
    // as the two being one series.
    expect(relation('edition-of')).toBe('an edition of')
    expect(relation('same-series')).toBe('the same series as')
  })

  it('says only that the two are related, for a kind it has never heard of', () => {
    // **Deliberately vaguer than `kindLabel`'s fallback, which is the raw
    // kind string.** A kind is a name, so echoing it back is honest; this
    // phrase is a *claim about two series*, and a client cannot make one on
    // behalf of a kind the server invented after it shipped. `align` is the
    // live case — nothing raises it today, and this map does not name it.
    expect(relation('align')).toBe('related to')
    expect(relation('future-kind')).toBe('related to')
  })
})

describe('consequence', () => {
  it('states what accepting does, including what it does not touch', () => {
    // `entries_matched` and `works_moved` are deliberately different numbers
    // here (15 vs 9) — equal numbers would let a version of `consequence`
    // that reads `entries_matched` instead of `works_moved` pass this test
    // by accident.
    const line = consequence(
      { title: 'Monster - The Perfect Edition' },
      {
        set_edition_key: 'en-color',
        set_kind: 'official',
        set_language: null,
      },
      clearResolution({ entries_matched: 15, works_moved: 9 }),
    )
    expect(line).toContain('en-color')
    expect(line).toContain('official')
    expect(line).toContain('9 works')
    expect(line).not.toContain('15')
    expect(line).toContain('Nothing on disk is touched')
    // **The verb, pinned.** This said `move 9 works` until a reproduction
    // from a clean database — scan, accept, rescan — showed the matched
    // volumes arriving as a new edition of the target's works while their
    // original works stayed standing over the same files (O73, since closed
    // by the fold `THE_FOLD` now describes). Nothing moves *on disk*,
    // and the word is the whole finding — the fold that follows moves rows,
    // not files.
    expect(line).not.toContain('move')
    expect(line).toContain('from 9 works')
    // And when: the accept writes a rule and changes no `edition` row.
    expect(line).toContain('until the next scan')
    expect(line).not.toContain('language')
    // The sentence's whole job is naming what accepting does *to what* — a
    // version that renders "to series (unknown)" reads identically on every
    // assertion above.
    expect(line).toContain('to series Monster - The Perfect Edition')
  })

  it('says how many other proposals a widened rule answers', () => {
    // Same reasoning as above: `entries_matched` (60) and `works_moved` (54)
    // are different numbers, so asserting on '54 works' actually pins which
    // field the sentence reads rather than passing on either.
    const line = consequence(
      { title: 'Monster - The Perfect Edition' },
      { set_edition_key: 'en-color', set_kind: 'official', set_language: null },
      clearResolution({
        also_resolves: [2, 3, 4, 5, 6, 7],
        entries_matched: 60,
        works_moved: 54,
      }),
    )
    // Pins the wording, not just the number: the mockup and `proposals.rs`'s
    // own `Resolution` doc both say "resolves", not "answer".
    expect(line).toMatch(/resolves 6 more/i)
    expect(line).toContain('54 works')
    expect(line).not.toContain('60')
  })

  it('says "1 more" without the plural s for exactly one other proposal', () => {
    const line = consequence(
      { title: 'Monster - The Perfect Edition' },
      { set_edition_key: 'en-color', set_kind: 'official', set_language: null },
      clearResolution({ also_resolves: [4] }),
    )
    expect(line).toContain('1 more proposal')
    expect(line).not.toContain('1 more proposals')
  })

  it('falls back to (none) when the rule sets no edition key, and omits an unset kind', () => {
    const line = consequence(
      { title: 'Monster - The Perfect Edition' },
      { set_edition_key: null, set_kind: null, set_language: null },
      clearResolution(),
    )
    expect(line).toContain('(none)')
    expect(line).not.toContain('kind')
  })

  it('names the language when the rule sets one', () => {
    const line = consequence(
      { title: 'Monster - The Perfect Edition' },
      { set_edition_key: 'en-color', set_kind: 'official', set_language: 'en' },
      clearResolution(),
    )
    expect(line).toContain('language en')
  })
})

describe('THE_FOLD', () => {
  it('describes the fold rather than the duplication it closed', () => {
    // Until O73 closed this line named the open item and asserted `twice`
    // and `not.toContain('move')` — the accept-time truth being that nothing
    // moved and the duplication was permanent. The fold built to close it
    // (`derive::superseded`, `fold::absorb`) now removes the superseded work
    // on the next scan and carries a reader's own state onto the surviving
    // edition, so the line has to say the opposite of what it said before:
    // this pins that it does, and that the rarer refusal path — where moving
    // a reader's own state would not be safe — is still named rather than
    // silently dropped.
    expect(THE_FOLD).toContain('folds these works together')
    expect(THE_FOLD).toContain('refuses instead')
    expect(THE_FOLD).not.toContain('O73')
  })

  it('promises the move only as far as the store makes it', () => {
    // **The unqualified promise is what this replaces.** The line said "any
    // reader position, note or marked page on them moves onto this edition
    // instead", and `fold::Moved` carries a counter for each of the three
    // ways that is not so — `collisions`, `lost_positions` and `discarded`,
    // every one of them constructed by a test in `taiyaku-library`. This is
    // the control that causes the fold, so `works_moved`'s rule in
    // `proposals.rs` governs it: a false statement to the person deciding is
    // a defect whether it is a number or a verb.
    expect(THE_FOLD).toContain('move onto this edition wherever they can')
    expect(THE_FOLD).not.toContain('moves onto this edition instead')
    // `Moved::collisions`: the surviving edition's own marks win.
    expect(THE_FOLD).toContain('already has marked keeps its own marks')
    // `Moved::lost_positions`: two places in one edition, one survivor.
    expect(THE_FOLD).toContain('only the newer survives')
  })

  it('names what the fold discards, which nothing on this panel said', () => {
    // `Moved::discarded` takes the work's `work_tag` rows and its
    // `collection_item` places — a reader's own curation — along with its
    // credits and alignments. A hand-corrected alignment is **O76**, open
    // against exactly this, and the panel offering the accept was silent
    // about all of it.
    expect(THE_FOLD).toContain('discards')
    expect(THE_FOLD).toContain('tags')
    expect(THE_FOLD).toContain('places in reading lists')
    expect(THE_FOLD).toContain('alignment corrected by hand')
  })
})

describe('inboxTotal', () => {
  it('adds every kind up, rather than reading one of them', () => {
    // Three different numbers, none of them the total and no two equal: a
    // version returning `counts[0]`, the largest, or the number of kinds
    // passes any fixture where two of those coincide.
    expect(inboxTotal({ 'edition-of': 5, 'same-series': 2, align: 4 })).toBe(11)
  })

  it('is zero for an inbox the server reports no kinds for', () => {
    // A kind with nothing open is *absent* from `counts`, not present with a
    // zero — so the empty map is the real shape of an empty inbox, and the
    // sidebar draws a `0` from it rather than nothing.
    expect(inboxTotal({})).toBe(0)
  })
})

describe('INBOX_MIN_SCORE', () => {
  it('is the threshold §6.1 puts the inbox at, not the bottom of the scale', () => {
    // The scale is {0, 1, 2, 3} and 0 is the measured false-positive shape.
    // A constant of 0 here would make the sidebar's badge count proposals the
    // screen it links to does not list — the exact disagreement the constant
    // exists to make impossible.
    expect(INBOX_MIN_SCORE).toBe(1)
  })
})

describe('refusals', () => {
  // Each case below isolates exactly one of the five conditions — the other
  // four are left in their non-refusing state — so a guard that was deleted
  // or that reads the wrong field fails only the one test built to catch it,
  // never all five at once.

  it('is empty when every condition is clear, even with other proposals answered', () => {
    // `also_resolves` is non-empty here on purpose: it is the sixth field on
    // `Resolution` and is not a refusal, so a version of `refusals` that
    // mistakenly treated it as one would fail only this case.
    expect(refusals(clearResolution({ also_resolves: [2, 3] }))).toEqual([])
  })

  it('names the collateral series nobody asked about', () => {
    const lines = refusals(
      clearResolution({ collateral: ['Kaguya-sama wa Kokurasetai'] }),
    )
    expect(lines).toHaveLength(1)
    // Without this, the *targets_reached* wording ("its own volumes") on the
    // collateral data would pass this test too — the wrong explanation on
    // the right data, reaching the screen green.
    expect(lines[0]).toMatch(/nobody asked about/i)
    expect(lines[0]).toContain('Kaguya-sama wa Kokurasetai')
  })

  it('names a target the rule would also stamp', () => {
    const lines = refusals(
      clearResolution({ targets_reached: ['Monster - The Perfect Edition'] }),
    )
    expect(lines).toHaveLength(1)
    expect(lines[0]).toMatch(/its own volumes/i)
    expect(lines[0]).toContain('Monster - The Perfect Edition')
  })

  it('refuses a rule that matches nothing', () => {
    const lines = refusals(
      clearResolution({ entries_matched: 0, subject_reached: true }),
    )
    expect(lines).toHaveLength(1)
    expect(lines[0]).toMatch(/matches nothing/i)
  })

  it('refuses a rule that never reaches the proposal’s own subject', () => {
    const lines = refusals(clearResolution({ subject_reached: false }))
    expect(lines).toHaveLength(1)
    expect(lines[0]).toMatch(/subject/i)
  })

  it('refuses a rule that reaches the subject and changes nothing', () => {
    const lines = refusals(clearResolution({ works_moved: 0 }))
    expect(lines).toHaveLength(1)
    expect(lines[0]).toMatch(/would change nothing/i)
  })
})

describe('the curation HTTP client', () => {
  it('lists the inbox with no filter, and with every filter applied', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        Promise.resolve(json({ proposals: [], counts: {} })),
      )
    vi.stubGlobal('fetch', fetchMock)

    await listProposals()
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/proposals')

    await listProposals({ state: 'open', kind: 'edition-of', min_score: 2 })
    expect(fetchMock.mock.calls[1]?.[0]).toBe(
      '/api/library/proposals?state=open&kind=edition-of&min_score=2',
    )
  })

  it('reads one proposal by id', async () => {
    const detail: Detail = {
      proposal: {
        id: 7,
        kind: 'edition-of',
        subject_id: 1,
        target_id: 2,
        score: 3,
        reason: 'matches',
        state: 'open',
        created_rule_id: null,
      },
      subject: {
        series_id: 1,
        title: 'Monster',
        directories: ['Monster/'],
        volume_count: 9,
        volumes: [1, 2, 3],
        root_name: 'library',
        first_volume: null,
      },
      target: null,
      reasons: [{ text: 'same title', sign: '+' }],
      rule: null,
    }
    const fetchMock = vi.fn().mockResolvedValue(json(detail))
    vi.stubGlobal('fetch', fetchMock)

    await expect(proposalDetail(7)).resolves.toEqual(detail)
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/proposals/7')
  })

  it('posts a candidate rule for a dry run, and returns the resolution', async () => {
    const calls: RequestInit[] = []
    const res = clearResolution({ also_resolves: [4] })
    vi.stubGlobal('fetch', (_url: string, init: RequestInit) => {
      calls.push(init)
      return Promise.resolve(json(res))
    })

    const theRule = rule()
    await expect(resolution(7, theRule)).resolves.toEqual(res)
    expect(calls[0].method).toBe('POST')
    expect(new Headers(calls[0].headers).get('content-type')).toBe(
      'application/json',
    )
    expect(JSON.parse(calls[0].body as string)).toEqual(theRule)
  })

  it('accepts a proposal, writing exactly the rule it was shown', async () => {
    const calls: RequestInit[] = []
    vi.stubGlobal('fetch', (_url: string, init: RequestInit) => {
      calls.push(init)
      return Promise.resolve(json({ rule_id: 42 }))
    })

    const theRule = rule()
    await expect(acceptProposal(7, theRule)).resolves.toEqual({ rule_id: 42 })
    expect(calls[0].method).toBe('POST')
    expect(JSON.parse(calls[0].body as string)).toEqual(theRule)
  })

  /**
   * 409's body is the feature, not the status: the server refuses an accept
   * by naming the series it would have moved, so the thrown error has to
   * carry that text rather than only the code.
   */
  it('carries the 409 body — the series it names — into the thrown error', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        new Response('would move Kaguya-sama wa Kokurasetai', {
          status: 409,
        }),
      ),
    )

    await expect(acceptProposal(7, rule())).rejects.toThrow(
      /Kaguya-sama wa Kokurasetai/,
    )
  })

  it('un-accepts and dismisses with no body, resolving to undefined on a 204', async () => {
    const calls: Array<[string, RequestInit]> = []
    vi.stubGlobal('fetch', (url: string, init: RequestInit) => {
      calls.push([url, init])
      return Promise.resolve(new Response(null, { status: 204 }))
    })

    await expect(unacceptProposal(7)).resolves.toBeUndefined()
    await expect(dismissProposal(7)).resolves.toBeUndefined()

    expect(calls[0][0]).toBe('/api/library/proposals/7/accept')
    expect(calls[0][1].method).toBe('DELETE')
    expect(calls[0][1].body).toBeUndefined()
    // Pins that a body-less call sends no `content-type` header either —
    // without this, always sending both `headers` and
    // `body: JSON.stringify(body)` would still pass, since
    // `JSON.stringify(undefined) === undefined`.
    expect(calls[0][1].headers).toBeUndefined()
    expect(calls[1][0]).toBe('/api/library/proposals/7/dismiss')
    expect(calls[1][1].method).toBe('POST')
    expect(calls[1][1].body).toBeUndefined()
    expect(calls[1][1].headers).toBeUndefined()
  })

  it('routes a 401 through the one place that notices it, from a write too', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 401 })),
    )
    await expect(dismissProposal(7)).rejects.toBeInstanceOf(Unauthenticated)
  })
})
