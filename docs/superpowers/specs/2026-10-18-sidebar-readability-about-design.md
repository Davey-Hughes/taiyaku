# Sidebar, readability and About — design

**Date:** 2026-10-18. **Branch:** `sidebar-readability-proto`, from `main` at
`e4f2e4e5`. **Closes:** O221 (with the first PR). **Replaces** the Later
entry "A sidebar and readability redesign" in `docs/status.md`.

The user asked for "some slight redesign" after O219: keep the site's motif,
library browsing and the reader; trim the sidebar; and make text easier to
read, "especially in the dark mode". Asked what was hard to read, they named
all four candidates — small grey text, ordinary body text, too much at once,
and the Configuration screens in particular: "things are too cluttered, text
is too small, separation between text and background particularly in the
dark mode is hard to read, and there isn't enough visual separation between
sections". They like Stump's configuration UI and want motifs carried over,
not copied. Mid-way they added two ideas of their own (search as a bar on
the Library view; the Libraries heading as the all-libraries link) and asked
for a version label, an About page and a licence page.

Everything in §1–§3 was decided by trying a prototype on scratch servers over
five rounds; §4 was decided in conversation (the user skipped its prototype).

---

## 0. Scope and order

Four pieces; this spec holds three of them.

| Piece | Here | PR |
|---|---|---|
| 1. Sidebar, More, View, search bar | §1, §2 | first |
| 2. Text: size floor, Ink colours, Ink weight | §3 | first |
| 3. Grouping the Configuration screens (Stump motifs) | **own spec, after the first PR** | — |
| 4. Version label, About, licence | §4 | second |

Pieces 1 and 2 share a PR because the prototype entangles them (its first
commit carries both) and the user judged them together.

**On the branch, as the user tried it:** `f85d07fa` (sidebar menu, View
buttons, 14px floor, Ink lift), `88722a71` (weight 500 in Ink — the variant
the user chose: "the bolder text helps"), `439ba0f1` (Theme above the
button), `2c490905` (the button reads More), `5c222bfb` (search bar; Libraries
heading as the link), `62e76a9b` (a non-admin's More is a link). These are
the starting point. The plan holds them with tests and fixes §5; it does not
rebuild them.

**Not in this spec** (to the Later list): items the user adds to the
sidebar, or more quick items in it — the user noted the trimmed sidebar
leaves room for both; a version field for `taiyaku.notes.json`, the one
sidecar file without one; licence notices for every Rust and npm dependency,
generated at build; semantic versioning (the short commit is the version
"for now").

## 1. Sidebar

Top to bottom, in the column and in the phone drawer alike:

1. The wordmark.
2. **"Libraries"** — the heading is a link to `/` (every library), drawn in
   the heading's face with its hairline, current on `/` with no `?library`.
   It is drawn even when the account has no library, so that account keeps
   a way to `/` and its "No library shared" state.
3. The library list, each a link to `/?library=N`, as now; absent when empty.
4. At the foot: the version label (§4), then **More**, then **Theme** (the
   System/Paper/Ink control, unchanged, with its visible legend), last so that
   no link sits in the window's bottom-left corner, where a browser draws a
   hovered link's address (the user's ruling of 2026-10-27).

The sidebar's Library and Search links go; so does BrowseSettings, whose
controls move to §2. The doubled "Library" heading goes with it.

**More.** For an administrator it is a disclosure button that opens upward,
holding Proposals (with the open count), Configuration and Account; the
button carries the same count as a badge, and its accessible name carries
the count in the words the Proposals link uses for it. For anyone else it is a plain link to
`/account`, still labelled More — a menu of one item is a detour. Either
form is drawn in the accent while the screen is Proposals, Configuration or
Account, standing in for the current mark those links used to carry.
"Settings" was rejected as the label because the menu holds Configuration.

**The disclosure pattern, not `role="menu"`.** The panel holds only links
now that Theme has left it, and a menu role would demand arrow-key roving
that a list of links does not need. The button has `aria-expanded` and
`aria-controls`; Esc closes it and returns focus to the button; a click
outside and any navigation close it. The prototype's `role="dialog"` with
`aria-haspopup="dialog"` dates from when the panel held a radio group.

## 2. View, and search on the Library view

**View.** A View button at the right of the page's top row opens a panel of
the same browse preferences, stored and applied as today; only where they
are drawn changes. Labels are in sentence case.

| Page | Controls |
|---|---|
| Series | Layout, Edition selector, Panel, Density, Reset |
| Library (`/`, `/?library=N`) | Continue strip, Density, Reset |

*Replaced 2026-11-15 (O285): Density is Cover size, a slider, on both
pages; see `2026-11-14-cover-size-and-continue-row-design.md`.*

