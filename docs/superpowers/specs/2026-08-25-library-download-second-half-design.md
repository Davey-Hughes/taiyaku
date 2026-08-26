# The library's download, second half: the OPDS acquisition route and the browse UI's control

An addendum to `2026-08-23-library-download-design.md`. That document's §8 (the
UI) and §9 (OPDS) were designed and deliberately not built — spec C2 shipped six
of its nine branch-shape steps as PR #85 and recorded the other three as
deferred rather than implied, in `docs/status.md`'s `taiyaku-serve` entry and in
`opds.rs::work_feed`'s own doc comment.

**This document does not restate §7, §8 or §9.** They stand as written and are
the source; this addendum carries only what building them turned up that they do
not answer, plus the four decisions that answering it required. Where the two
disagree, this document wins and says so explicitly.

## 1. What is actually being built

Three changes, and nothing else:

```
GET /opds/v1.2/works/{work}/{edition}/download    a third registration of an existing handler
```

- the acquisition link in `work_feed`'s entries, repointed from a page to that
  route, and the doc comment above it rewritten;
- a download control on the series page, in both the grid and the table layout.

**The route needs no new handler.** `download::edition` takes its path
parameters positionally — `AxPath((slug, edition_key)): AxPath<(String, String)>`
(`download.rs:458`) — so `{work}`/`{edition}` in the OPDS namespace bind exactly
as `{work}`/`{edition}` do under `/api`. §2's "one handler, two registrations" is
literally true, and `opds::routes()` (`opds.rs:507`) is where the second
registration goes, beside `stream_page`'s, which exists for the same reason.

There is no member download in the OPDS namespace (§2, unchanged).

## 2. Four decisions §8 and §9 did not make

### 2.1 The table layout has no "shown" edition, so the control goes in the cells

§8 says "one control per volume tile, downloading the edition the tile is already
showing", and then, of the other layout: "The `table` layout gets the same
control as a column."

**That sentence does not survive the table's actual shape.** `Series.tsx:71`'s
`Table` renders one column *per edition key* — `editionColumns(works)` — with
each cell holding that edition's page count, or an em-dash with
`aria-label={`no ${c.label} edition`}` where the volume does not have it. There
is no `shown` variable in this component and no edition a row is "already
showing"; a row shows all of them at once. A single extra column would have to
pick one edition arbitrarily — `editions[0]`, the hub — inside the one view whose
entire purpose is comparing editions side by side.

**So: each cell's page count becomes the download anchor for that (volume,
edition) pair.** No new column. A cell that holds `—` stays `—`, because there is
nothing to download.

This contradicts §8's "a volume with three editions gets one control rather than
three", and the contradiction is resolved in favour of the table because §8's
argument does not transfer. That argument is about the *tile*: a tile shows one
cover, for one edition, chosen by a selector the page already has, so a reader
asking "which edition am I getting" is already answered and three controls would
be three answers to a settled question. A table row has no such selector and no
settled question — every cell is a different edition, already labelled by its own
column header, and the page count in it is already the per-edition fact. Making
that number the link adds no new element to the layout at all.

### 2.2 The acquisition link's `type` must be the type the route will send

§9 gives the new link `type="application/vnd.comicbook+zip"` flatly. **That is
wrong for a single-member edition whose member is not a comic archive**, and §7
of the same document is what makes it wrong: a passthrough download's type
follows the bytes, not the route, because `member.format` admits `pdf`,
`epub-fixed` and `epub-reflow` beside `zip` and `dir`. `download.rs::type_for`
maps the member's extension to `application/pdf`, `application/epub+zip`,
`application/vnd.comicbook-rar` or `application/octet-stream`, and §7 states the
reason plainly: serving a PDF as `comicbook+zip` under a `.cbz` name "would be a
lie told twice". A feed that advertises the same lie in a third place is the
same defect, and an OPDS client matches its acquisition link's `type` to decide
whether it can open the entry at all.

**The feed asks the route's own decision rather than restating it.** A helper in
`download.rs` — the module that owns `type_for`, `CBZ_TYPE` and the
`paths.len() == 1 && paths[0].is_file()` test — answers "what type would a
download of this edition send", and both `prepare` and `work_feed` call it. Not
a copy of the rule in `opds.rs`: this project has already paid once for a
visibility rule spelled out in three places, and §2.1 of the source document
names reusing `refuse_if_hidden` as what "stops the download becoming the third
place that spells the rule out". The same argument applies to the type.

The cost is stated rather than hidden: this puts a filesystem `metadata` call per
edition into a feed handler that currently touches no disk. It is bounded — one
work, its editions, metadata only, no archive opened — and it is the price of
the feed and the route being unable to drift. **A helper that cannot answer
falls back to `CBZ_TYPE` rather than failing the feed**: a missing file must not
turn one bad edition into a 500 for the whole work, and the entry is no worse
than today's, which points at a page that would also 404.

