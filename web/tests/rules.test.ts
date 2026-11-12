import { afterEach, describe, expect, it, vi } from 'vitest'
import type { Root } from '../src/config'
import {
  blankRule,
  FIELDS,
  fromDocument,
  importRules,
  listRules,
  MATCH_ONS,
  moved,
  previewRules,
  renumber,
  type Row,
  type Rule,
  type RuleDoc,
  RULE_KINDS,
  ruleMechanism,
  ruleName,
  saveRules,
  toDocument,
  unsetFields,
  unsetSentence,
} from '../src/rules'

const rule = (over: Partial<Rule> = {}): Rule => ({
  id: 1,
  ordinal: 1,
  enabled: true,
  root_id: null,
  kind: 'regex',
  pattern: 'Manga \\(RAW\\)/.*',
  match_on: 'path',
  set_series: null,
  set_edition_key: 'raw',
  set_kind: null,
  set_language: null,
  note: null,
  ...over,
})

const root = (over: Partial<Root> = {}): Root => ({
  id: 1,
  path: '/srv/manga',
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  exclude_globs: null,
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
  next_scan_at: null,
  ...over,
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('the wire vocabularies', () => {
  it('spells the four rule kinds the store parses', () => {
    // `rules.rs`'s own `FromStr` — a fifth value here is a 400 the editor
    // would offer in a dropdown.
    expect([...RULE_KINDS]).toEqual([
      'prefix',
      'regex',
      'builtin-volume',
      'builtin-chapter',
    ])
  })

  it('spells the three match_on values the store parses', () => {
    expect([...MATCH_ONS]).toEqual(['path', 'name', 'parent'])
  })
})

describe('the export/import converter', () => {
  it('writes a root path, never a root id', () => {
    // A document carried between libraries cannot mean anything by a bare
    // number — the build design's §5.6.
    const doc = toDocument([rule({ root_id: 1 })], [root()])
    expect(doc[0].root_path).toBe('/srv/manga')
    expect('root_id' in doc[0]).toBe(false)
    expect('id' in doc[0]).toBe(false)
  })

  it('writes null for a rule scoped to every root', () => {
    expect(toDocument([rule({ root_id: null })], [root()])[0].root_path).toBe(
      null,
    )
  })

  it('round-trips through one converter, so an export and an import cannot disagree', () => {
    const roots = [root(), root({ id: 2, path: '/srv/raw' })]
    const rules = [
      rule({ id: 3, ordinal: 1, root_id: 2 }),
      rule({ id: 4, ordinal: 2, root_id: null, kind: 'prefix' }),
    ]
    const back = fromDocument(toDocument(rules, roots), roots)
    expect(back.map((r) => r.root_id)).toEqual([2, null])
    expect(back.map((r) => r.kind)).toEqual(['regex', 'prefix'])
  })

  it('carries every field across the trip, not only the ones spot-checked', () => {
    // **A round-trip test is satisfied by a converter that drops a field on
    // BOTH sides**, which is exactly what an export/import boundary must not
    // do: the field vanishes from the document, the import cannot restore
    // what is not there, and the rule comes back subtly different in a
    // library nobody is looking at. Checking `root_id` and `kind` alone —
    // which the test above does — cannot see it. This pins the whole shape
    // against a rule whose every nullable column is set to something
    // distinguishable, so a dropped column reads as `null` rather than as its
    // value.
    const roots = [root()]
    const full = rule({
      id: 7,
      ordinal: 3,
      enabled: false,
      root_id: 1,
      kind: 'prefix',
      pattern: 'Colour/',
      match_on: 'parent',
      set_series: 'A Series',
      set_edition_key: 'en-color',
      set_kind: 'manga',
      set_language: 'ja',
      note: 'why this rule exists',
    })

    const [back] = fromDocument(toDocument([full], roots), roots)

    // `id` is the one field that must NOT survive: a document carries none of
    // this library's ids, so an import reads every rule as unsaved.
    expect(back).toEqual({ ...full, id: 0 })
  })

  it('reads an imported rule back as unsaved', () => {
    // `id: 0` is the store's own sentinel for "not written yet". An imported
    // document carries none of this library's ids and must not invent one.
    const doc: RuleDoc[] = [
      {
        ordinal: 1,
        enabled: true,
        root_path: '/srv/manga',
        kind: 'regex',
        pattern: '.*',
        match_on: 'path',
        set_edition_key: null,
        set_kind: null,
        set_language: null,
        set_series: null,
        note: null,
      },
    ]
    expect(fromDocument(doc, [root()])[0].id).toBe(0)
  })

  it('refuses a root path this library does not have, naming it', () => {
    const doc: RuleDoc[] = [
      {
        ordinal: 1,
        enabled: true,
        root_path: '/somewhere/else',
        kind: 'regex',
        pattern: '.*',
        match_on: 'path',
        set_edition_key: null,
        set_kind: null,
        set_language: null,
        set_series: null,
        note: null,
      },
    ]
    expect(() => fromDocument(doc, [root()])).toThrow('/somewhere/else')
  })

  it('refuses a root id this library does not have, naming it', () => {
    // The other direction of the throw above. A rule can outlive the root it
    // scopes to — the root row deleted, the rule left behind — and Export has
    // to fail loudly rather than write a document that silently drops the
    // scope or invents a path for an id nobody has anymore.
    expect(() =>
      toDocument([rule({ ordinal: 5, root_id: 99 })], [root()]),
    ).toThrow('rule 5 names root 99')
  })
})

describe('reordering', () => {
  it('moves one entry and closes the gap behind it', () => {
    expect(moved(['a', 'b', 'c', 'd'], 0, 2)).toEqual(['b', 'c', 'a', 'd'])
    expect(moved(['a', 'b', 'c', 'd'], 3, 1)).toEqual(['a', 'd', 'b', 'c'])
  })

  it('leaves the list alone when nothing moves', () => {
    expect(moved(['a', 'b', 'c'], 1, 1)).toEqual(['a', 'b', 'c'])
  })

  it('renumbers ordinals to the list position, one-based', () => {
    // Layering is positional: `Engine::compile` orders by `ordinal` then id,
    // so a drag that changed the array and not the ordinals would preview
    // exactly as it did before it.
    const out = renumber([rule({ ordinal: 9 }), rule({ id: 2, ordinal: 3 })])
    expect(out.map((r) => r.ordinal)).toEqual([1, 2])
  })
})

describe('a blank rule', () => {
  it('carries the unsaved sentinel and the given ordinal', () => {
    const fresh = blankRule(4)
    expect(fresh.id).toBe(0)
    expect(fresh.ordinal).toBe(4)
    expect(fresh.enabled).toBe(true)
    expect(RULE_KINDS).toContain(fresh.kind)
    expect(MATCH_ONS).toContain(fresh.match_on)
  })
})

describe('the client', () => {
  it('sends the whole proposed list to the preview route', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          impact: {},
          rows: [],
          rows_truncated: false,
          overrides: [],
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    )

    await previewRules([rule()], 0)

    const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(path).toBe('/api/library/rules/preview')
    expect(init.method).toBe('POST')
    expect(JSON.parse(init.body as string)).toEqual({
      rules: [rule()],
      selected: 0,
    })
  })

  it('omits `selected` entirely when no rule is open', async () => {
    // The route reads absent and null the same way, but sending `selected`
    // for nothing open would make the server build a row table nobody asked
    // for — one `scan_entry` walk and a second Engine, per `preview_with_rows`.
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          impact: {},
          rows: [],
          rows_truncated: false,
          overrides: [],
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    )

    await previewRules([rule()])

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(JSON.parse(init.body as string)).toEqual({ rules: [rule()] })
  })

  it('saves with PUT, to the collection URL', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          impact: {},
          rules: [],
          reopened_proposals: [],
          complaints: [],
        }),
        { status: 200, headers: { 'content-type': 'application/json' } },
      ),
    )

    await saveRules([rule()])

    const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(path).toBe('/api/library/rules')
    expect(init.method).toBe('PUT')
    expect(JSON.parse(init.body as string)).toEqual({ rules: [rule()] })
  })

  it('throws the server’s own sentence when a save is refused', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response('rule 9999: no such rule exists to keep', { status: 400 }),
    )
    await expect(saveRules([rule()])).rejects.toThrow('no such rule exists')
  })

  it('throws on an empty 204, rather than handing a caller undefined as if it were a body', async () => {
    // Every route this module calls always answers a body on success —
    // `sendJson` only returns `undefined` for a 204, and none of these routes
    // are declared to send one. Reaching this means the client and the store
    // have drifted apart, which a caller reading `undefined.rules` would find
    // out about as a crash three lines away rather than here, named.
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(null, { status: 204 }),
    )
    await expect(saveRules([rule()])).rejects.toThrow(
      'expected a JSON body, got an empty 204',
    )
  })

  it('lists rules with a bare GET, and hands back what the store sent', async () => {
    // Exercised nowhere else: every screen test mocks `listRules` outright, so
    // this is the only place the real request and the real parse are checked
    // at all.
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ rules: [rule()], complaints: [] }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    )
    await expect(listRules()).resolves.toEqual({
      rules: [rule()],
      complaints: [],
    })
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/rules')
  })

  it('imports by POSTing the parsed document, never the raw text', async () => {
    // The import route reads a bare array of documents — see this module's
    // own doc comment on why a document names a root by path. Sending the
    // still-JSON string instead would 400 against every route that expects a
    // body, and nothing downstream of `fetch` would say why.
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(
        new Response(
          JSON.stringify({ rules: [], complaints: [], reopened_proposals: [] }),
          { status: 200, headers: { 'content-type': 'application/json' } },
        ),
      )
    await importRules(JSON.stringify([{ ordinal: 1 }]))
    const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit]
    expect(path).toBe('/api/library/rules/import')
    expect(init.method).toBe('POST')
    expect(JSON.parse(init.body as string)).toEqual([{ ordinal: 1 }])
  })

  it('says a file that is not JSON is not a rules document, in its own words and not the parser’s, and sends nothing', () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch')
    const page = '<!doctype html>\n<html><body><h1>Rules</h1></body></html>'
    // Thrown out of the call itself: the screen's import catches it as a
    // synchronous throw (`RuleEditor.tsx`'s `sendImport`).
    expect(() => importRules(page)).toThrow(
      new Error('the file is not a rules document'),
    )
    expect(fetchMock).not.toHaveBeenCalled()
  })
})

