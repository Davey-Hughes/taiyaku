# The loading work, second PR: the Library view, series and search on the store, covers the browser keeps, and the scroll on Back

2026-10-28. The `docs/status.md` Later entry "The loading work, in two PRs"
asks for it; the first PR's spec is `2026-10-27-loading-reads-design.md`,
whose store (`reads.ts`) and page hook (`usePage`) this builds on. It closes
O224.

## 1. What changes for the reader

Today the Library view, a series page and search read their data on every
visit into the component's own state, so a return draws an empty grid until
both lists land, and then every cover is a new image that asks the network
before it paints: a series cover is a `307` sent `private, no-store` to a
thumbnail sent `private, no-cache`. Nothing keeps the scroll, the chip, the
sort or the chosen edition. After this:

- **Back to the Library view, a series page or a search** draws the rows
  left behind in its first render, with covers from the browser's cache and
  no cover requests, at the scroll position, chip, sort and edition left
  behind. It then reads again quietly and updates in one step if anything
  changed, such as the Continue strip after reading.
- **The reader's Library button** opens the last Library view seen in this
  tab (its library, chip and sort) at the scroll it was left at.
- **The sidebar's Libraries heading** opens the defaults at the top, as
  today. Its link for each library switches library and keeps the chip and
  the sort. A link to the view already shown, a library's own link or,
  over the defaults, the heading or a wordmark (the column's or the phone
  bar's, each a link to `/` as the heading is), lands at the top too and
  adds no history entry (§10).
- **A first visit, or a switch to a library not seen in this tab**, draws
  nothing of the old library, and one "Loading…" line where the grid goes
  once the wait has held 200 ms, as on the Configuration pages.
- **A change known to alter what these views draw** (a scan's end, a rules
  save, a library's removal, a directory's removal or move, a work's title
  edit; corrected while building, §6.4) marks them stale, so the next
  arrival waits rather than draw what the app knows is wrong. Reading
  progress marks nothing; the re-read after reading waits for the reader's
  last progress save to answer.
- **Not restored:** a series page's open volume panel, which follows hover
  and focus in one of its modes. A rotation or resize between leaving and
  returning can land the scroll on different rows.

## 2. The user's rulings (2026-10-28)

1. **Covers get versioned addresses** that the browser keeps for good (§4),
   over a short freshness on the `307` and the thumbnail, and over a
   versioned target behind a cacheable `307`.
2. **The chip, the sort and a series page's edition live in the URL** (§5.1),
   over the history entry's state and over memory for the tab.
3. **After reading, draw, then update** (§1): the return draws the kept rows
   and the quiet re-read moves them once, over marking them stale so the
   return waits. The re-read waits for the last progress save (§3.3).
4. **The reader's Library button returns to where the reader was** (§5.2),
   over going Back only when the reader was opened from the grid, and over
   leaving it.
5. **The store and a scroll keeper**, over making the page scroll at every
   width for the browser to restore, and over keeping the Library view
   mounted but hidden.
6. §1 and §3 to §9 as presented, section by section.
7. *Corrected while planning (2026-10-28)*, from the plan's reading of the
   code: the answers carry each cover's address, built by the server, in
   place of the parts the client would build it from (§4.2, §4.4), and
   there is no "no hash" case, since `member.sha256` is `NOT NULL`; the
   sidebar has a Libraries heading and a link per library, not a Library
   link, and those links are pushes that drop every parameter today (§1,
   §5.1); the reader's three exits that share `onLeave` all return to where
   the reader was (§5.2); `Licence.tsx` puts the browser's restoration back
   to `auto` whatever it found (§5.2); and four closed rows the checks and
   docs answer to: O1, O133, O221 and O223 (§5.2, §6.2, §7, §8).
   *Corrected while building (2026-10-28), by the user's ruling:* the
   keeper puts back `auto` as `Licence.tsx` does, and `Licence.tsx` is
   unchanged (§5.2).

## 3. The store, `web/src/reads.ts`: three additions

1. **A bound.** The store keeps at most 50 entries that nothing is drawing;
   past that it drops the one drawn least recently. An entry something is
   drawing is never dropped. A dropped entry costs only this: the next
   arrival at its address is a first visit. A count rather than a byte
   budget, because a count is simpler to reason about and to test, and the
   largest answer, a large library's series list, is under a megabyte of
   JSON.
2. **Invalidating by prefix.** The views' addresses carry ids and queries
   (`/api/library/series?library=2`, `/api/library/series/17`,
   `/api/library/search?q=…`), so a write cannot name them one by one.
   `invalidate` gains a form taking path prefixes, which covers every kept
   address under each: the address equal to the prefix, or continuing it
   with `?` or `/`. It does what `invalidate` does (§3.6 of the first
   spec): an address something draws is read again at once, and the rest
   stay stale until asked for.
3. **The views' reads wait for the last progress save.** `putProgress`
   keeps its request in flight where the read functions can see it, and the
   reads of the grid, home, a series and a search wait for it to settle
   before they send. A save that fails settles too; one that hangs holds
   the re-read, while what is kept stays drawn, and a first visit's read as
   well (§10).

The new addresses are factories in `pageReads.ts`, as `libraryScan(id)` is:
one each for the grid and home of a library, a series, and a search. Each
calls the existing client function in `library.ts`, so the tests'
`vi.mock('../src/library')` keeps working. A work's lazy detail stays in
the series page's own state: it is read on a hover or a press, not as part
of the page's body.

## 4. Covers the browser keeps

1. **A cover version.** A thumbnail's `ETag` is made from its member's
   `sha256`, the page's offset in that member, `PAGE_FORMAT_VERSION` and
   the long edge (`routes.rs`'s `page_etag`, `cover.rs`'s `page_key`). The
   version is the same inputs without the edge, as one opaque string the
   client only hands back, so it changes exactly when the `ETag` of any of
   the cover sizes would.
2. **The answers carry each cover's address, with its version in it.**
   *Corrected while planning (2026-10-28):* this item had the answers carry
   a cover's work, edition and version for the client to build the address
   from; a series row already carries `cover` as an address, so the server
   builds every one.
   - A series row's `cover` becomes the thumbnail address of the edition its
     cover is taken from, `/api/works/{work}/{edition}/pages/0/thumb?v=…`,
     or null when the series has no visible cover. The cover is chosen as
     `browse.rs`'s `cover_work` chooses it for the `307` (a hand-set cover
     if visible, else by `w.volume`, then `w.id`, then `e.key`), through one
     SQL fragment both use, so the list and the `307` cannot disagree for
     the same access. The grid, search and a series page's own row all
     carry it. *Settled while building (2026-10-28), by the user's ruling:*
     a grid narrowed by `?library=` chooses through the narrowed access, a
     volume in that library, so a hand-set cover from another library does
     not show there, and a narrowed tile can differ from its series page's
     header and from the `307`.
   - Each edition card gains `cover`, its page 0's address with the version.
     A series page's volume tiles and home's Continue cards draw it, so the
     client no longer builds a cover address.
   - The version comes from the `member` rows: page 0 is offset 0 of the
     edition's first member by `ordinal` with `pages > 0`, as
     `Edition::locate` finds it, and `member.sha256` is `NOT NULL`. That
     member is not filtered by the caller's grants, since `serve_page`
     locates page 0 in the whole edition. The served registry refreshes
     from those rows every 2 s, so a list read in that window after a scan
     can carry a version the route does not yet serve, which is item 3's
     mismatch.