Reset restores all five preferences from either page, as it does today.
The panel follows §1's disclosure rules.

**Search is a bar on the Library view** (the user's idea). The top row of
`/` and `/?library=N` is the search box, then View. Its scope is the library
the view shows — every library on `/` — and its placeholder names it
("Search every library", "Search Volumes"); the "Search in" select goes,
because the sidebar now picks the scope. The query lives in `?q=` with
`replace` navigation, debounced and sequence-numbered as `Search.tsx` did,
in `searchHits.ts` with that file's reasons moved alongside. While the box
holds a query the hits replace the grid (`SeriesGrid`, and `EmptyState`'s
`no-matches`), and the tabs, the sort and the continue strip are hidden: the
search endpoint takes none of them. Clearing the box restores the grid.

- **The box keeps its own text**, and `?q=` sets it only on a change the box
  did not make (Back, the redirect). The router commits each URL change in a
  transition — the prototype's builder's diagnosis — so a box drawn straight
  from `?q=` lost keystrokes typed fast ("oshi" arrived as "oi"). The
  builder also reports that `main`'s
  `Search.tsx` has the same pattern; the plan confirms it on `main` before
  filing anything.
- **An account that sees no library is not offered the box.** It has
  nothing to search, and "No library shared" says why. This closes O221:
  that row asked for a hint where search drew `no-matches` for such an
  account, and the search it described no longer exists.
- **`/search` redirects** to `/`, carrying `q` and `library`, so old links
  keep working. The phone bar's search icon goes to the Library view (keeping
  `?library` when on one) and focuses the box. Arriving from another screen,
  the focus lands after navigation, which iOS may not answer with the
  keyboard; the device check (§6) settles whether that needs more.

## 3. Text

**Measured with APCA, because WCAG 2 passed what the user could not read.**
Every text token already cleared 4.5:1 on bg, surface and field in both
themes (style pass spec §2). APCA scored Ink's muted and accent at Lc 48 and
45 on the page against Paper's 70 and 70 — the asymmetry the user reported.
WCAG 2 is kept as the existing floor; APCA is added beside it.

**A 14px floor for text that is read.** `--text-meta` becomes `.875rem`.
A new `--text-badge: .75rem` serves count badges, kind badges and similar
chips, which keep 12px; the hard-coded `0.7rem` sites in `admin.css` move to
one or the other by what they are. Both themes.

**Ink's colours, lifted; Paper's unchanged.** Every Ink text token reaches
APCA |Lc| ≥ 60 on bg, surface and field, by changing OKLCH lightness only.
Where a colour cannot hold its chroma that light inside sRGB it is cut to
the gamut edge at the same hue; the accent therefore reads as a paler salmon,
which the user saw in the prototype and chose. Ink's surface steps away from
the page (WCAG 1.12 → at least 1.25:1) so cards and buttons read as objects;
`--line` is lifted with it so a hairline on surface keeps its old contrast;
`--line-strong` stays ≥ 3:1 on bg, surface and field; text on the accent
stays ≥ 4.5:1.

| Ink token | Before | After |
|---|---|---|
| surface | `#211e18` | `#2b2821` |
| muted | `#a69b87` | `#c1b6a2` |
| accent | `#e8715a` | `#ff9d88` |
| danger | `#f28b9a` | `#ff9aa8` |
| success | `#95c283` | `#97c485` |
| line | `#35312a` | `#3e3932` |
| line-strong | `#746c60` | `#787064` |

`text` and `warning` already cleared the floor and are unchanged.

**Ink's UI text at weight 500** (the variant the user chose): light strokes
on a dark ground read thinner than the same strokes on paper. It applies
wherever Ink is in force — Ink chosen, or System with an OS dark preference
— and not to titles (Shippori Mincho). Zen Kaku Gothic New 500 is already
shipped.

**The reader is unchanged.** It is dark in both themes by an earlier ruling
and the user called it "quite good"; it reads several of the lifted tokens
and `--text-meta`, so `.reader-route, .reader` pin their old values, and the
weight rule does not reach it.

## 4. Version, About and licence

**The version is the short commit** — the first seven characters, which is
what CI tags the image with (`sha-<7>`), so the label matches the image that
is running. The user: "we can use the github short sha for the version on the
site for now."

**Getting the commit into the binary.** `taiyaku-serve` reads
`option_env!("TAIYAKU_COMMIT")` at compile time. The Dockerfile's server
stage takes `ARG TAIYAKU_COMMIT` and CI's docker job passes the commit it
already names the image after. Nothing runs git during a build: `.git` is
excluded from the image build, and a local build without the variable
reports no commit rather than a wrong one.