// `aRule` spreads `blankRule(1)`, whose every optional field is `null` — not
// this file's own `rule()` above, which sets `set_edition_key: 'raw'` and so
// would make the chain return `'raw'` for cases below that need every set_*
// field empty to reach the pattern or the 'new rule' fallback.
const aRule = (over: Partial<Rule> = {}): Rule => ({
  ...blankRule(1),
  ...over,
})

describe('ruleName', () => {
  it('prefers the note, which is the only thing a builtin has', () => {
    expect(
      ruleName(
        aRule({
          kind: 'builtin-volume',
          match_on: 'name',
          pattern: '',
          note: 'volume from the name',
        }),
      ),
    ).toBe('volume from the name')
  })

  it('prefers the note over a non-empty pattern, not only over an empty one', () => {
    // The case above pins note-over-nothing (a builtin's empty pattern), which
    // cannot tell `firstNamed(note, …, pattern)` apart from
    // `firstNamed(…, pattern, note)` — with an empty pattern, note is the only
    // non-empty candidate either way. This fixture gives `pattern` something
    // to win with if `note` were not checked first, which is what actually
    // pins the argument order `firstNamed` is called with.
    expect(
      ruleName(aRule({ note: 'why this rule exists', pattern: 'Manga/.*' })),
    ).toBe('why this rule exists')
  })

  it('falls past an empty note rather than stopping on it', () => {
    // `??` cannot see `''`, and a trailing `|| 'new rule'` would catch it for
    // the *last* item only — so an empty note would skip the pattern entirely
    // and land on 'new rule'.
    expect(ruleName(aRule({ note: '', pattern: 'Manga/.*' }))).toBe('Manga/.*')
  })

  it('falls past an empty note to a set field before the pattern', () => {
    expect(
      ruleName(aRule({ note: null, set_series: 'Akira', pattern: 'x' })),
    ).toBe('Akira')
  })

  it('says "new rule" only when a rule genuinely says nothing', () => {
    expect(ruleName(blankRule(1))).toBe('new rule')
  })
})

