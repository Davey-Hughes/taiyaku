# The loading work, first PR: a read store and one page hook

2026-10-27. The `docs/status.md` Later entry "The loading work, in two PRs"
asks for it; this spec is the first of the two. The second (the
Library view, series, search, cacheable covers, scroll on Back, O224) gets a
spec of its own.

## 1. What changes for the reader

Today each Configuration page, `/account` and About reads its data on every
visit, each part on its own, and draws each part as it lands. After this:

- **A first visit** draws the page's header at once and its body in one step,
  when every read the page needs has answered or failed. A wait longer than 200 ms
  draws one "Loading…" line where the body will go.
- **A revisit** draws the last answers at once, reads them again quietly, and
  updates in one step, or not at all when nothing changed (but see §9, known
  limits 1 and 3).
- **What is kept** lives in this tab's memory: a reload or a new tab starts
  empty, and signing out or in empties it.

## 2. The user's rulings (2026-10-26 and 2026-10-27)

1. The loading work is two PRs of its own, after the build label's PR
   (2026-10-26, recorded in status.md's Later entry).
2. Our own store, not a data library such as TanStack Query: the one-render
   rules are this project's, and owning the layer keeps their timing ours
   (2026-10-26).
3. First visit, revisit and lifetime as in §1 (2026-10-26).
4. §3 to §6 as presented, section by section (2026-10-27), with two picks:
   a stale answer is not drawn on arrival (§4.3), and the sidebar's library
   status moves onto the store in this PR (§5); and the corrections made
   to this spec while planning (`3a36a1ad`).
5. At the look (2026-10-27): one Loading… per pane at pair width; §9's
   calls 2 and 7, and its known limits 1, 3 and 6.
6. After the look (2026-10-27): a return to a library's page is not to
   draw a scan status from before a directory's own scan. The user had
   accepted that limit at the look as then worded, for phone width, and
   ruled it fixed once review found it at any width. The directory's page
   marks that status stale and tells no reader of it, so a wide window
   draws as before (§9's call 9).

## 3. The store, `web/src/reads.ts`

1. **One entry per read address**, the path and query a `GET` is sent to,
   such as `/api/library/libraries`. An entry keeps the last good answer, its
   text, whether it is stale, and the request in flight.
2. **One request per address.** A component that starts reading an address
   while a request for it is out joins that request, with three
   exceptions. A poll's read (`readNow`) always starts its own request and
   applies it alone. The other two exist because the request out may
   predate the change that made the entry stale: `invalidate` (item 6)
   never joins one, but starts a new one for an address something draws
   and drops the one out for the rest; and a request out at a `markStale`
   (item 12) is joined by no later read.
3. **Newest wins.** An answer is kept only when it comes from the newest
   request started for its address; a later answer to an older request is
   dropped.
4. **Unchanged means untouched.** An answer whose text equals the kept text
   leaves the kept object as it was, so nothing drawing it redraws.
5. **A failure keeps the kept answer.** The failure is recorded beside it,
   and the next good answer clears it.
6. **`invalidate(...addresses)`** marks each address stale. Every address
   that something is drawing is read again at once, together, as in item 8;
   one that nothing draws stays stale until something asks for it.
7. **The store calls each address's client function** (`listLibraries`,
   `listKeys`, …). Each already reads through `api.ts`'s `request`, so a 401
   reaches the gate as it does today, and each read fails with the sentence
   it throws today. Two answers are the same when their `JSON.stringify`
   texts are equal. *Corrected while planning (2026-10-27):* this item said
   the store reads through `request` itself, which would have gone around the
   `vi.mock` stubs of the client modules that every screen's tests but
   `/account`'s use, and changed `/account`'s failure sentences.
8. **A set of reads started together lands together.** When the last of them
   settles, every answer of the set is applied and the store tells its
   listeners once, so every component drawing any of them updates in the same
   render. Newest-wins still applies to each address of the set.
9. **Lifetime.** In memory only. Signing in and signing out empty it, and an
   answer to a request started before the emptying is dropped.
10. **Components subscribe per address** through `useSyncExternalStore`.
11. **Tests** reset the store between cases through an export the tests use,
    so one case's answers never reach the next.
12. **`markStale(...addresses)`** marks each kept entry stale and tells no
    listener, so nothing is drawn again and nothing is read; an address with
    nothing kept gets no entry. A request out at the mark is dropped then
    if nothing draws its address; otherwise it still answers the page
    drawing it, unless that page has gone when it lands. An entry such a
    request settled is marked again as the last page drawing its address
    goes. §9's call 9 is its one use.

## 4. The page hook, `usePage`

A page names every read address it needs. The hook answers whether the body
can be drawn, the answers, whether "Loading…" is drawn, and the failure.

1. **First visit.** The page draws its header at once, from what it knows
   without a read: an account's or a library's page names itself from the
   URL. The hook says the body can be drawn only once every address has
   settled, answered or failed (item 6). A wait that has held for `WAIT_DRAWN_AFTER_MS` (200 ms)
   draws one "Loading…" line in the body's place, through `useDrawnWait`; an
   answer within it draws none. The line has no minimum time on screen.
2. **Revisit.** With every address answered and none stale, the body is
   drawn from the kept answers in the first render. Every address is then
   read again as one set (§3.8), so the page moves at most once (but see
   §9, known limits 1 and 3).
3. **A stale answer on arrival counts as missing**: the page waits as on a
   first visit, so it never draws what the app already knows is wrong. A page
   already drawing an answer that goes stale keeps drawing it until the
   re-read lands.
4. **Some answered, some not.** The page waits as on a first visit, and the
   answered addresses are read again in the same set as the missing ones.
5. **A read that needs another's answer** (a library's scan status needs the
   library's id from the libraries list) is named once its address is known;
   until then the hook holds the body back as if it were unanswered. The
   answers that have landed are available for working out such an address,
   not for drawing the body.
6. **Failure.** The hook holds the body until every read has settled and
   hands the page each failure, and the page draws each one as it does
   today. A page whose body is the list that failed (Libraries, Users,
   Backup, Rules, About) draws the header and its alert and no body. A page
   that draws a part's failure beside the rest still does: an account's
   page's grant editor, `/account`'s catalog address, and a library's page's
   scan status, whose failure is silent. With answers drawn, a failed re-read
   keeps them and the page draws the failure as it does today. *Corrected
   while planning (2026-10-27):* this item said a failed read on a first
   visit draws no body, which would have hidden `/account`'s password form
   behind a failed key list.

## 5. The screens

| Screen | Reads it waits for | What changes |
| --- | --- | --- |
| Libraries | the libraries list | a revisit draws at once |
| A library's page | the libraries list, then the scan status it draws (§4.5) | the scan status no longer lands after the rest |
| Users | the users list, the libraries list | the libraries list is shared with Libraries |
| An account's page | the users list, the libraries list | **defect:** "The libraries list has not been read, so there is nothing to offer…" is no longer drawn while that list is being read |
| Backup | the backups list | the list is no longer inserted above "How backups work" |
| Rules | the rules, the roots, the accepted proposals | **defect:** its empty states are no longer drawn before its reads land, and the preview pane holds its wait from the first render until the first preview answers, as the comment in `PreviewTabs.tsx` above "No preview yet" says it does. A re-read that changes the rules after the body is drawn replaces the editor's rules only while nothing is edited |
| `/account`, and the reader's account panel (the same component) | keys, sessions, `/api/about` | one step: today each list starts empty and draws its rows as it lands, so an unread list looks like an empty one. A failed list is still drawn as nothing, as today |
| About, and the sidebar's build label | `/api/about` | shared with `/account`; `about.ts`'s module-level memo and `forgetAbout` go |
| Licence | no read: the texts are a code chunk | the header at once and "Loading…" after 200 ms, as §4.1; the chunk once loaded is kept, so a revisit draws at once |

- **The sidebar's library status** (`/api/library/status`) is read through
  the store. `StatusRefreshContext` and `useStatusRefresh` go, and the saves
  that called it invalidate the address instead. `StatusContext` stays as
  how the Shell hands the answer down, so the components that read it are
  untouched until the second PR.
- **Each write names the addresses it changes** and invalidates them after
  it succeeds, in place of each screen's own re-read and sequence counter.
  The plan lists them write by write.
- **A library's scan polling** keeps its interval and reads through the
  store. Each answer is drawn as it lands: that is progress, not a page
  moving.
- **Not on the store:** the directory browser, read on demand per path; the
  Rules preview, which is a send, not a read; a directory row's own
  mount read (`RootRow.tsx`), which draws a directory scan already running
  and stays outside the page's wait (§9, known limit 10); and a directory's
  page's read of its library's scan status, which holds its controls while
  another directory scans (`DirPage.tsx`'s `useOtherScan`, §9).

## 6. Checks

- **The store** (`reads.test.ts`, a fake `fetch` whose promises the test
  resolves): two subscribers to one address send one request; an older
  request answering last is dropped; the same text again keeps the same
  object and calls no listener; a failure keeps the kept answer; `invalidate`
  re-reads an address being drawn and leaves one not drawn stale; signing in
  or out empties it and drops an answer already in flight; a set of reads is
  applied with one notification.
- **The hook** (jsdom, fake timers): a first visit draws the header alone
  until 199 ms, "Loading…" at 200 and the whole body in one render; a revisit
  draws the body in its first render, never draws "Loading…", sends one
  re-read per address and updates at most once; a stale answer on arrival
  waits; both failure cases. A count of updates is taken per element, not by
  a whole-page `toContain`.
- **Each defect's test fails on `main` first:** Rules never draws "No preview
  yet" before its first preview; an account's page never says "has not been
  read" while the libraries list is being read; `/account` draws no key row
  while the sessions list is unread, and no session row while the keys list
  is.
- **Hand mutations**, each turning a named test red, recorded in the PR body:
  newest-wins removed; unchanged-means-untouched removed; a stale answer
  drawn on arrival; a set's answers applied one by one.
- **The full gate** (`check-all.sh`, the per-file coverage floors included),
  and **the user's look** on a live server with the network throttled, first
  visits and revisits on each screen of §5.

## 7. The docs

Each says what is true after this:

- `docs/status.md`: the Later entry's first PR becomes what shipped, and the
  second stays in Later. The rows that describe a list read again on each
  change of page (O200, O209 and O222, by the survey) are read and updated
  where this changes what they say.
- `docs/status.md`, the About entry: the sentence on the build label's PR
  gains its number and date (#207, merged 2026-10-27).
- The doc comments of the screens in §5 that describe their own first read
  or sequence counter.

## 8. Not in this

- The Library view, series, search, covers and scroll on Back: the second PR.
- HTTP caching, a service worker, and keeping answers across a reload.
- Retries, and reading again on focus or when the network returns.
- A bound on what the store keeps: the addresses here are a handful. The
  second PR, whose addresses grow with the library, decides whether it needs
  one.

## 9. Settled while building, and known limits

**The controller's calls**, made while building it, each but 3, which
changes no code, with the commit that holds it. The user accepted 2 and 7
at the look (§2.5) and chose 9 after it (§2.6):

1. **A failure kept from an earlier visit is not drawn on arrival.**
   `usePage` hands a page a read's failure only once a read of that address
   by this mount has settled, so such a failure is neither drawn nor brought
   into view before this visit's read settles, while an answer kept beside
   it is drawn (`d92511b3`).
2. **A scan a return finds over draws no report.** A return to a library's
   page whose kept status said a scan was running, and whose first read of
   the status finds it over, draws the page with no scan running and no
   reports, and reads no list, as `RootRow`'s mount read adopts only a
   running scan. A scan this visit saw running, or one a press started,
   still draws its reports when it ends (`e3154c33`), and a first read that
   answers in the kept status's own words counts as seeing it running
   (`e2c01192`), unless the page's own poll overtook that read
   (`168a8797`).
3. **A directory's page reads its library's scan status off the store**
   (`DirPage.tsx`'s `useOtherScan`, which this branch leaves as it is),
   added to §5's "Not on the store".
4. **An account's page keeps "The libraries list has not been read…" for a
   read that failed**; it is no longer drawn while the list is being read
   (`009f6a0c`).
5. **Rules says "The rules were not loaded: …" only for a visit that cannot
   draw its body.** A re-read that fails after a save, or on a return, keeps
   what is drawn and draws no alert (`104a452f`). The alert gives the
   reason of a read with no answer kept (`e2c01192`).
6. **A write marks stale what it changes beyond its own screen.** Every
   directory write in Libraries marks the roots stale, and a change of page
   there marks none of them; an accept marks the rules and the accepted
   proposals; a removal marks the roots, the rules and the accepted
   proposals, which it and its cascades delete or change, and for a
   library's removal, or a move that empties one, the users list too
   (`104a452f`, `d595d0e1`). A library's
   rename marks the users list, whose grants name it; each write of
   `/account` and the reader's account panel marks the users list, whose row
   for the caller carries their keys and sessions; a revocation on the
   caller's own account's page marks the keys or the sessions; Scan library
   started, or answered 409, marks the library's scan status, which a return
   before the first poll would otherwise draw as no scan; and a Rules save,
   which derives the library again, marks the libraries list, whose works
   counts it can change (`ec3bbc4c`).
7. **The build label can name a redeployed server's build after a later
   About or `/account` visit**, since it reads `/api/about` through the
   store with them: `about.ts`'s held answer was a cache, and went with
   `forgetAbout` (`ef8a91e9`).
8. **`usePage` takes an empty list**, typed `readonly []`, for a page that
   reads nothing for now, as a closed account panel does (`c5c76445`).
9. **A directory's own scan marks its library's scan status stale.**
   `RootRow` marks it, through the `onScanMoved` that `DirPage` passes as
   `reads.ts`'s `markStale`, when Scan directory is pressed; again when the
   start is answered or refused with 409, for a read that was out while the
   start was; when its mount read takes up a running scan; when its poll
   finds the scan over; and when it unmounts with the scan running. So a
   return to the library's page waits for its read rather than draw a
   status from before the scan or its end.
   `markStale` tells no reader, so the library's page drawn beside the
   directory's in a wide window draws what it drew before: it does not
   take the scan up, hold Scan library and Remove…, or draw the directory's
   report a second time beside `RootRow`'s own. A request already out at
   the mark still answers a page drawing the address, and the entry is
   marked again as the last page drawing it goes, so the next arrival
   waits for a read of its own (`82ccc298`, `6b44bf90`, `33c5c179`). A
   start refused otherwise leaves the status marked, so the next return
   waits one round trip for a status that did not change.

**Known limits**, listed again in the PR body. The user accepted 1, 3 and
6 at the look (§2.5) and 10 with the planning corrections (§2.4); the
controller accepted the others while building:

1. **A revisit whose set joins a request another set started can move
   twice**: the joined read lands when the set that started it does
   (`usePage.ts`'s doc).
2. **Arriving at Libraries, the sidebar's status lands apart from the
   list.** The screen reads it beside `usePage`'s read of the list, not in
   that set, which would hold the body for the sidebar's answer; after a
   write or a change of page the two land as one set (`d92511b3`).
3. **A return to a library's page reads the list and the scan status as two
   sets**, so the page can move twice (`1aba6ad1`).
4. **A failed re-read clears the stale mark and keeps the old answer**
   (`reads.ts`'s `apply`), so after a write or a mark (call 9) whose
   re-read failed, a later return draws the answer from before the write
   or the mark until its own re-read lands; and a return whose own read of
   a marked address fails draws the answer from before the mark until that
   visit reads the address again, as a press of Scan library does.
5. **Rules can keep a list older than the server's for the rest of a
   visit.** A stored list that lands while a write is out, newer than the
   write because another writer came between, is marked seen by the write's
   answer; where the write's own re-read answers the same text, nothing is
   drawn again (`d595d0e1`).
6. **A failed first visit to a page whose body is the list that failed
   draws the header and the failure only** (§4.6): Backup without "How
   backups work", Rules without its editor.
7. **A directory's address loaded cold draws its library's page first.**
   The address holds only the directory's id, so until the list names the
   library the screen draws the library's page, named from the address,
   with no directory's page beside it (`Libraries.tsx`'s
   `dirOpen && library !== undefined`); at phone width the directory's page
   then replaces it once the list lands. A cold load has no focus to keep.
8. **A Rules save makes Users wait once on a return**, though nothing Users
   draws can change: its grant editor reads the libraries list, which the
   save marks (call 6), and the store keeps one entry per address. The wait
   is one round trip, with Loading… only after 200 ms.
9. **A change from elsewhere to the rule open in Rules' form redraws the
   form** (`RuleEditor.tsx`'s `formMade` key), dropping keys typed into it
   and not yet committed by a blur or Enter. Each write in the app that
   changes the rules marks them, so this takes another writer: the CLI,
   another tab or another device.
10. **A directory's page draws a scan already running only once its row's
    own mount read lands** (`RootRow.tsx`), outside the page's wait (§5).
11. **A page arriving at an address while another page draws an answer
    that a request out at a mark brought draws that answer at once**,
    though it is from before the mark (call 9): the entry is marked again
    only as the last page drawing it goes. No address marked today has two
    pages drawing it.
12. **An address on both the old and the new list of a page's reads is
    read once more when the list changes** if it holds an answer that a
    request out at a mark brought, since for that moment nothing draws it.
    A library's page changes its list only as the library's id becomes
    known on arrival.