3. **The thumbnail route** honours `?v=` only for a cover: page 0 at a long
   edge in `COVER_LONG_EDGES` (256, 512, 768).
   - **A version matching what the route serves**: `200` or `304` with
     `Cache-Control: private, max-age=31536000, immutable`, and the `ETag`
     as now.
   - **A version that does not match** (the image changed after the list
     was read): the current image with today's `private, no-cache`, so no
     address is kept for good with bytes other than the ones it names.
   - **No version**: as today, for the reader's page strip and every other
     caller.
4. **The client.** `coverSources` adds `w` to the answer's address, with
   the same 1x, 2x and 3x `srcSet`, for series tiles, volume tiles and
   Continue cards; a null series cover draws the missing-cover box with no
   request. `workCover` goes. The series cover `307` route stays as it is;
   the web client no longer uses it.
   *Since 2026-11-15 (O285) the `srcSet` describes the three sizes by
   width, and a cover's `sizes` gives the width it is drawn at, or on a
   phone a bound on it; see O133's row in `docs/status.md`.*
5. **What it does not change.** `private` keeps shared caches out. Covers
   already stay in the browser's cache after signing out, under `no-cache`.
   A revoked grant drops the series from the next list read, so its cover's
   address is not asked for again. The version names the member's `sha256`,
   which the thumbnail's `ETag` already sends. A download-only account sees
   covers as O133 made it, at every cover size.