describe('ruleMechanism', () => {
  it('names a builtin by what it sets, which its kind states outright', () => {
    expect(
      ruleMechanism(aRule({ kind: 'builtin-volume', match_on: 'name' })),
    ).toBe('builtin on name → volume')
    expect(
      ruleMechanism(aRule({ kind: 'builtin-chapter', match_on: 'name' })),
    ).toBe('builtin on name → chapter')
  })

  it('names the fields a rule sets outright, in field order', () => {
    expect(
      ruleMechanism(
        aRule({
          kind: 'regex',
          match_on: 'path',
          set_series: 'A',
          set_language: 'en',
        }),
      ),
    ).toBe('regex on path → series, language')
  })

  it('claims nothing for a regex whose effect is in its capture groups', () => {
    // Saying "→ series" here would mean parsing the pattern and
    // reimplementing the server's derive in the client — the shape O93
    // closed. The line stops at what the rule states.
    expect(
      ruleMechanism(
        aRule({
          kind: 'regex',
          match_on: 'path',
          pattern: '^(?<series>[^/]+)/',
        }),
      ),
    ).toBe('regex on path')
  })

  it('is never empty, whatever the rule holds', () => {
    expect(ruleMechanism(blankRule(1))).toBe('regex on path')
  })
})

/** A row deciding exactly `decided_by`, and nothing else about it varying. */
const deciding = (rel_path: string, decided_by: Row['decided_by']): Row => ({
  rel_path,
  root_id: 1,
  before: null,
  after: null,
  matched: true,
  decided_by,
})