### 2.3 A reader who may not download keeps the page link

§8's rule for the web UI is "absent entirely when `can_download` is false", and
the reasoning — a disabled control invites a reader to find out why, and nothing
on that screen can tell them — is right and is kept.

**The feed cannot do the same thing.** `work_feed`'s existing doc comment records
why the acquisition link points where it does: OPDS 1.2 requires at least one
`rel="http://opds-spec.org/acquisition"` link on every entry in an acquisition
feed, "or a conformant client may render the entry as unopenable". Removing the
link for a `can_download == false` reader would produce exactly the malformed
feed that constraint exists to prevent.

So the page link is not deleted; **it becomes the fallback**. An entry carries
the download link when the principal may download and the page link when it may
not. Both are valid acquisition links, the conformance requirement is met either
way, and a restricted reader gets today's behaviour rather than a link that
403s on every attempt.

`work_feed` already reads `Principal` out of extensions, but only inside
`Some(principal) if !principal.roots.is_empty()` — the grant filter's guard. The
principal must be read once, unconditionally, and both facts taken from it.

### 2.4 The tile's anchor is a sibling of the `<Link>`, not a child

The grid tile wraps its cover and title in `<Link to={`/read/${w.slug}`}>`
(`Series.tsx:404–423`). An `<a download>` nested inside another anchor is invalid
HTML and browsers reparent it, so the control goes **after** `</Link>`, in the
same `<li>`, beside the existing `panel === 'button'` toggle — which is already
there and already a sibling, so the pattern needs no invention.

`download` is written bare, with no value. The server sends
`Content-Disposition: attachment` with both an ASCII `filename=` and an RFC 6266
`filename*=UTF-8''…` (§7); a value here would override the careful one with a
guess made in the browser.

The flag comes from `useGate()` (`gate.ts:46`), which returns `Gated { who: Me,
signOut }`, and `Me.can_download` is typed at `api.ts:105`. Nothing new is
fetched: `Gate` asks `me()` once per page load and the context has held the
answer since A2.

## 3. What the enumeration forces

`routes::guarded_routes` folds `crate::opds::routes()` into its own list, and
`auth::tests::every_route_declares_its_grant_posture` asserts that
`routes::grant_postures` is **set-equal** to it. A new registration with no
posture row does not fail a review; it fails the build. That is the mechanism the
previous branch built and it is why this one is cheap.

- The new route's posture is **`Refuses`**, the same as
  `/api/works/{work}/{edition}/download`, because it is the same handler behind
  the same `refuse_if_hidden`.
- It is a **driven** entry, not a declared one: every `Refuses` row is proven by
  a real request from a restricted account, against both a wholly-hidden work
  and a work split across roots.

Every counted figure that describes this table therefore moves, and each appears
in prose that was itself re-counted on the last branch rather than grepped:

| | before | after |
| --- | --- | --- |
| guarded routes | 28 | 29 |
| `Refuses` | 7 | 8 |
| driven / declared | 18 / 10 | 19 / 10 |

`docs/status.md`'s `taiyaku-serve` entry carries all four numbers in one
sentence. **Count the table; do not trust the numbers above.** They have drifted
three times already, twice within a single branch, and this row is written from
reading the code rather than from running the test.

## 4. Two refusals, unchanged

§2.1's order stands and is not re-derived here: `refuse_if_hidden` answers 404
first, `can_download` answers 403 second. The OPDS registration inherits both by
being the same handler.

One consequence is worth naming because it is easy to read the wrong way: an
anonymous request to the new route answers **401 with
`WWW-Authenticate: Basic realm="taiyaku"`**, where the `/api` twin answers a bare
401. That asymmetry is the entire reason the OPDS mirror exists (§2), and it is
what a test should assert about this route specifically rather than assuming it
follows from registration.

## 5. O55 is a prerequisite, and it was found by writing this document

**O55 is open and nobody had connected it to C2's purpose.** It says no OPDS
client has ever been shown to accept a `tyk_` API key in the HTTP Basic password
field. The server half is verified with `curl`; the client half never was, and
the account screen promises it regardless.

C2's stated purpose is offline reading in Panels. **If Panels will not
authenticate, repointing the acquisition link proves nothing** — the feed is
unreachable before the link is ever read. So O55 is settled first, as a spike
against the real iPad, before any of §1 is built. It costs one attempt and it
closes an open item either way. If it fails, its own entry already names the
fallback — a password-shaped per-device credential, with §2's table amended
rather than the prefix removed — and that becomes work before this branch, not
inside it.

