# The library's browse UI — design

**Status: settled in brainstorming on 2026-08-14. Not built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is **spec C1** of the roadmap in `2026-08-14-library-store-design.md`, §10.
It depends on **A** (the store), **A2** (the guard) and **B** (something to
browse). It is the screen that makes the rest of it usable, and it was designed
against live mockups rather than described — those are kept in
`.superpowers/brainstorm/`, gitignored, and the decisions they settled are
recorded here.

Spec C was four UIs and is now three, split by how often each is used. **C3** is
curation — the proposals inbox and metadata editing, done constantly and from
inside the library. **C4** is configuration — roots, scan control, spec B's rule
editor with its per-field provenance display, and A2's user administration, all
touched rarely and all covered by the CLI in the meantime. This spec is browse,
search, and the reader finally getting an entry point.

## 1. Routing, and the refactor that turned out not to be one

`react-router` enters as **the first runtime dependency beyond React since the
project began** — the list today is `react`, `react-dom`, `taiyaku-wasm` and
nothing else. Chosen over a hand-rolled router of about fifty lines over
`popstate`, deliberately: scroll restoration, nested routes and navigation
blocking are all things this UI wants and all things that are tedious to get
right twice. **The plan measures what it costs the bundle**, because this
repository states measured figures and "it is small" is not one.

```
/                 Library — the grid
/series/:id       one series
/search           results
/read/:work       the reader
```

**`App.tsx` does not need to become re-entrant, and an earlier draft of this
design said it did.** It is 2,661 lines with a single load effect written to run
once, and making it reload for a changing work looked like the price of a single
SPA. It is not:

```tsx
<Route path="/read/:work" element={<App key={work} work={work} />} />
```

A changing `key` unmounts and remounts, so **App.tsx's "loads exactly once"
assumption stays true rather than becoming a lie maintained by hand.** Verified
before it was relied on: the file has exactly one module-level binding
(`EMPTY_SLOTS`, an immutable empty `Set`), and its load effect already carries a
`cancelled` flag. The only edit inside it is `openFirstWork(PORTS)` →
`openWork(PORTS, work)`.

Two properties make the remount cheap. The wasm module is a module-level ES
import, so it initialises once per **page**, not per mount — a remount costs a
sidecar fetch and a resolver construction, not a wasm boot. And what should
survive navigation already does, in `localStorage` via `prefs.ts`, while what
should reset — scroll, decode cache, undo history — correctly does.

## 2. The shell: a sidebar, and the part of it that is provisional

Chosen over shelf rows and a dense grid. Persistent navigation, with the grid as
a surface you filter rather than a page you scroll.

```
┌──────────┬────────────────────────────────┐
│ Library  │  [All 127] [Multi-edition 12]  │
│ Continue │  [Unread]  [Added ▾]           │
│ Recent   │  ▦ ▦ ▦ ▦ ▦ ▦ ▦                 │
│          │  ▦ ▦ ▦ ▦ ▦ ▦ ▦                 │
│ ROOTS    │  ▦ ▦ ▦ ▦ ▦ ▦ ▦                 │
│ Manga    │                                │
│ …        │                                │
│ LISTS    │                                │
└──────────┴────────────────────────────────┘
```

**The `ROOTS` section is provisional and is marked as such in the design rather
than discovered later.** A root is a *storage* fact, and spec B's entire
argument is that where a file sits should stop being how you find it. Putting
roots in the primary navigation partly re-introduces what that spec removes.

Resolved rather than argued: the section is a preference, defaulting **on for an
admin and off for everyone else**, and **no navigation path may require it** —
anything reachable through `ROOTS` is reachable another way. If it turns out to
be an administrative tool it moves to C4 with the rest of the configuration, and
nothing breaks when it does.

**A slim continue-reading strip sits at the top of the Library view**, and
`Continue` is also a view of its own. This is the one thing the mockups did not
settle and it is a judgement rather than a finding: the sidebar layout's cost
against shelf rows is that you no longer land on what you were doing, and a
strip buys that back for one row of vertical space. It is a preference
(`continueStrip`), so a reader who disagrees turns it off.

## 3. The series page: three switches, not three layouts

The mockups offered three series-page designs and the answer was "all three,
per-user". **They are not three layouts.** They are three positions of three
switches, which is what makes the answer affordable:

| Preset | `layout` | `editionSelector` | `panel` |
| --- | --- | --- | --- |
| A — edition as a mode | `grid` | on | `button` |
| B — editions per volume | `table` | off | `off` |
| C — grid with a panel | `grid` | off | `on-select` |

So the build is **two components — a grid and a table — plus two optional
attachments**, not three pages.

**Defaults are preset A**: `grid`, selector on, panel on `button`. The panel as
a button rather than on select was the owner's correction to mockup C, and it is
the right one for this library: with about 90% of series single-edition, a panel
that opens on every selection is mostly empty, while a small `i` on the cover
costs nothing until it is wanted.

**What the page must show that a Komga series page cannot.** Go-Toubun is 14
volumes in `Manga`, 14 in `Manga (RAW)`, a fan chapter release — and **only
volumes 1, 3 and 8 are actually aligned**. A series page that lists volumes and
editions without saying which are aligned is a Komga screen with extra badges.
So: an alignment mark on the cover (`⇄ 3`), the verdict and paired count in the
panel, and in `table` layout a dashed badge for an edition that has no page for
this volume.

## 4. Settings are generated, and that is not a hope

`settingsPanel.ts:45` derives a control from a preference's **runtime type** —
boolean to checkbox, number to numeric input, string to text, and a string whose
module declares `options` to a select. `ReaderModule<P>` carries `defaults` and
`options?: Record<string, string[]>` for exactly that (`module.ts`).

So a `browse` module needs no settings UI of its own:

```ts
defaults: { layout: 'grid', editionSelector: true, panel: 'button',
            density: 'comfortable', continueStrip: true, showRoots: false }
options:  { layout: ['grid', 'table'], panel: ['off', 'button', 'on-select'],
            density: ['compact', 'comfortable'] }
```

**A second registry, not an entry in the existing one.** `modules/registry.ts`
is *"every reading behaviour this build ships"*, built at module scope so
`buildRegistry`'s collision check fires on import. A browse module is not a
reading behaviour, and its keys live in a different collision domain — `/` for
search on a browse screen has nothing to argue with a reader key about, and
folding them together would forbid that for no benefit. `buildRegistry` and
`settingsPanel.ts` are reused verbatim; only the list is separate.

**The honest cost.** The reader's module system works because each module is a
small *behaviour*. This is the first one that owns a whole *screen*, and two
presentations of one dataset are genuinely two components to keep in step — the
tests double for anything that differs between them. Recorded because the reuse
argument above is real and does not make that cost disappear.

## 5. The API — additive, changing nothing

```
GET /api/library/home              continue reading · recently added · recently read
GET /api/library/series            sort, filter, paginate
GET /api/library/series/{id}       the series, its works, per-work edition and alignment state
GET /api/library/works/{slug}      one work: editions, page counts, verdict, note count
GET /api/library/search?q=         FTS5 over series_title and work.title
GET /api/library/series/{id}/cover
```

**Not one existing route changes.** The reader's sidecar, page, progress, notes
and alignment routes are untouched, so C1 cannot break reading. A series cover
is `series.cover_work_id` pointed at an edition cover that `cover.rs` already
generates and content-addresses.

Search is one endpoint, queried as you type through `debounce.ts`, which exists
and is already tested. FTS5 over `series_title` is what makes 「五等分の花嫁」,
the romaji and the English title find the same shelf — the payoff for a table
that migration 1 carries for this.

Every route sits behind A2's guard, and a 401 routes to the login screen rather
than throwing — `/api` deliberately answers no `WWW-Authenticate`, per that
spec's §4.

## 6. Getting back out of the reader

`Chrome.tsx` gains a control, and the key that does it goes **through
`keys.ts`'s one ordered table** — `Help` is generated from that table and from
the registry, so a binding added anywhere else would be a key the help screen
does not know about, which is the drift `help` exists to prevent.

## 7. Scale, stated rather than assumed

No virtualisation. The numbers this library actually produces are small: 127
series on the home grid, 111 volumes for the largest series, both measured on
2026-08-14. Covers get `loading="lazy"`, which is free.