## 5. The view's state in the URL, and the scroll keeper

1. **The URL.**
   - The Library view's chip and sort are `?show=multi|unread` and
     `?sort=added`; All and Title, the defaults, are left out.
   - A series page's edition is `?edition=<key>`; "all" is left out. With
     the edition selector switched off it reads as all, and the address
     keeps it (§10).
   - Each change is written with `replace`, as the search box writes `?q=`,
     so it adds no history entry, and keeps the other parameters. A library
     switch keeps `show` and `sort`, as the state does today: the sidebar's
     link for each library is a push that drops every parameter, so drawn
     beside the Library view it carries that view's `show` and `sort`.
   - An unknown value reads as the default, and so does an edition the
     series does not have, which keeps today's rule that a leftover edition
     never selects a real one in another series.
2. **The scroll keeper**, one module and a hook the Library view and a
   series page use.
   - **The scroller** is the element that scrolls at the width drawn:
     `<main>` at 900 px and wider, the document below. It is read from the
     layout, not from a hard-coded width.
   - **Saving.** The position is saved for each history entry, by
     react-router's `location.key`, as the reader scrolls (once per
     animation frame at most) and on leaving. It is held in memory for the
     tab, so a reload starts at the top.
   - **Restoring.** An arrival by Back or Forward restores the saved
     position once the body is drawn: in the first render when the answers
     are kept, or when the body lands after a wait. It restores once per
     arrival; the quiet re-read's update does not move the scroll again.
     Tiles are sized by `aspect-ratio`, so the height is right before any
     image loads. The position is clamped to what is there.
   - **The Library button.** The keeper also holds the address and the
     scroll of the last Library view drawn in this tab, the whole address,
     `?q=` included. The reader's three exits share `Routes.tsx`'s
     `onLeave`: the top bar's Library button, its key, and the failed
     load's "Back to the library". Each opens that address with a marker in
     the navigation's state, and the view restores the held scroll as it
     would on Back. With none held, they open `/` at the top. The marker
     is a key of its own in that state, so the search box, which reads
     only its tag, takes the arrival as a change it did not make and draws
     the `?q=` it opens (O223).
   - **Every other arrival lands at the top**: as today, the sidebar's
     heading, a link within the app and a library switch, and now a
     sidebar link or a wordmark to the view already shown, which kept the
     scroll before (§10). So does a replace that draws the view, as the
     `/search` redirect does. A replace while the view is drawn, which a
     chip, the sort, an edition or a key typed into the box writes, leaves
     the scroll where it is. A sidebar link or a wordmark to the view
     already shown is such a replace, but for §10's third known limit, so
     it adds no history entry, and it carries a mark, `SCROLL_TO_TOP`,
     that lands it at the top.
   - **The browser's own restoration** is turned off while these views are
     drawn, so it cannot fight the keeper, and set back to `auto` when they
     go, as `Licence.tsx` does for its own page. *Corrected while building
     (2026-10-28), by the user's ruling:* this item had the keeper put back
     what it found, and `Licence.tsx` changed to do the same. In Chromium
     the setting is the history entry's, and an entry a push makes starts
     with the setting of the one it is made from, so putting back a
     `manual` found on arrival left it on the next entry pushed, and Back
     to that entry restored nothing at a phone width. The plan checks that
     every other page keeps its Back behaviour.

## 6. The screens