describe('unsetFields — O149', () => {
  it('returns the fields no row decided, in FIELDS order', () => {
    // Two rows deciding *different* fields, so "the fields the first row
    // left unset" and "the fields some row left unset" both give a wrong
    // answer here. Expected in `FIELDS` order, which is not alphabetical
    // (`chapter` would come first), so a sort is caught too.
    expect(
      unsetFields([
        deciding('a.cbz', { title: { kind: 'rule', index: 0 } }),
        deciding('b.cbz', { series: { kind: 'rule', index: 1 } }),
      ]),
    ).toEqual(['edition_key', 'kind', 'language', 'volume', 'chapter'])
  })

  it('counts a root default as decided', () => {
    // "root default" is an answer the table draws. Only a field *nothing*
    // decided is the column of dashes O149 drops.
    expect(
      unsetFields([
        deciding('a.cbz', { edition_key: { kind: 'root_default' } }),
      ]),
    ).not.toContain('edition_key')
  })

  it('keeps FIELDS as the seven rules::Field declares, in its order', () => {
    expect(FIELDS).toEqual([
      'series',
      'edition_key',
      'kind',
      'language',
      'volume',
      'chapter',
      'title',
    ])
  })
})

describe('unsetSentence — O149', () => {
  it('lists several with "and" before the last', () => {
    expect(
      unsetSentence(['edition_key', 'kind', 'language', 'title'], 224, false),
    ).toBe(
      'edition_key, kind, language and title are not set for any of these 224 files.',
    )
  })

  it('says "is" for one field', () => {
    expect(unsetSentence(['title'], 2, false)).toBe(
      'title is not set for any of these 2 files.',
    )
  })

  it('says "this file" for one row', () => {
    expect(unsetSentence(['kind', 'title'], 1, false)).toBe(
      'kind and title are not set for this file.',
    )
  })

  it('says "the first N" when the route capped the rows', () => {
    // The sentence is only true of the rows it was handed, and 500 read as
    // "these 500 files" is easy to take for the whole library.
    expect(unsetSentence(['title'], 500, true)).toBe(
      'title is not set for any of the first 500 files.',
    )
  })
})