*A correction belongs here, because a plan was nearly built on it: no device test
of a generated CBZ in Panels has ever been run. `docs/notes.md` ends at
2026-08-24 and records nothing of the kind, and #85's commit body states "offline
reading in Panels does not work" as a consequence read off the code — the
acquisition link pointed at a cover image — not as a measurement. The claim that
a device test had proven it was unsourced.*

## 6. O66, and the decision this branch will reach

O66 — a generated `Range` resume reads and discards the prefix instead of seeking
— is open with a **named trigger**: a fresh `taiyaku serve --log-requests`
session against Panels, bound to the LAN, showing `Range` on an acquisition
fetch. The existing `panels*.jsonl` logs predate request-header capture and
report `(not recorded)`, so their silence is an instrument's blind spot rather
than evidence.

The device session in §5 is the first opportunity to fire that trigger, because
until this branch lands there is no acquisition fetch to put a `Range` on. The
owner's decision is that if it fires, the seek is built here.

**The cost of that decision is stated once, and it is a real one.** The remote is
squash-only, so a branch becomes exactly one commit on `main`. An O66 fix landing
here merges into the same commit as a feature, and nobody can revert one without
the other — against this project's own rule that a branch holds one checkable
claim. The mitigation is ordering, not argument: the O66 work is the **last**
step, after the gate is green on everything else, so that splitting it into a
stacked PR remains a live option at the moment the evidence arrives rather than a
rewrite. The choice is made then, with the log in hand.

If the trigger does not fire, O66 stays open exactly as written. Nothing here
weakens its evidence standard.

## 7. Testing

The route:

- a restricted account is refused on the OPDS route, driven through
  `every_route_declares_its_grant_posture` — whole-work and split-across-roots,
  the same two shapes every other `Refuses` row is driven against;
- `can_download == false` answers 403, and is a different assertion from the 404
  above rather than a second spelling of it;
- **anonymous answers 401 *with* `WWW-Authenticate`** — the property the mirror
  exists for, and the one that silently regresses if the route is ever moved
  under a different guard;
- byte-identical output to the `/api` twin for the same edition, which is what
  makes "one handler, two registrations" a claim rather than a hope.

The feed:

- the acquisition link's `href` is the download route and its `type` is what the
  route would send — asserted for a generated edition **and** for a
  single-member passthrough edition of a non-`cbz` type, since one is exactly
  the case §9 got wrong;
- a `can_download == false` principal gets the page link, and the entry still
  carries exactly one acquisition link in both cases;
- the PSE link is unchanged in both cases. A test that would still pass with PSE
  deleted is not evidence, so it asserts the link's presence and its
  `pse:count`.

The UI, in `Series.test.tsx`:

- the grid tile's anchor is present, points at the `shown ?? w.editions[0]`
  edition, and is **not** inside the `<Link>` — asserted structurally, because
  that is the failure a snapshot would render identically;
- the table's page-count cell is an anchor for a held edition and plain text for
  an em-dash cell;
- `can_download: false` renders neither, in both layouts. `Shell.test.tsx`
  already fixtures `can_download: false`, so the absent case has a home.

The device session records, in `docs/notes.md` with the date and what was run:
whether Panels accepted the key (O55), whether the acquisition link produced a
readable download, and what request headers the log actually captured (O66).

## 8. Branch shape

One branch, one squashed PR. O55 first because it can invalidate the rest.

0. **The O55 spike.** Not a commit. If it fails, this branch stops and the
   credential fallback is designed first.
1. The route registration, its posture row, and §7's four route tests.
2. The shared type helper, `work_feed`'s two link cases, the doc-comment
   rewrite, and the feed tests.
3. The grid tile control and its tests.
4. The table cell control and its tests.
5. `docs/status.md` — the four counted figures, the `taiyaku-serve` entry's
   deferral sentence (which becomes false on this branch and must be rewritten
   rather than left standing, for the same reason §9 gives about the doc
   comment), and O55's closure with its five-part edit and the count-word sweep.
6. The device session, and `docs/notes.md`.
7. **Only if O66's trigger fired**: the seek, or the decision to stack it.

## 9. Open items this moves

- **O55 closes**, either as verified or as failed-with-a-fallback-designed. It
  cannot stay open past §8's step 0.
- **O66** either fires and is decided at step 7, or stays open unchanged.
- No new item is expected. If §2.2's per-edition `metadata` call turns out to
  cost something measurable on a large feed, that is an item rather than a
  silent regression.

## 10. What is deliberately not done

- **No work page.** §8's opening argument is unchanged: there is none, and the
  `AlignmentPanel` is a preference that can be switched off, so a download placed
  inside it would be a download some readers cannot see.
- **No per-member UI.** The route exists for direct callers; a reader wants the
  volume.
- **Nothing about O61.** Serving with sidecar paths on argv already lists works
  the routes cannot serve; the download control inherits that and does not
  worsen it.