1. **The Library view** reads its library's grid and home through
   `usePage`.
   - A first visit draws its body in one step, as today, and now "Loading…"
     in the grid's place after 200 ms.
   - A switch to a library kept in this tab draws it at once; to one not
     kept, nothing of the old library while it loads (the two "draws
     nothing of the library it left" tests in `web/tests/Library.test.tsx`
     keep pinning this).
   - The search box still waits for the sidebar's status before it is
     offered, and the status still comes through `StatusContext`.
   - The retry after a failure becomes an `invalidate`. *Settled while
     building:* the view had no retry control, so this has nothing to
     change (§10).
2. **Search** (`/?q=`) reads its query's hits through the store.
   - An arrival with `?q=` (Back, a link, a reload) draws kept hits at once,
     and with none kept, reads at once rather than after the 250 ms typing
     delay, once the box is offered: where the view cannot search, no query
     is sent (O221). Until then it draws "Loading…" in the hits' place
     after 200 ms (§10).
   - Typing keeps the 250 ms delay, and the last query's hits stay drawn
     until the next one's answer lands, as today, rather than giving way to
     "Loading…" on each key. A typed query that fails with nothing kept for
     it draws the alert alone, without the last query's hits (§10). This
     is the one place a view bends `usePage`'s first-visit rule, and its
     hook's doc says so.
3. **A series page** reads its series through `usePage`. Its header draws at
   once, as today. A move to another series no longer keeps the old one
   drawn while the next loads: it draws as a library switch does.
4. **Writes invalidate by prefix** every view address they change: a work's
   title edit on a series page (the page drawing it reads again at once; the
   grid, home and search are marked), a scan's end (a library's poll, a
   directory's, and a directory page's watch of another directory's scan),
   a rules save, a library's removal, and a directory's removal or move.
   Signing in and out already empty the store. The plan lists each call
   site, as the first PR's did. *Corrected while building (2026-10-28), by
   the user's rulings:* this item had an accept, a library's creation and
   rename, and an edit to the caller's own account's grants mark the views
   too, and left out the directory page's watch. The first three change
   nothing these views draw, and the grant edit's premise, that the
   caller's own grants change what this tab is shown, does not hold: only
   an administrator can save a grant, and an administrator sees every
   library.

## 7. Checks

- **Rust.** The thumbnail's headers for a matching, a mismatched and an
  absent `v`, on the `200` and on the `304`, and for a size that is not a
  cover; a download-only account's covers with `v` (O133). A cover address
  taken from a list answer, over a scanned library with real hashes, brings
  back the immutable header. The list's cover choice equals the `307`'s over
  fixtures with a hand-set cover, one hidden by a grant, a lower volume
  added, and several editions; a series with no visible cover has a null
  one. An edition whose first member has no pages takes its version from
  the next. The new SQL also runs in `rust:1-bookworm` (SQLite 3.40).
- **Web, jsdom.** The bound (the 51st idle entry drops the one drawn least
  recently; a drawn entry is never dropped); prefix invalidation; the
  progress wait; a revisit drawing kept rows with the same cover `src` in
  its first render; the URL state both ways; search's arrival without the
  delay and its hits kept while typing; a series switch; each write's
  invalidation.
- **Web, browser, with real scrolling**, at a wide and a phone width: Back
  restores the Library view and a series page; the Library button restores;
  the sidebar's link lands at the top; a body drawn after a wait restores;
  the quiet re-read's update does not move the scroll.
- **Every guard a test names is hand-mutated** and watched failing.
- **On the real thing.** A live server on a copy of the test library; then
  the iPhone, whose user agent is read first: Back to the grid with no cover
  request in the server's log, which is O224's closing evidence.
- The full gate.

## 8. The docs

- `docs/status.md`: the Later entry says what the second PR shipped,
  naming reading progress as a second exception to "each write marks stale
  what it changes"; O224 closes with the five-part edit; the entry's
  sentence on the web tests' move gains "(#211)".
- `docs/notes.md` gains a section on the cover version, beside O133's
  (covers sized per screen): what the version is made of, the route's three
  answers to `?v=`, the answers that carry it, and why it answers O1's
  reason for declining `immutable` (the address now names the bytes). The
  deploy notes' list of the guarded answers' `Cache-Control` values gains
  the new one.

