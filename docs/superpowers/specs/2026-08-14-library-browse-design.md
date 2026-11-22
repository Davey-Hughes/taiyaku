# The library's browse UI — design

**Status: settled in brainstorming on 2026-08-14; amended 2026-08-20 against
the shipped A2 and B, which reopened two questions this spec had not asked;
amended again the same day against its own shipped server half (`8a99580`,
#81), which is §5.5. Server half built; UI half not built.**

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
design said it did.** It is 2,794 lines (measured 2026-08-20; 2,661 when this
was written) with a single load effect written to run once, and making it
reload for a changing work looked like the price of a single SPA. It is not:

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

**Two of the four chips read facts the shipped API does not carry**, and §5.5
adds them: `Added ▾` sorts on `SeriesCard.added`, `Unread` filters on
`SeriesCard.unread`. The other two need nothing new. `All` is the unfiltered
list, and `Multi-edition` is `editions > works` exactly rather than
approximately — a work with no edition is not counted at all, so the counts
differ if and only if some work here has a second edition.

**The `ROOTS` section is provisional and is marked as such in the design rather
than discovered later.** A root is a *storage* fact, and spec B's entire
argument is that where a file sits should stop being how you find it. Putting
roots in the primary navigation partly re-introduces what that spec removes.

Resolved rather than argued: the section is a preference, defaulting **on for an
admin and off for everyone else**, and **no navigation path may require it** —
anything reachable through `ROOTS` is reachable another way. If it turns out to
be an administrative tool it moves to C4 with the rest of the configuration, and
nothing breaks when it does. What it lists and what it filters by are §5.5's
`/api/library/status` and `?root=`, both of which intersect the caller's grant
rather than replacing it.

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

**The verdict and the paired count come from `/api/library/works/{slug}` and
from nowhere else** — §5.5's `AlignmentCard`, fetched when the panel opens.
That is what makes the `button` default cheap rather than merely tidy: no list
endpoint carries alignment detail, so a series of 111 volumes costs one query
for the list and one more for whichever volume is asked about. The `⇄` mark on
the tile needs none of it; `WorkCard.aligned` is a bool that shipped.

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

*Since 2026-11-15 (O285) `density` is `coverSize`, a number, and a number
preference its module declares a slider (`ReaderModule.sliders`) is drawn
as one by `settingsFields.tsx`'s `FieldControl`, in the View panel and the
reader's Settings alike, so this section holds whole: no control is drawn
by name; see `2026-11-14-cover-size-and-continue-row-design.md`.*

*Since 2026-11-21 (O416) the module also has `alwaysShowAligned`, a
boolean, off by default and listed by the Series page's View alone. On, it
keeps ⇄ shown on every volume's cover where it would otherwise wait for
hover.*

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

### 5.1 The root filter, and why it is not `work.root_id`

*(Added 2026-08-20. This spec shipped without asking the question; O54 and the
`#[expect]` on `Principal.roots` both name C1 as the answer, so it is asked
here.)*

A2 declined `work.root_id` for the right reason — a column written on every
scan and read by nothing is O45's exact shape, and it would have opened O45's
neighbour in the branch that opened O45. C1 makes it a *reader*, so the column
stops being dead and the question reopens. **It reopens with a different answer
than either spec expected, and the code is what forces it.**

**A work does not have a root.** `derive.rs`'s `Group` is *"every entry across
every root that resolved to the same `(series, volume)`, split by edition key"*
(`derive.rs:362`), and that is the entire project rather than an accident: one
volume held in `Manga` and in `Manga (RAW)` is **one work with two editions**.
`Candidate` carries `rel_path` beside an absolute `path` for the same reason —
*"the absolute path would sort two roots' copies of one volume by which root
they came from before by what they are called"*. A scalar `work.root_id` has no
value to hold for such a work, and picking one — the first root, or the hub's —
would be a lie that a permission filter then acts on.

So the column is **`member.root_id`**, in migration 6. Every member resolves
from exactly one `scan_entry` under exactly one root, so the value is never
ambiguous. It is also the only granularity that expresses what per-root sharing
is *for*: grant someone `Manga` and not `Manga (RAW)`, and they see the work
with one edition rather than with two or with none.

**Written by `derive`, not by `import` — the established shape here rather than
a new one.** `derive.rs`'s module doc states the rule: `work`, `edition` and
`member` rows are `import::scanned`'s alone, and what derivation writes
directly is *"exactly what the sidecar format has no field for and so cannot
carry: `work.series_id`, `work.volume` and
`member.chapter_declared`/`chapter_source`"*. A root id is one more of those.
`taiyaku.json` has no field for it and must not gain one — a scanned work has
no document at all, which is why `import::scanned` leaves `sidecar_path` NULL.
`write_chapters` (`derive.rs:1171`) is the precedent to copy, and copying it
keeps `import::scanned`'s signature answering one question rather than two.

The rule the routes apply:

- **`Principal.roots` empty — no filter at all.** Not "sees nothing".
- **Non-empty — `member.root_id IN (roots)`**, applied to *editions*: an
  edition survives if its members do, and a work is listed if it keeps at
  least one edition. NULL excludes itself, so an imported work, which sits
  under no root, needs no special case in the SQL.

**And the hub can be the edition that disappears**, which is the one case this
rule does not answer for free. `work.hub` names the edition `content_key` was
computed over and the one the reader opens; grant `Manga (RAW)` alone on a work
whose hub is the English edition and the hub is gone while the work is still
listed. Hiding the whole work instead was rejected — it withholds material the
account was granted, which is the opposite of what a grant is for. So the
reader falls back to the first surviving edition, and **`work.hub` becomes the
second field `serve` may not take at face value**, beside the dangling
alignment edge spec B already documents. The cost is stated rather than
discovered: a restricted reader's position is recorded against a work whose hub
they cannot see, and if the grant is later widened they resume in the edition
they were reading rather than the hub — which is what `progress` records
anyway, so nothing is lost, but the two readers of one work now differ in which
edition "the work" opens to.

**Empty meaning unrestricted is a migration requirement, not a preference.**
`user_root` has no writer today, so every account in every existing library
holds an empty grant. Any other reading of empty empties every library on
upgrade to migration 6 — silently, and for every user at once.

**`taiyaku library user grant-root` and `revoke-root` ship here, and that is
this spec crossing into C4 deliberately.** A2 §9 placed granting roots with
user administration in C4, and that boundary is real. It is crossed for one
reason: without a writer, this filter is a permission check that cannot be
exercised against the corpus at all, only against fixtures — and this
repository's whole argument is that unexercised code is where the defects are.
The crossing is kept as small as the argument needs: two subcommands on
`taiyaku library user`, which A2 already shipped with `add`, `promote`,
`password` and a default `list`, and which parse arguments by hand in
`taiyaku-cli/src/user.rs` in the shape `key_revoke` already has. No screen, no
C4 surface, and C4's user administration still has to be built.

**What this closes and what it does not.** O54 tracks three fields nothing acts
on; this closes `roots` and lifts the `#[expect(dead_code)]` A2 put on it
precisely so the build would fail the day C1 read it. `can_download` and
`can_stream` stay open, and stay C2's.

### 5.2 The series cover, which has no writer

`series.cover_work_id` exists in migration 1 and **nothing in the repository
writes it** — verified 2026-08-20, and this spec's §5 pointed a route at it
without noticing. So it is NULL for every series a scan has produced, which is
all of them.

The route resolves it as a fallback rather than a lookup: `cover_work_id` when
set, otherwise the lowest-`volume` work in the series, and that work's hub
edition's first member through the `cover.rs` that already generates and
content-addresses covers. **C1 writes the column at no point.**

Its writer is C3's metadata editing, which is where choosing a cover belongs.
The alternative — `derive` picking the lowest volume and storing it on every
scan — was declined because it is a scan writing a *presentation* choice, and
it would need the same don't-overwrite-what-a-person-set guard that
`work.title`, `work.reading`, `work.hub` and `edition.label` already carry,
for a value the fallback computes for free. The cost of the fallback is one
more join on the series list; §7's numbers say that is affordable, and §13
records that it has not been timed.

### 5.3 `serve --db`, without which none of this is reachable

*(Added 2026-08-20, and it is the largest thing this amendment found.)*

**The reader cannot open a scanned work, and §5's "not one existing route
changes" quietly depends on nobody noticing.** That sentence is true — and it
is about the wrong risk. The reader's routes are intact; they are also
unreachable for everything spec B produces. `taiyaku serve` is handed sidecar
paths on argv and builds its `Work`s from them one at a time in
`taiyaku-cli`'s `serve_work`; a scanned work has `sidecar_path` NULL by design,
because one `taiyaku.json` per directory cannot describe fourteen volumes
sitting beside each other. So a browse UI over a scanned library is a grid of
covers that open nothing.

This was not discovered here so much as read: `taiyaku-serve/src/lib.rs`
already says it, at the site where `from_library` was deleted as the
projection nothing called — *"a `serve --db` with no paths would want it back;
it is six lines of `SELECT id, slug, sidecar_path` and `project::sidecar`, and
it should be written then, against whatever that command decides about a work
whose `sidecar_path` is NULL."* C1 is that command, and this is that decision.

`serve --db` takes no paths and builds `ServeState.works` from every `work`
row. The six lines are genuinely six lines — `test_support::works_in` is that
loop already, written as a fixture precisely because production had no caller
for it. What is not six lines is the NULL.

**A work with no sidecar path is served from rows and corrected into rows.**
Its members already resolve: `derive` writes member paths **absolute**, and
`root.join(&member.path)` returns an absolute path unchanged, so pages need
nothing. The one route that needs a decision is `patch_alignments`, which
writes the file *before* the rows and argues that ordering deliberately — the
file is the copy a person can read, diff and back up, and row-first *"would put
the only copy of the correction in a database the reader was never told to look
in"*. With no file, the file write is skipped and the rows are written alone.

**That is the stated cost rather than a solved problem**: for a scanned work
the correction's only copy *is* in the database until somebody runs `library
export`, which is exactly what that comment declines to do for a
document-backed work — and it remains the right refusal there, because there
the file exists. The two branches are one function asking one question:
whether this work has a path. Refusing the correction outright was rejected: it
would withhold hand alignment from the whole scanned library, which after spec
B is most of it, and hand alignment is what this project is. Minting a
`taiyaku.json` on first correction was rejected more firmly — it makes the
server a writer into a tree spec B took care never to open for writing, and
gives `import`'s `sidecar_path` reconciliation a second writer to race.

**`Work.path` is what a NULL becomes**, and it has four readers: two error
messages, the bridge row's `sidecar_path` in `row_id_for_write`, and that
alignment write. The first two degrade to naming a slug; the third must not
mint a bridge row for a work that already has a real one, which it will not,
because a `serve --db` work is read from the row it would otherwise mint.

### 5.4 Unfiled works, which the grid could not show

*(Added 2026-08-20, found by running §5's queries against the real schema
rather than by reading them.)*

**`import` never writes `series_id`** — it cannot, because `taiyaku.json` has
no field for it, which is why derivation sets it directly and why spec A lists
it among the two columns an import leaves alone. So every hand-imported work
has no series, and every query in §5 that reaches works *through* `series`
cannot see one. Measured on 2026-08-20 against a fixture of two scanned works
and one imported:

| Screen | An imported work |
| --- | --- |
| `/api/library/home` | visible |
| `/api/library/works/{slug}` | visible |
| `/api/library/series` — the grid | **invisible** |
| `/api/library/search` | **invisible** |

Narrower than it first looks, and worse where it lands: the grid is what a
reader opens on. A library that is entirely `taiyaku serve x.json` imports —
which is every library this project had before spec B — shows an empty page,
while `serve --db` serves those same works perfectly well.

**So the grid carries an `unfiled` list beside its series**, and
`/api/library/series` and `/api/library/search` return the same
`{ series, unfiled }` shape. `unfiled` is `work.series_id IS NULL`, filtered by
the same roots as everything else — and a restricted account sees none of them,
because an imported member's `root_id` is NULL and NULL excludes itself, which
is §5.1's rule applying unchanged rather than a second one.

**This is also what makes search match its own description.** §5 says search is
"FTS5 over `series_title` and `work.title`", and `work_fts` has been populated
by triggers since migration 1 with nothing reading it. Searching series alone
would have left that index unread for a second release and left an imported
work unfindable by its own title.

**The alternative — having `import` mint a one-work series — was declined.** It
is the cleaner data model, and it would let collections, tags and the grid all
work through one path. What it costs is a reconciliation nobody has designed: a
later scan that files the work properly leaves the minted series orphaned, and
`derive` has no rule for a series it did not create and no longer wants. That
is a spec C3 question about provenance, not a thing to decide inside a browse
screen.

**What it does not fix**: an unfiled work still has no series page, so §3's
whole edition-and-alignment presentation is unreachable for it. That is
correct rather than a gap — an imported work is one document with its editions
already in it, and `/api/library/works/{slug}` is where those are shown.

### 5.5 Four facts the UI needs and the shipped API does not carry

*(Added 2026-08-20, after the server half merged as `8a99580` (#81). Found by
reading §2, §3 and §8 against the routes that shipped rather than against §5,
which describes them accurately and still leaves four screens unbuildable. §5's
own listing says `/api/library/series` does "sort, filter, paginate"; the
handler takes no query parameter at all, and `SeriesCard` carries no field any
of the three could act on.)*

All four are additive in the sense §5 uses: no shipped route changes its
response, no field moves, and each inherits the root filter rather than
introducing a second one.

**1. `SeriesCard` gains `added` and `unread`, and `browse::series` gains a
`user_id`.**

`added` is `MAX(w.id)` over the works the caller may see — the same recency
proxy `home`'s `added` list already uses (`w.id DESC LIMIT 20`), on a query
that already groups by series, so it costs no join. It is a proxy and says so:
a real `work.created_at` is what would replace it, and a migration for a sort
order is not worth one while every row any scan has produced sorts identically
under both.

`unread` counts the works in the series with no `progress` row for this caller
— one `LEFT JOIN` on a query that already carries four. **The chip means
`unread == works`: nothing here started.** The alternative, `unread > 0`, was
rejected on the corpus's own shape — it leaves a 14-volume series in the
`Unread` chip forever after volume 1, which is the opposite of what the chip is
for. The count is carried rather than a boolean because the series page wants
it and it is free once the join exists.

`series` gaining a `user_id` makes its signature `home`'s, which already takes
one for exactly this reason.

**The sort stays client-side; only the field is added.** §7's measured 127
series make a `?sort=` round trip a request for something the browser does
instantly on data it already holds. `?root=` is a query parameter for the
opposite reason: it changes which rows *exist*, and which rows exist is the
grant's business and therefore SQL's.

**2. `browse::work` returns alignments; no list query does.**

`AlignmentCard { from, to, verdict, slots, paired }`, read from the `alignment`
rows §3's panel wants. `verdict`, `slots` and `paired` are columns of migration
1, so this is a projection rather than a computation.

**One row per edition pair, not a scalar on the work.** A scalar `verdict` has
no meaning for a work with three editions, and picking one pair's would repeat
§5.1's `work.root_id` mistake somewhere new: a field with nothing true to hold,
which a screen then presents as fact.

**Only `/api/library/works/{slug}` carries them.** `works_of` already fires an
`editions_of` query per row it returns — the N+1 the whole-branch review caught
in `home` and closed with a SQL `LIMIT` — and an `alignments_of` per row would
reopen it, at 111 queries on the largest series. §3's panel defaults to
`button` and opens one work at a time, so the single-work route is what fills
it.

**3. `GET /api/library/status` → `{ roots: [{ id, name }], scanned }`.**

`scanned` is `EXISTS (SELECT 1 FROM dir_state)`. Read-only, with no writer and
no screen: C4 still builds roots administration, and this crosses into it the
way §5.1's `grant-root` did and for the same kind of reason — without it two of
§8's first-run states cannot be told apart by any caller, and a first-run state
nobody can render is one that ships broken to the only people who will ever see
it.

`roots` is the caller's grant resolved to names — every root when the grant is
empty, per §5.1 — which is also what §2's provisional `ROOTS` section lists. A
roots-only endpoint was weighed and does less for the same work: it answers "no
roots configured" and leaves "not yet scanned" unanswerable.

**4. `GET /api/library/series?root=N`.**

An additional filter, **intersected with the grant rather than replacing it**.
A `root` the caller does not hold answers an empty list rather than an error —
the same refusal to confirm which ids exist that made `one_series` answer 404
instead of 403. Client-side filtering was never an option to weigh: `SeriesCard`
carries no root, and giving it one would mean a work's roots, which §5.1 spends
its length explaining a work does not have.

**What is deliberately not a fifth change.**

§8's last empty state — *a work whose members are missing from disk* — needs a
per-member existence fact nothing projects. It is derivable: the scanner
**deletes** `scan_entry` rows for files it no longer meets (`forget_unseen`)
while `member` rows survive, so a member under a root with no `scan_entry` at
its path is a missing file. It is not derivable *cheaply* — `member.path` is
absolute and `scan_entry.rel_path` is relative to its root, so the join is over
a computed string rather than an index.

*This last sentence is wrong, and is kept rather than rewritten because it is
why the item sat unbuilt. The computed string is on the **probing** side; the
side being probed is `scan_entry`, whose `UNIQUE (root_id, rel_path)` is the
index the probe wants, and `member.root_id` has existed since migration 6 —
which this same spec shipped. Measured 2026-08-26 against `schema/001`–`008`:
`SEARCH se USING COVERING INDEX sqlite_autoindex_scan_entry_1`, one probe per
member. Shipped code performs that exact join for another reason
(`proposals::edition_formats`). See C3's build design §8 and O62's row.* That is a question about what the store
should record when a file vanishes, not about how a browse screen displays it,
and it goes to C3 with the rest of provenance. Until then the state renders as
the failure it already is: the work opens and its first page 404s, which is
exactly what §8 says a reader should not be shown. Recorded rather than
quietly dropped.

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
  offered and then failing at the first page. *(Not built by C1 — §5.5 says
  what it needs and why that is C3's question rather than this spec's.)*

**The first two are told apart by `/api/library/status` and by nothing else**
(§5.5). `/api/library/series` answers `{ series: [], unfiled: [] }` in both
states, which is correct of it and useless here.

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
- **The root filter, at the three cases that differ**: an empty grant sees
  everything (the migration case — every account holds one on upgrade), a grant
  hides the editions it does not name while keeping the work, and a grant that
  hides the hub still opens the work at a surviving edition. These are store
  and route tests rather than `jsdom` ones.
- **The cover fallback with `cover_work_id` NULL**, which is every series in
  every library today, and again with it set, which is the state C3 will
  produce and which nothing can produce yet.
- **`serve --db` over a derived library**: a scanned work's pages served from
  absolute member paths, and a hand correction on it writing rows with no file
  and no bridge row minted. The second is §5.3's whole decision, and the test
  that a `taiyaku.json` does *not* appear beside the work is the one that
  pins it.

## 10. What is deliberately not done

- **No proposals inbox and no metadata editing.** Spec C3.
- **No roots UI, scan control, rule editor or user administration.** Spec C4,
  with the single exception §5.1 argues for and costs: `library user
  grant-root` / `revoke-root`, two subcommands on a command A2 already
  shipped, so the root filter has a writer and can be exercised against the
  corpus rather than only against fixtures.
- **No download UI.** Spec C2.
- **No alignment started from the UI.** Project 2.
- **No virtualisation**, per §7.
- **No offline or service worker.** The reader is described as a PWA first in
  `docs/status.md` and nothing here makes it one; adding a service worker while
  authentication is one release old is a way to cache a 401.
- **No mobile-specific layout.** The grid is responsive; a phone-shaped
  navigation is not designed here.

## 11. Branch shape

**Steps 1, 4 and 5 shipped as `8a99580` (#81) on 2026-08-20** — migrations 6
and 7, `serve --db`, the six `/api/library/*` routes, the root filter and
`library user grant-root` / `revoke-root`. The ten steps keep their original
numbers rather than being renumbered, so this list and that commit still
describe the same branch.

1. ~~Migration 6: `member.root_id`, `derive` writing it, and
   `grant-root` / `revoke-root` so it has a writer.~~ **Shipped.**
2. `react-router`, the four routes, and the bundle measurement.
3. `openWork(slug)` replacing `openFirstWork`, and `<App key={work}>`.
4. ~~`serve --db`, per §5.3.~~ **Shipped.**
5. ~~The `/api/library/*` routes, with the store queries behind them.~~
   **Shipped**, and §5.5 adds four things to them.
6. The shell: sidebar, filter chips, the continue strip.
7. The series page: grid, then table, then the two attachments.
8. The `browse` module, its registry, and the generated settings section.
9. Search.
10. Empty states, and the reader's way back out.

**What is left is two plans against one branch**, split where the library
first becomes reachable end to end:

- **Plan A** — §5.5's four API changes, then steps 2, 3, 6 and 10. It ends
  with a reader who can open the app, see their library, filter it, land on
  what they were reading and get back out, and with two of §13's unknowns
  answered: what `react-router` costs the bundle, and what the grid's list
  query costs once §5.5 has added a fifth join to it. It also reads the Rust
  coverage margin, which is not a §13 unknown but is what §5.5's SQL spends.
- **Plan B** — steps 7, 8 and 9: the series page's two presentations over one
  fixture, the `browse` module and its second registry, and search, with §13's
  FTS5 tokenisation measured against real titles rather than asserted.

**The seam is honest about what it leaves half-built.** At the end of Plan A
`/series/:id` is unrouted: the grid's tiles have nowhere to go, and only the
continue strip and the `unfiled` list open anything. That is a checkpoint
rather than a release — both plans land on one branch and the branch merges
once — and it is the seam that keeps the two riskiest pieces apart: §5.5's SQL,
where a permission filter is what goes wrong, and §3's two presentations, where
divergence from each other is.

## 12. The measurements this invalidates

- `scripts/coverage-floor.mjs` and `scripts/status-test-count.mjs` — this adds a
  large number of `jsdom` tests, which moves both the web floor and the file
  count
- `web/README.md` — a second and third route into the app
- `docs/status.md` — the `web/` row of the crate table, the dependency list
  which gains `react-router`, and the test counts
- `README.md` — "To read in the browser" now describes a library, not a work
- `crates/taiyaku-library/src/schema.rs` — `SCHEMA_VERSION` 5 → 6, and the
  `const _: () = assert!(MIGRATIONS.len() == SCHEMA_VERSION as usize)` that
  fails the build if only one of the two is changed
- `crates/taiyaku-serve/src/auth.rs` — the `#[expect(dead_code)]` on
  `Principal.roots` comes off, and the doc comment calling it "the one field
  genuinely nothing reads" stops being true
- `docs/status.md` — the `taiyaku-library` row's schema description, the CLI
  command list, and O54, which closes for `roots` and stays open for
  `can_download` and `can_stream`
- `crates/taiyaku-serve/src/lib.rs` — the comment explaining why
  `from_library` was deleted, which §5.3 turns from a standing decision into a
  description of what was built instead
- `crates/taiyaku-library/src/browse.rs` — `series`'s signature gains a
  `user_id` and `SeriesCard` two fields, per §5.5; its only caller is the one
  route
- `web/package.json` and its lockfile — `react-router`, the first runtime
  dependency beyond React since the project began
- `docs/status.md` again — the route list gains `/api/library/status` and the
  `?root=` parameter
- `README.md` and `web/README.md` again — `taiyaku serve` gains an invocation
  that takes no paths at all, which is the one every scanned library uses

## 13. What could not be measured

- **What `react-router` costs this bundle.** Asserted by nobody; §11 step 2 —
  Plan A — is where it stops being unknown. *(This said "step 1" until
  2026-08-20; step 1 was migration 6 and never touched the bundle.)* If it is
  large enough to matter, the fifty-line hand-rolled router is still on the
  table and this spec's routing section is the only part that changes.
- **Whether a 127-cover grid is actually fast enough** on the deployment host
  over a real network, with covers generated on demand the first time. The cover
  cache is content-addressed and warm after one visit; the cold case is the one
  nobody has timed.
- **What the root filter costs a list query, and what the cover fallback costs
  the series grid.** Both add a join to the query behind the busiest screen,
  and §7's argument against virtualisation is a claim about the same numbers.
  **Neither has been timed, and both have now shipped** — this said "step 4 of
  §11 is where they stop being unknown", step 5 is where they were built, and
  they were built without being timed. §5.5 adds a fifth join to the same
  query, so the measurement is Plan A's rather than nobody's.
- **Whether anyone has ever held a root grant.** `user_root` has no rows in any
  library that exists, here or elsewhere, because nothing could write one until
  this spec. So the filter's non-empty branch is reachable only through the new
  CLI, and every claim about how a restricted account sees this library is a
  prediction until somebody grants a root and looks.
- **FTS5 tokenisation on Japanese.** SQLite's default tokeniser does not segment
  CJK, so 「五等分の花嫁」 may match as one token or not at all depending on how
  the query is written. `trigram` is available and untested here. §8's "name
  what was searched" empty state exists partly because of this, and the plan
  should measure it against real titles before the search box is called done.