**The number that would change this is not the series count.** It is a
chapter-organised edition — `Sousou no Frieren - Fan Translated` is 146 files,
and a future library of several thousand series is plausible. Virtualisation is
deferred until a real library produces a figure that needs it, and the figure
should be produced rather than imagined.

## 8. Empty and broken states

Each is a screen someone will see on their first run, so none of them is a
`null` return:

- **No roots configured** — the state every new deployment starts in. Points at
  adding one, and says what a scan will do.
- **Roots configured, scan not yet run** — distinct from the above, and saying
  so is the difference between "you have no library" and "your library has not
  been read yet."
- **A series with no cover** — a generated placeholder carrying the title, not a
  broken image.
- **Search with no results** — names what was searched, since FTS5 tokenisation
  on CJK is a place where a query can be surprising.
- **A work whose members are missing from disk** — the store knows the `sha256`
  and the scanner knows the file is gone. Shown as unreadable rather than
  offered and then failing at the first page.

## 9. Testing, and an advantage this screen has that the reader does not

**Browse components are `jsdom`-testable and `App.tsx` is not.** That file
value-imports `resolver.ts`, which imports `taiyaku-wasm`, whose package is
generated into a gitignored directory — so importing `App.tsx` from a test
passes on a machine that has run `build-wasm.sh` and breaks CI and every fresh
clone. Nothing in browse touches the resolver. It is ordinary components over an
HTTP API, so the whole of it lands in the `jsdom` project that CI already runs,
rather than in the `browser` project that CI deliberately cannot.

That is a real difference in what the gate can see, and it means the coverage
this spec adds is coverage CI enforces.

- The two presentations tested against **one** fixture, asserting they present
  the same facts — the failure mode of a two-component design is divergence, and
  a shared fixture is what catches it.
- Alignment state rendered for the case that matters: a series where some
  volumes are aligned and some are not.
- The `key={work}` remount asserted to reset reader state, since §1's whole
  argument rests on it.
- 401 routes to login rather than rendering an empty grid.
- Every empty state in §8 rendered, since each is a first-run experience.

## 10. What is deliberately not done

- **No proposals inbox and no metadata editing.** Spec C3.
- **No roots, scan control, rule editor or user administration.** Spec C4.
- **No download UI.** Spec C2.
- **No alignment started from the UI.** Project 2.
- **No virtualisation**, per §7.
- **No offline or service worker.** The reader is described as a PWA first in
  `docs/status.md` and nothing here makes it one; adding a service worker while
  authentication is one release old is a way to cache a 401.
- **No mobile-specific layout.** The grid is responsive; a phone-shaped
  navigation is not designed here.

## 11. Branch shape

1. `react-router`, the four routes, and the bundle measurement.
2. `openWork(slug)` replacing `openFirstWork`, and `<App key={work}>`.
3. The `/api/library/*` routes, with the store queries behind them.
4. The shell: sidebar, filter chips, the continue strip.
5. The series page: grid, then table, then the two attachments.
6. The `browse` module, its registry, and the generated settings section.
7. Search.
8. Empty states, and the reader's way back out.

## 12. The measurements this invalidates

- `scripts/coverage-floor.mjs` and `scripts/status-test-count.mjs` — this adds a
  large number of `jsdom` tests, which moves both the web floor and the file
  count
- `web/README.md` — a second and third route into the app
- `docs/status.md` — the `web/` row of the crate table, the dependency list
  which gains `react-router`, and the test counts
- `README.md` — "To read in the browser" now describes a library, not a work

## 13. What could not be measured

- **What `react-router` costs this bundle.** Asserted by nobody; §11 step 1 is
  where it stops being unknown. If it is large enough to matter, the fifty-line
  hand-rolled router is still on the table and this spec's routing section is
  the only part that changes.
- **Whether a 127-cover grid is actually fast enough** on the deployment host
  over a real network, with covers generated on demand the first time. The cover
  cache is content-addressed and warm after one visit; the cold case is the one
  nobody has timed.
- **FTS5 tokenisation on Japanese.** SQLite's default tokeniser does not segment
  CJK, so 「五等分の花嫁」 may match as one token or not at all depending on how
  the query is written. `trigram` is available and untested here. §8's "name
  what was searched" empty state exists partly because of this, and the plan
  should measure it against real titles before the search box is called done.