## 9. Not in this

- A work's lazy detail on the store.
- Restoring a series page's open volume panel.
- HTTP caching of the JSON lists, a service worker, and keeping answers
  across a reload.
- Scroll anchoring that survives a rotation.
- Watching for a scan's end from the Library view: a scan that ends while no
  Configuration page watches it is picked up by the next arrival's quiet
  re-read.
- The pre-existing items found on #211 (the Rust coverage file count, O148's
  `declared`, O152's "six callers").

## 10. Settled while building, and known limits

**The user's rulings while building.** Where one corrects an earlier
section, that section says so too.

1. **A narrowed grid's cover** (2026-10-28): a grid narrowed by
   `?library=` takes its cover through the narrowed access, a volume in
   that library, so a hand-set cover from another library does not show
   there, and a narrowed tile can differ from its series page's header
   (§4.2).
2. **The writes that mark the views** (2026-10-28): an accept, a library's
   creation and rename, and an edit to the caller's own grants mark
   nothing; the end of another directory's scan, which a directory's page
   watches, marks them (§1, §6.4).
3. **A hidden edition selector** (2026-10-28): with the selector switched
   off, `?edition=` reads as all, so no volume is dimmed by an edition no
   control on the page names. The address keeps it, and switching the
   selector back on restores it (`Series.tsx`'s `chosen`).
4. **Search** (2026-10-28): a typed query that fails with nothing kept for
   it draws the alert alone, without the last query's hits. An arrival at
   `?q=` before the box is offered draws the page's "Loading…", after
   200 ms, in the hits' place while it waits, and still sends no query
   until the box is offered (O221).
5. **The browser's restoration is set back to `auto`** (2026-10-28), and
   `Licence.tsx` is unchanged (§5.2).
6. **A cover that fails takes the room of one that loads** (2026-10-28), a
   defect from before this work found while building the keeper, fixed
   here: a cover image was inline, so its tile carried the line's descender
   and a failed cover's placeholder drew a shorter tile, moving a scroll
   restored below it. The grid's, a series page's and the Continue strip's
   cover images are blocks now (`library.css`).
7. **A sidebar link to the view already shown lands at the top**
   (2026-10-29): a library's own link, and on the defaults the Libraries
   heading. The live check found a library's own link kept the scroll, a
   case §1 and §5.2 had not settled.

**Settled without a ruling:** §6.1's retry has nothing to change, since
the Library view had no retry control. A click on a link to the view
already shown adds no history entry, the controller's default, told to
the user, but for the third known limit below. Both wordmarks, the
column's and the phone bar's, land at the top of `/` already drawn too:
the review of the fixes found they kept the scroll, and the controller
read ruling 7 as covering them, since the user's ruling of 2026-10-26
makes the column's wordmark a link to `/`, where the heading goes, and
the phone bar's is the same link.

**Known limits:**

1. **A progress save that hangs holds a first visit's "Loading…" too.**
   Each of the four reads (`pageReads.ts`'s `gridRead`, `homeRead`,
   `seriesRead` and `searchRead`) waits for the saves out before it sends,
   whether or not anything is kept, so a first visit after a save that does
   not answer draws "Loading…" for as long as the save is out (§3.3).
2. **After heavy image use, Back can draw the grid before its covers**
   (O230, to be investigated). In the live check in Chromium, Back drew the
   grid at its kept scroll with most of its covers not yet complete, and
   all of them a moment later; they came from the browser's HTTP cache
   with no request to the server, so its in-memory image cache had dropped
   them. The user's iPhone asked for no cover or thumbnail address twice.
   The figures are in `docs/notes.md`, "The cover version".
3. **An address typed or bookmarked in another form makes the link to it
   a push.** A press of the chip or the sort puts the address's parameters
   in the order of their names, the defaults left out (`library.ts`'s
   `withView`), the form the sidebar's links are written in, so a link to
   the view a press drew is a replace. An address typed or bookmarked with
   its parameters in another order, or with a default written out, is not
   in that form: its own link, or over the defaults the heading or a
   wordmark, is a push, which lands at the top but adds a history entry,
   until the first press rewrites the address.
