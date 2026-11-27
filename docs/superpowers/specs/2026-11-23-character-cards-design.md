# Character cards: a tapped name says who that is, from haiyaku

Status: approved by the owner, 2026-11-23. Branch `haiyaku-cards`
from main 01b6d0ba (#298). Rows O540–O549 are this lane's.

This is taiyaku's half of the work. haiyaku's half shipped as haiyaku 0.2.0
(haiyaku's repository, `docs/superpowers/specs/2026-11-23-haiyaku-name-lookup-design.md`):
it hands out a cast as data and finds which characters a piece of text names.

## 1. Why

The owner reads long series in the book reader and loses track of who people
are. With look-up mode on, a tap on a word already shows its dictionary entry.
A tap on a name should also say who that character is, as far as the reader
knows: nothing from the chapter being read or anything after it, and, when the
reader wants it, everything from the series' earlier books. That is the first
thing taiyaku builds on haiyaku; a Cast panel that lists everyone comes later.

haiyaku is a library in its own right (the owner, 2026-11-23): taiyaku calls it
the way any reader app could, and taiyaku draws what it hands out.

## 2. How it works today (at 01b6d0ba)

**The tap.** In look-up mode `lookUpTapped` (`web/src/BookReader.tsx`) takes
the tapped block and offset from `offsetAtPoint`. Japanese sends
`GET /api/dictionary/{lang}/at/{text}` with the text from the tapped character
(`headAt`); other languages find the word with `wordAt` (`web/src/wordAt.ts`)
and send `GET /api/dictionary/{lang}/{word}`. Only the language and the text
go to the server. The work, edition, block index and range stay in the
client's `LookingUp` state, used by Save word. `LookupPanel`
(`web/src/BookLookup.tsx`) draws the answer; `bookLookupSeat.ts` places it
at phone size.

**The dictionary routes** (`crates/taiyaku-serve/src/dictionary.rs`) open
`<dictionaries>/<lang>.db` read-only per request in `spawn_blocking`. They
are `GrantIndependent`: they say nothing about any work.

**A book.** An edition is a book when it has one member whose scan entry's
format is `epub-reflow`. The member's `sha256` is the SHA-256 of the whole
file in lowercase hex (`taiyaku_read::member_digest`). haiyaku keys a book by
exactly that (`haiyaku::store::sha256_file`), so a member's `sha256` is a
haiyaku book id as it stands. The server keeps a read cache of parsed books by
`sha256` (`crates/taiyaku-serve/src/book.rs`). Each `taiyaku_read` block
has a `BlockId` whose `document` is the spine document's path inside the
archive: the manifest href resolved against the OPF's directory, fragment
dropped, percent-decoded, dots folded. haiyaku resolves its document paths the
same way (`haiyaku`'s `book/epub.rs`, `resolve`), so for the same file the
two give the same string. They differ only on archives taiyaku cannot read
(an OPF path written with `./`, an empty href).

**The block index** the client holds is edition-wide, the index into the
book's blocks, and the saved position (`progress.slot`) is the first visible
block in the same numbering.

**The store.** `Library::open` configures one shared connection
(`Library::configure`: foreign keys, WAL, `secure_delete`, IMMEDIATE
transactions) and runs `schema::migrate`, whose version is the `meta` row
`schema_version`. `working()` and `saving()` open further connections
through `configure` without migrating.

**Reader settings** live in `localStorage` only, one key per module
(`web/src/prefs.ts`); look-up's module is `lookupModule`
(`web/src/modules/lookup.ts`, `{on: false}`).

**Routes** are listed in `routes::guarded_routes` and classified in
`grant_postures` and `narrowing_postures`; the auth tests hold the three in
step and drive a restricted account through every refusing route. A route on
book is per work: `GET /api/works/{work}/book`. It and the book's image route
check the caller with `view::of_work`, take the edition from that view, and
add `refuse_if_cannot_stream`.

## 3. Goals

- In look-up mode, a tap on a character's name shows that character's card
  above the dictionary entry.
- The card knows only what the reader has finished: the chapter holding the
  tapped paragraph and everything after it are left out.
- A reader setting, "Include earlier books in the series", on by default,
  adds the series' earlier books.
- taiyaku draws the card from data; haiyaku forces no look on it.
- A book with no character data reads exactly as it does today.

## 4. Non-goals and anti-goals

Non-goals:

- The Cast panel (everyone known so far). Later, its own design.
- Running extraction inside taiyaku, on the desktop or the NAS (haiyaku's O4).
- Any way to put character data into the deployed library. For now the owner
  fills a local database by hand.
- Choosing a series' reading order (§7, "Room for a reading-order choice").
- Joining one character across editions or languages.
- The comic reader. haiyaku reads EPUBs.

Anti-goals, which would make this a failure even if it worked:

- **A spoiler.** Anything on the card from the chapter being read or later,
  or from a book the reader has not reached in the series' order.
- **A book quoted to someone the library would not show it to.** A card
  carries evidence quotes; an account must not read, on a card, text from a
  book its grant keeps from it.
- **haiyaku able to stop the library.** Nothing haiyaku does, an unknown
  schema included, may keep taiyaku from opening its database or a book from
  opening.
- **taiyaku re-implementing haiyaku's name rules.** The look-up is haiyaku's.

## 5. Decided (owner, 2026-11-23)

- The first thing built on haiyaku is a tap on a name in look-up mode that
  shows the character's card; a Cast panel comes later.
- The code bases stay apart: haiyaku is a library callable by any reader app,
  and the caller draws the data.
- taiyaku depends on the published haiyaku crate.
- The cast cuts at the last chapter finished, for taps too, so a name first
  met in the chapter being read shows only the dictionary until that chapter
  ends.
- The card: the shown name and the other names; first met and last seen; each
  relation with its note, its quote on demand, an overturned fact marked;
  several matches listed most recent first, after any matched through its
  shown name, whole or by a part of it (settled at the prototype, 2026-11-24);
  the character above the dictionary entry. Its look is settled on a live
  prototype.
- A reader setting "Include earlier books in the series", on by default,
  decides whether earlier books count.
- One character map per edition's series for now; joining editions stays
  possible later.
- haiyaku's tables live in taiyaku's own database file, beside taiyaku's
  (asked again and confirmed this session). For now the owner fills a local
  database by hand; later, taiyaku may run extraction itself (O4), not now.
  This replaced a separate database file and a sidecar picked up by the scan,
  both proposed and dropped.
- A series with several reading orders need not be solved now, but the design
  must not make choosing one hard to add later.

## 6. Approaches considered

1. **A characters route per tap, the look-up on the server** (picked). The
   reader sends the tap to a new route on the book beside its dictionary
   request; the server folds the cast as of the tapped paragraph, runs
   haiyaku's look-up and answers with taiyaku's own JSON. The look-up stays in
   haiyaku, the route carries the book's grant check, and the dictionary
   routes stay as they are. Costs a second request per tap and a fold per tap.
2. **Characters inside the dictionary's answer.** One request, but the
   dictionary routes are keyed by language and say nothing about a work; every
   dictionary call would have to carry and check a book. Dropped.
3. **The whole cast to the browser, the look-up in the browser.** One fetch a
   chapter and instant taps, and the Cast panel wants a whole-cast answer
   anyway. But haiyaku does not build for WebAssembly (it needs SQLite), so
   the browser would need its own copy of the name rules, which is the
   anti-goal above. Dropped for the tap; the Cast panel's design will add a
   whole-cast route of its own.

I proposed approach 1 with an outline of this design on 2026-11-23. The owner
asked about the database file and the reading order, and the answers are in §5.

## 7. Design

### The dependency

`haiyaku = { version = "0.2", default-features = false }` in
`taiyaku-library`. With no features haiyaku builds its cast, fold, log,
look-up and store modules and depends on `rusqlite` 0.43 and `sha2` 0.11,
the versions taiyaku already locks, and Cargo's feature unification gives it
taiyaku's bundled SQLite. Licence (GPL-3.0-or-later), `rust-version` (1.91)
and toolchain pin (1.100.0) match. `taiyaku-core` and `taiyaku-wasm` do not
change. `taiyaku-serve` reaches haiyaku only through `taiyaku-library`.

### The tables: one database, haiyaku's own schema

`Library::open` (and `open_in_memory`) runs `haiyaku::store::migrate` on the
shared connection after `schema::migrate`. haiyaku keeps its version in its
own table, `haiyaku_schema`, and names every table `haiyaku_…`; it never
touches `meta` or `user_version`. So taiyaku's migrations take no new number
and taiyaku's `SCHEMA_VERSION` does not move.

If haiyaku's migrate fails, `SchemaTooNew` (a newer haiyaku CLI wrote the
file) or anything else, taiyaku logs a warning naming the error, marks
character cards off for the life of the `Library`, and opens as usual.
Nothing else reads haiyaku's tables while cards are off.

Before migrating, `from_connection` takes a backup when one of taiyaku's own
migrations is pending. It takes the same backup when haiyaku's migrate has
work to do (`haiyaku::store::schema_version` below haiyaku's
`SCHEMA_VERSION`), so the first start after deploying, and every later
haiyaku schema change, has a backup behind it. My call; the builder names the
file so a backup taken for haiyaku is told apart.

The tables travel with the file: a backup or copy of the database carries the
character data. Removing a book from the library leaves its haiyaku rows,
keyed by `sha256`; they do nothing until a file with that hash comes back.

### Server: the characters route

A new route beside the book's own,
`GET /api/works/{work}/{edition}/book/characters?block=…&text=…&earlier=1&before=…`,
with optional `series` and `order` (next section). It names the edition in
its path, as the book's image route does.

- `block` is the edition-wide block index of the tapped paragraph, as the
  client holds it.
- `text` is the paragraph's text from the start of the tapped word: what
  `wordAt` found for an alphabetic or Hangul word, the tapped character for
  Japanese. The client sends at most 128 UTF-16 code units of it. That is my
  call: longer than any name and short enough for a query string. From the
  selection bar's Look up, `text` is the selected text and `block` the
  block the selection starts in.
- `before` is the block's text before the tapped word, so that a tap on a
  later word of a name finds the name: at most 64 UTF-16 code units
  (`LONGEST_BEFORE`), cut by the reader to begin at a word, after white
  space, so that half a word is never tried as one. It is left out where no
  word starts in reach, and from the selection bar's Look up. In text that
  sets no space between words, Japanese among it, it is sent only where the
  block starts within reach, and is then the block's text up to the tap.

The query is refused with 400 where `block` is missing or not a number,
`text` is missing, empty or over 128 units, `before` is over 64 units, or
`earlier` is other than `0` or `1`; a `block` past the end is 400 too
(step 1 below).

The route makes the book route's checks: `view::of_work`, the edition from
that view, `refuse_if_cannot_stream`. It is `Refuses` in `grant_postures`,
driven by the auth tests like the others.

In `spawn_blocking`, the server:

1. takes the edition's member `sha256`, and the `document` of block `block`
   from the read cache's parsed book (400 for a block past the end);
2. asks `haiyaku::store::series_holding(conn, sha256)`. Its `NotFound` is the
   one answer that means haiyaku does not hold the book: the route answers
   `known: false` and stops;
3. chooses the scope (next section), from that same answer;
4. calls `haiyaku::cast_at(conn, sha256, document, scope)`, then
   `haiyaku::characters_named` from each place the look-up may start, in
   turn, until one names someone whose name reaches into `text` (below);
5. answers with taiyaku's JSON (below).

`cast_at` leaves out the chapter holding `document`, at its last page too,
so the cut is haiyaku's and taiyaku adds none of its own. `cast_at` reports
an unknown book and an unknown document alike (`NotFound`), which is why step
2 asks first: a `NotFound` from `cast_at` after it means a document haiyaku
does not hold for a book it does (a spine item it skipped), answered
`known: true` with no characters.

haiyaku reads a name from the start of the text it is handed, so the server
tries the look-up on `before` followed by `text` from several places in
turn, and the first place that names someone whose name reaches into `text`
answers: the first letter or digit of each word of `before` (a word is what
white space parts, and one with no letter or digit gives no place), earliest
first, so that a tap on a later word of a name finds the whole name and of
two names holding the tapped word the one that starts earlier answers; then
the start of `text`; then, inside the tapped word, the first letter or digit
past each run of hyphens, earliest first, so that a stuttered name is read
from its last part. A place found in `before` or past a hyphen is always a
letter or a digit: haiyaku counts what stands before a text's first word
inside the match, and the reader would light an opening quotation mark or a
dash as part of the name.

The fold runs on every tap, with no cache, on a connection of its own
(`Library::working()`), so it never holds the shared connection's lock,
which every request waits on, for longer than a page request does. The plan's
rule (its FLAG 10) decided both, on the figures `docs/notes.md` quotes under
"What the plan's FLAG 10 decided" ("Character cards from haiyaku").

### Server: choosing the scope

- `earlier=0`: `Scope::Book`.
- `earlier=1`: from `series_holding`'s answer (step 2). No series holds the
  book: `Scope::Book`. Otherwise the series is the request's `series` if it
  is among them, else the first (haiyaku lists them by name), and the order is
  the request's `order` if that series lists it, else the first it lists.
  `series_holding` lists only the orders that hold the book, by rank, so the
  first is the highest-ranked order the book is in. The order is always passed
  by name: `cast_at`'s own default (`order: None`) is the series'
  first-ranked order even when that order lacks the book, and then it fails.
  A named series or order that no longer holds the book falls back the same
  way; it is not an error.
- **The grant.** After a series-scope fold, every book in `cast.books` (the
  books the fold started) must be one the caller could open in taiyaku: a
  work the caller's view shows, with an edition whose member has that
  `sha256`, which the stream check lets through. If any is not, including a
  book the library does not hold at all, the server folds again with `Scope::Book` and the answer says
  why. My call: an account limited to some libraries must not read, on a
  card, quotes from a book outside them. For an account that sees every
  library, the only fallback is a book the library does not hold.

### Server: the answer

taiyaku's own shape, built in `taiyaku-library` from haiyaku's types (which
are `#[non_exhaustive]` and are not serialised as they stand):

```json
{
  "known": true,
  "scope": { "series": "Harry Potter", "order": "publication" },
  "narrowed": null,
  "start": -5,
  "length": 12,
  "characters": [
    {
      "name": { "text": "…", "given": "…", "surname": "…", "title": "…" },
      "aliases": [ { "text": "…" } ],
      "matched": { "text": "…", "part": "whole" },
      "first": { "book": "…", "chapter": 3, "chapterTitle": "…" },
      "last": { "book": "…", "chapter": 9, "chapterTitle": "…" },
      "relations": [
        {
          "with": { "text": "…" },
          "facts": [
            { "kind": "…", "note": "…", "quote": "…",
              "first": {}, "last": {},
              "overturned": { "note": "…", "quote": "…", "at": {} } }
          ]
        }
      ]
    }
  ]
}
```

- `scope` names the series and order folded, or is null for this book alone;
  `narrowed` says why a series fold fell back (`"not-visible"`, naming no
  book), or is null.
- `start` is where the matched name begins, in UTF-16 code units from the
  start of `text`: below 0 where it begins in `before`, above 0 where it
  begins inside the tapped word, as past a stutter; not sent when 0.
- `length` is the matched name's length in UTF-16 code units, counted from
  `start`, so the reader can light the whole name; absent when nothing
  matched.
- `characters` are haiyaku's matches: first those matched through their
  shown name, whole or by a part of it, then those matched only through
  another of their names, each group most recently seen first, in haiyaku's
  order. A tap on a title + surname that is one character's shown name and
  another's other name so lists the first before the second, but only while
  haiyaku shows the first under that name: once the books give the first a
  fuller name, the title + surname is another name of both, and the more
  recently seen leads again.
- Each relation is one with the matched character on either side; `with` is
  the other character's shown name. Every field of haiyaku's `Fact` and
  `Overturn` is carried in taiyaku's words; the prototype decides which the
  card shows.
- Book titles and chapter titles come from haiyaku's stored book and
  chapters, so a card reads the same whatever taiyaku calls the work.

The example's values are placeholders; the field names are fixed once, in the
web types (`web/src/book.ts`) and the Rust struct together.

### Web: the request, the card, the setting

- When the reader opens a book it learns whether that book has character data
  (cards on and haiyaku holds its `sha256`). The builder puts this on the
  book's own answer unless that answer is cached in a way a later fill would
  not reach, in which case a small route answers it.
- In look-up mode, a tap on a book with character data sends the characters
  request beside the dictionary request. A book without sends nothing new.
- `LookupPanel` draws each character's card above the dictionary entry. My
  call: nothing already drawn in the panel moves when the cards arrive, so on
  a book with character data the panel holds its first draw until both answers
  are in or the dictionary's has waited a short time the prototype sets. A
  characters answer later than that, or one that fails, is dropped for that
  tap: the dictionary shows alone, and the failure is logged. A tap that
  matches no one shows the dictionary as today.
- `lookupModule` gains `includeEarlierBooks: true`. The settings panel is
  generated and labels a field from its key (`humanize` in
  `web/src/settingsPanel.ts`), so it reads "include earlier books". My call:
  name the key for the label rather than widen the module contract with
  labels. The request sends it as `earlier`.
- The selection bar's Look up opens the same panel, so it sends the same
  request with the selected text. My call: the panel shows cards whenever it
  opens on a book with character data.
- The prototype settles the card's look, where the quote opens, how an
  overturned fact is marked, whether the lit text widens to the whole name,
  and how the card sits in the phone seat (O369 and O370 already describe
  its limits).
- While a card shows, the reader marks the other mentions of the first
  card's character in the blocks on the screen and a screen either way (asked
  for at the prototype, 2026-11-24). The rule is the reader's own, over that
  card's names (`web/src/bookMentions.ts`): haiyaku answers who a text names
  from its start, not which character each mention on a page is. It keeps to
  the names that are that character's alone, since a mark on someone else is
  worse than a mention missed, so it misses some mentions and marks some
  words wrongly; `docs/notes.md`, "The marks", lists those cases, and O543
  and O548 hold them open.

### Room for a reading-order choice

Nothing here chooses an order; three things leave room for it:

- the request already takes `series` and `order`;
- the answer names the series and order it used, so a picker can show the
  current choice;
- the setting lives in `lookupModule`, where a per-series map of chosen
  orders can sit beside `includeEarlierBooks` without moving it. The
  generated settings panel cannot draw a per-series picker, so that choice
  will need a control of its own, probably on the card or the series page.

A stale choice (a series renamed, an order removed) falls back to the default
instead of failing. The choice itself is O540.

### Filling the database, for now

A short how-to in `docs/notes.md`: back up `library.db`, stop the server,
then run haiyaku's CLI against the same file with `--db`: `extract` on the
library's own EPUB (the hash must be the file the library holds), `series`,
`link`. Use a haiyaku CLI whose schema the deployed haiyaku dependency knows;
a newer one switches cards off (above) until taiyaku's dependency catches up.
Whether the CLI can safely write while the server runs is the builder's to
check and write down; until then the how-to says to stop the server.

### What the NAS sees

Merging deploys main to the NAS. At its next start the live library is
backed up, then gains haiyaku's empty tables, and the reader gains the
setting; no book has character data there, so no card shows and no
characters request is sent (O541).

### Files touched (expected)

- `crates/taiyaku-library`: `Cargo.toml`; `src/lib.rs` (migrate at open,
  the backup, the cards-off flag); `src/backup.rs` if the backup's name
  needs it; a new `src/characters.rs` (scope choice, grant
  check hook, the answer type).
- `crates/taiyaku-serve`: a new `src/characters.rs` (the route);
  `src/routes.rs` (`guarded_routes`, `grant_postures`,
  `narrowing_postures`); `src/book.rs` (the read cache's document for a
  block, the has-character-data flag).
- `web/src`: `book.ts`, `BookReader.tsx`, `BookLookup.tsx`,
  `modules/lookup.ts`, styles.
- `docs/status.md` (what is built, O540–O542), `docs/notes.md` (the fold
  measurement, the how-to, the prototype's rounds).
- `Cargo.lock`; `deny.toml` only if cargo-deny asks.

## 8. Verification

Fixtures are invented: the test books are copyrighted. A test builds a small
EPUB, hashes it, and imports an invented haiyaku sidecar for that hash with
`haiyaku::store::import_book`, which haiyaku builds without its `extract`
feature.

- **Store:** opening creates haiyaku's tables at
  `haiyaku::store::SCHEMA_VERSION`; reopening changes nothing; a
  `haiyaku_schema` row above the known version leaves the library open, its
  own tables working and cards off; a pending haiyaku migration alone takes
  a backup first.
- **The cut:** a character first met in chapter 2 is absent from a tap in
  chapter 2, including at its last block, and present from chapter 3; a
  relation and its quote from chapter 2 likewise.
- **Scope:** `earlier=0` folds the book alone; `earlier=1` folds the earlier
  book; a named order the series lacks falls back to the first-ranked; a book
  in no series folds alone.
- **The grant:** the route refuses a restricted account outside the grant
  (the auth tests' table); a series fold whose earlier book the account cannot
  see narrows to the book and says so.
- **Text:** `length` is right in UTF-16 for a Hangul name with a particle and
  for text holding a character outside the BMP.
- **Answers:** an unknown book is `known: false`; a block past the end is 400.
- **Web:** the request carries the block, the word's text and `earlier`; the
  selection bar's Look up sends the selected text; a book without character
  data sends no characters request; the cards draw above the dictionary
  entry; a late or failed characters answer leaves the dictionary alone and
  moves nothing; the setting defaults on and its change reaches the next
  request.
- Each test is watched failing against the defect it names, by hand, before
  it counts.
- **Real material:** the owner tries it on the prototype server (§9) in
  English and Korean.

## 9. The prototype

The look is the owner's to settle by trying it. A prototype server serves a
scratch copy of the test library with its Books root, following the project's
recipe. haiyaku data for the Harry Potter books comes from haiyaku's CLI run
against that scratch copy; the test library's Harry Potter EPUBs are
byte-identical to the ones haiyaku has already extracted, so the CLI's reply
cache should make this free. A cache miss would cost model calls, and the
controller asks the owner before running any. The scratch copy holds book
quotes and stays in the session scratchpad, deleted when the rounds end. The
approved look lands as its own commit, then tests hold it.

## 10. Rows this PR opens

- **O540:** which series and reading order count as "earlier books" when a
  book has several: the first series by name and its first-ranked order today.
- **O541:** the deployed library has no way to receive character data; it
  waits on haiyaku's O4 (extraction inside taiyaku) or an import path.
- **O542:** the Cast panel, everyone known so far, not built.

Further rows found while building take O543–O549.

## 11. Left to the builder

- The route's path, matching the book route's neighbours.
- The connection for the fold, and whether a cache is needed, from the
  measurement in §7.
- Whether the has-character-data flag rides the book's answer or a small route.
- Chapter titles. haiyaku's public `store::read_book` gives a book's title
  and chapter titles, but reads the book's whole log, too much per tap. Read
  them once per book and keep them while the book's haiyaku row is unchanged,
  or show chapter numbers and file a haiyaku item for a light read. Never
  read `haiyaku_chapter` directly.
- Whether the client's `Block.text` for index n is the server's block n's
  text, which the `length` field relies on.
- Whether haiyaku's CLI can write `library.db` while the server runs (lease,
  busy timeout).
- Any taiyaku test that lists the database's tables and now meets haiyaku's.

## 12. My calls in this document

1. Character cards are off, not fatal, when haiyaku's migrate fails.
2. A series fold narrows to the book when the caller cannot open every book
   it folded, including a book the library does not hold.
3. The client sends at most 128 UTF-16 code units of text.
4. No fold cache until a measurement asks for one.
5. The panel holds its first draw until both answers are in, so nothing
   moves; the wait is set on the prototype.
6. The first series by name and its first-ranked order, with a named but
   stale choice falling back to them.
7. Book and chapter titles on the card come from haiyaku.
8. A removed book's haiyaku rows stay.
9. The filling how-to goes in `docs/notes.md` and says to stop the server.
10. The selection bar's Look up asks for characters too.
11. A characters answer that comes after the wait, or fails, is dropped for
    that tap.
12. A backup before haiyaku's migrate when it has work to do, as before
    taiyaku's own.
13. The setting's key is `includeEarlierBooks`, so the generated panel reads
    "include earlier books"; no labels added to the module contract.