**`GET /api/about`**, behind a session like the rest of `/api`, returns the
commit (or null), the repository URL, and the version of each format
taiyaku defines, read from the constants that write them so the page cannot
drift from the code: the sidecar `taiyaku.json`, `.library.json`,
`.progress.json`, `.truth.json`, `.regions.json`, the database schema, and
the OPDS version the feed claims (1.2, with the Page Streaming Extension).
`taiyaku.notes.json`, the HTTP API and the wasm module are listed as
unversioned.

**The repository URL** is `https://github.com/Davey-Hughes/taiyaku`, given
by the user. `Cargo.toml`'s `repository` names the forge, which needs a
sign-in; it becomes the GitHub URL, and `/api/about` returns
`CARGO_PKG_REPOSITORY`, one source for both. On 2026-10-18 the GitHub URL
answered 404 to an anonymous request (private or not yet created); the links
are built regardless and work once it is public. A commit links to
`<repository>/commit/<full hash>`, which assumes GitHub mirrors the forge.

**The version label** sits at the top of the sidebar's foot, above More, in
the column and the drawer: the short commit (or "unknown build"), in muted
text at the 14px floor, a link to `/about`. It is the way to About for every
account, so a non-admin's More stays a plain link.

**`/about`:** the commit (linked), the source and issues links, the format
table, the OPDS catalog address, and a link to the licence page.
**The OPDS address** is built from the page's origin plus
`/opds/v1.2/catalog`, which the web client does not show today; it gets a copy
button here and on Account, beside the existing OPDS line. The button is
drawn only where the Clipboard API exists (a secure context); elsewhere the
address is selectable text.

**`/about/licence`:** taiyaku is GPL-3.0-or-later, with the full text from
the repository's `LICENSE`; then the fonts, each OFL-1.1 with its own
licence text — Shippori Mincho B1 and Zen Kaku Gothic New from their
packages' files, and the vendored marks subset from
`web/src/styles/fonts/OFL.txt`. The texts are bundled from those files at
build, not copied by hand; no licence text reaches `web/dist` today, so the
Dockerfile's web stage must also copy `LICENSE`.

## 5. What the prototype left

- The node project is red at the prototype's head: its builder counted 17
  failures in 3 files at `88722a71`, from the reader pin, the removed
  `.browse-settings` rules and the weight rule's scope. The plan's first task
  recounts against `main`, whole projects, before anything is fixed.
- The Sidebar and Shell tests were not run and need rewriting for §1.
- `index.css.test.ts` computes the reader's contrasts with the lifted
  values, not the pinned ones the reader draws; it must read the reader's own.
- `BrowseSettings.tsx` is unused and goes.
- The panels use `role="dialog"`; §1 asks for the disclosure pattern.
- The prototype set `--text-meta` and `--text-ui` to the same size; both
  names stay, since they name different roles.

## 6. How it is held

By the rules this repository has learned to need: each behaviour gets a test
watched failing against the defect it names, hand-applied; browser tests use
the real mouse (`web/src/testing/mouse.ts`) for the panels, focus the page
before any real key press, and run alone with `-t` as well; gates run whole
projects.

- **Panels:** Esc closes and returns focus; outside click and navigation
  close; a non-admin gets a link with no `aria-expanded`; the badge's count
  and accessible name.
- **Search bar:** fast typing keeps every character; clearing restores tabs,
  sort and strip; the scope follows `?library`; `/search?q=` redirects with
  hits; a no-library account has no box; overtaken responses are dropped.
- **Tokens:** `index.css.test.ts` computes APCA beside WCAG for every text
  token on bg, surface and field in both themes, with Ink's floor at 60;
  Paper's values are recorded, not changed. The reader's resolved values
  equal `main`'s.
- **Weight:** 500 under forced Ink and under System with a dark preference;
  400 under Paper and System with a light one; titles and the reader
  untouched.
- **About:** `/api/about` returns the constants, and null for the commit
  when the build had none; the label and the commit link are drawn only with
  a commit.
- **By eye, on the built app** (production CSS differs from dev, O218): both
  themes, desktop and phone width, against a `main` build beside it. On an
  iOS device: the search icon's focus from another screen.

Then a whole-branch review, which reviews its own fixes, and the claim check
over the status rows, the notes and the PR body.

## 7. Docs

`docs/status.md`: the Later entry is replaced by what the first PR ships;
O221 is closed by the five-part edit; the four "Not in this spec" items join
the Later list; piece 3 is recorded as next. `README.md` gains the OPDS
address's new home and, with the second PR, the About page and the
`TAIYAKU_COMMIT` build argument.
