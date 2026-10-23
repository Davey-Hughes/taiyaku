# Grouping the Configuration screens, and folder becomes directory — design

**Date:** 2026-10-20. **Branch:** `config-grouping-proto`, from `main` at
`72f98caf`. **Replaces** the Later entry "Next: group the Configuration
screens, carrying Stump's motifs over" in `docs/status.md`; this is piece 3
of the sidebar and readability spec (`2026-10-18-sidebar-readability-about-design.md`, §0).

The user found the Configuration screens hard to read: "things are too
cluttered, text is too small, separation between text and background
particularly in the dark mode is hard to read, and there isn't enough visual
separation between sections". They like Stump's settings UI and want its
motifs carried over, not copied: settings in bordered boxes with the heading
outside, rows split by faint lines, label and help on the left with the
control on the right, a page title with a one-line description. Trying the
prototype, they added two asks of their own: the end of a settings tree
opens beside its list on a wide screen, and "folder" becomes "directory"
everywhere.

§1–§4 were decided on a prototype over three rounds on a scratch server;
§5 was decided in conversation.

---

## 0. Scope and order

**In scope:** Configuration's four tabs (Libraries, Users, Rules, Backup)
and every page under them, plus `/account`, About and Licence, so they
share one look. **Not in scope:** Proposals (a queue with filters, not a
settings page); the reader, which stays dark in both themes; the reader's
Account dialog, which shares `Account.tsx` with `/account` through its
class map and keeps its current look. No setting gains or loses a function.

The tab row stays as it is; no settings side nav.

| PR | Holds |
|---|---|
| 1. Rename | §5: folder becomes directory, everywhere |
| 2. Configuration | §1–§4 for Libraries, Users and Backup, every page under them, and side by side |
| 3. The rest | §1–§2 for Rules, `/account`, About and Licence |

The rename lands first, so the other two are written against the new word.

**On the branch, as the user tried it:** `a5337327` (the components; the
account page, a folder's page and Backup regrouped; help text rewritten),
`ded13af7` (side by side; the Users table and a library's page on the
components), `1f538bc9` (the gap after the list; the selected row inside
its box). These are the starting point for PR 2: it rebases them onto the
renamed `main`, holds them with tests and deletes the prototype switcher; it
does not rebuild them.

## 1. Components

Four components in `web/src/settingsLayout.tsx`, styled in
`web/src/styles/settings.css`. Every in-scope page is built from them, and
the hand-built back link, title and `.actions` row each page draws today go.

- **`PageHeader`**: an optional back link, the page title, an optional
  one-line muted description, and an optional slot for the page's actions.
- **`Section`**: the heading, and an optional one-line description, outside
  a bordered box that holds rows, or a table as its whole content.
- **`Row`**: the label and its help on the left, the control on the right.
  It gives its control the ids for `htmlFor` and `aria-describedby`, so a
  checkbox toggles from its label and the help is read with the control. A
  row can hold an item that is not a form control: a session or a key, with
  Revoke on the right. Below a width threshold (the prototype's container
  query on the box) a row stacks, label above control.
- **`RemovalSection`**: the last box on a page that can remove something,
  with a danger-coloured edge. It holds Remove… and, once that is pressed,
  the existing confirm box. Remove… leaves the action row, so no destructive
  button sits beside Scan. In the confirm, its button and Cancel sit side by
  side, on two lines where they do not fit on one, and the disclosure of what
  re-deriving would find, where there is one, comes after them.

**Headings:** a page title is one level below the screen's h1, and a
section one level below the title. Configuration keeps its visually hidden
h1, so its pages have h2 titles and h3 sections; `/account`, About and
Licence keep a visible h1 title and have h2 sections. Size comes from the
component's class, not the tag, so a section looks the same at either level.

**Width:** every in-scope page uses the Configuration measure except Rules,
whose two-pane editor keeps its own width; only its headings and boxes change.

## 2. The look

The user chose each of these on the prototype's switcher (round 1: "it
looks good"):

- **Label beside the control**, not above it.
- **Heading outside the box**, not inside it.
- **Bare box**: the page's background, not `--surface`.
- **Strong edge**: the box border is `--line-strong`, and the faint `--line`
  is kept for the dividers between rows. In Ink, `--line` measured APCA Lc 0
  against the background, too faint to edge a box.

A table in a box sits the box's padding in from all four sides, so a last
row has space below it, and the selected row's background and accent strip
are drawn inside that inset, not on the box's border (round 3, after the
user's screenshot of a selected last row flush with the border).

## 3. Side by side

On a wide screen, the end of a settings tree opens beside its list:

| List | Opens beside it |
|---|---|
| The Users table | an account's page; New account |
| A library's page | a directory's page |

Clicking a library in the Libraries table still opens its page on its own,
because a library is not the end of its tree. New library stays a page of
its own.

- **The address changes exactly as today**, so Back and a shared link work;
  only the layout depends on width. Below the breakpoint the opened page is
  drawn alone with its back link, as today.
- **The breakpoint** is the list's column, the gap and the opened page's
  measure added up from their tokens, never written as a literal.
- **The list** is a column at least 20rem wide that grows to the table's
  measure where the window allows (the user ruled on 2026-10-21 for this,
  as tried on the prototype, over a fixed 20rem). It keeps all its
  information (the user chose "full" over names only); while its column is
  narrower than 36rem each row is drawn in two lines, as on a phone. The
  opened page starts one gap after the list (`--space-6`) at its own
  measure, and width beyond the list's measure goes to the page's right,
  never between the two (round 3: "the panel shouldn't be right aligned, it
  should basically be floating from an offset from the list"). The list is sticky with its own scroll, so a long page scrolls
  past it.
- **The open row** has `aria-current="page"` and the highlight from §2.
- **Focus:** opening a row moves focus to the opened page's heading, in both
  layouts (today it falls to the body). Closing it by its back link returns
  focus to the row that was open.
- **Mounting:** the list stays mounted while rows are switched, so its
  scroll and state survive; the opened page remounts per row, as today. The
  narrow layout does not mount the list at all, so a phone makes no reads
  for a list nobody sees (the prototype mounts it hidden; PR 2 corrects that).
- **Notices** are drawn once: on the opened page when one is open,
  otherwise on the list.

A library's page and its directories share one route, so the library page
stays mounted while directories are switched.

## 4. The pages

As built on the prototype, for the five pages it covers:

- **An account** (Configuration → Users → a name): header with ‹ Users, the
  name, and a description line from data the page already loads (role, and
  when it was last seen); the locked notice under it. Sections
  Role & permissions (Role; Can download, Can stream and Can stamp, each a
  row with its help), Libraries (the every-library or chosen-libraries
  choice as a row, the grant list below it), Password (field and button on
  one row; "Password set" is said from a polite live region, as "Password
  changed" is on `/account` and in the reader's account panel), Sessions &
  keys (the tally as the description; Revoke at each row's end); then the
  removal box. The last-admin reason is two sentences, one beside Role and
  one beside Remove….
- **A directory** (a library → a directory): header with ‹ the library, the
  path as title, the watch, scan-every and last-scan facts as the
  description, and Scan directory · Move… as actions. Move… opens in a modal
  dialog over the page, so nothing on the page moves (the user's ruling at
  PR 2's checkpoint, 2026-10-21). Sections How it is scanned (scan interval; exclude globs, with
  its hint as the row's help; keeps documents under `.taiyaku/`), How it is
  read (edition key, default kind, default language), the scan report when
  there is one; then the removal box.
- **Backup**: the description line says a backup is the whole database,
  password hashes included, so that warning stays visible; Back up now is
  the action. Section Backups holds the list; a "How backups work"
  disclosure holds the rest (restore is out of band, a sidecar export is not
  a backup, no schedule here).
- **The Users table**: header "Users" with a one-line description and New
  account as the action; the table in a box.
- **A library**: header with ‹ Libraries, the name, the works-and-directories
  count as the description, Rename… and Scan library as actions. Rename…
  edits the name in the title's place and Save · Cancel take the actions'
  place, so nothing below the header moves (the same ruling). A refusal is
  a callout laid over what is under the header, at the header's foot, as a
  form's validation bubble is: the page's own words in a box with an edge,
  on the raised surface, inside the page's column and clear of the phone
  bar. No room is kept for it, and it goes when the name is edited or the
  editor is closed, so opening the editor, a refusal and its going move
  nothing on the page (the user's ruling of 2026-10-21 on the look-over).
  A name another library holds, in any ASCII case, or `.` or `..`, is
  said in that callout as it is typed, in the server's own sentence, and
  Save is drawn held at once until the name is free; New library's name
  and New account's (another account's exactly as written) do the same in
  their refusal's place (the user's ruling of 2026-10-21, after trying the
  refusal). The server still judges every save;
  Directories
  in a box with Add directory…; What the scan found; then the removal box,
  whose button reads Remove….

The rest follow the same pattern, with no prototype round of their own:
the Libraries table, New library, New account (PR 2); Rules, `/account`,
About and Licence (PR 3). On `/account`, About and Licence the existing
cards become sections with the heading outside the box.

**Addresses are names** (the user's rulings during PR 2, 2026-10-21): a
library's page is at `/config/libraries/<name>`, its directories' at
`/config/libraries/<name>/dirs/<did>`, and an account's at
`/config/users/<name>`, the name escaped only where an address needs it.
After a rename the page follows the new name, and the old address says No
such library. New library and New account are at `/config/new-library` and
`/config/new-account`, outside the names' space, so no name is reserved.
The server refuses `.` and `..` as a library's or an account's name, since
the router and the URL parser collapse either segment and no address could
hold it. It refuses an empty account name too, whose address would be the
Users table's (the user's ruling of 2026-10-22).

**A save, a make, a rename, a move, a removal and a backup are drawn
waiting only once the wait has lasted 200 ms** (the user's ruling of
2026-10-21), as is a removal's line saying what it would do, and as
Rename's Save draws "Saving…": the words that say a wait is on, and the
held look of what it holds, appear only for a wait still out then, while
what the wait holds is held from the send. A button that waits keeps the
box of the wider and taller of its two words, so drawing the wait moves
nothing. Four waits keep their own timing: the directory browser draws
"Reading directories…" at 400 ms, a scan's start holds a directory's Scan
directory, Move… and Remove…, or a library's Scan library and Remove…,
from the send, the picker's Add directory on a library's page is held from
the send while its add is out, and the line saying a scan is running is
drawn as soon as its start answers.

**A control that a wait or a hold takes while it has focus keeps focus**
(the user's ruling of 2026-10-23), where a `disabled` control that has
focus drops it to the page in Chromium. It is held by `aria-disabled`,
through `HeldButton` for a button, and a press, a choice or an edit of it
is turned down while it is held, as the account page's checkboxes and
radios are held (O159). A held button, select or text box is drawn as a
`disabled` one is, under forced colours too. An account's Remove… stays
drawn and held while its confirm is open, as a library's and a
directory's do. The exceptions are Save, Make the library and Add, their
Cancels, the picker's Add directory, Set password once a set has emptied
its box, and See what this would do once the library chosen leaves the
list: these are held by `disabled`, and move the focus they had to the
name, path or password box, or to Move to.

**Help text** may be shortened, moved beside the setting it explains, or
folded into a disclosure, but every fact stays. The prototype's rewrites
are in its commits and land with PR 2; the user tried them on the
prototype ("it looks good") and raised nothing about them. PRs 2 and 3 write the remaining pages' help the same way.

## 5. Folder becomes directory

"Folder" becomes "directory" everywhere, with "dir" where a name has to be
short (the user: "everywhere, but use 'dir' where appropriate to shorten").

- **Prose says "directory"**: sentences, headings, buttons and notices, in
  the web app, in the server's sentences the web app shows, and in the CLI's
  help ("Add directory…", "Scan directory", "Directories").
- **Names say "dir"**: the address (`/config/libraries/<name>/dirs/<did>`), the
  flag (`--dirs`), the environment variable (`TAIYAKU_DIRS`), API routes and
  fields, file names and code identifiers (`dirs_api.rs`, `DirPicker`).
- **A clean break**: no alias for `--folders`, `TAIYAKU_FOLDERS` or the old
  addresses. The server refuses to start when `TAIYAKU_FOLDERS` is set,
  naming `TAIYAKU_DIRS`, rather than starting with no directories. The
  image's `TAIYAKU_FOLDERS=/media` becomes `TAIYAKU_DIRS=/media`, and the PR
  body says the deployed container's setting must be renamed in the same
  update.
- **Two commits**: one renames files and identifiers only, with no string
  or behaviour change, so it can be checked mechanically; the other changes
  the words, the flag, the variable, the routes and the fields.
- **Living docs only**: README, `docs/ci.md`, `docs/sidecar.md`, and
  `docs/status.md`'s open rows and Later list. Dated specs and plans, closed
  rows and the comments in applied schema migrations keep "folder", because
  they record what was true when written. The database's tables already say
  `root`, so no migration is needed.

This replaces the 2026-10-07 ruling's "Scan folder"; "Scan library" stays.

## 6. Testing

- **The components**: the label reaches its control, the help is the
  control's description, heading levels follow the rule in §1, and the
  removal box holds its confirm box.
- **Side by side**, in a real browser at a wide and a narrow viewport: the
  pair or the page alone; the selected row's `aria-current`; focus on the
  heading after opening and on the row after closing; the list surviving a
  row switch (a marker on its DOM); the narrow layout making no list reads;
  the gap the same at two wide widths; a selected last row inset from the
  box's bottom; no horizontal scroll at phone width, in Paper and Ink.
- **Existing tests** that pin the old structure are re-pinned against the
  components, not deleted; each test named for a defect is watched failing
  against that defect.
- **The rename**: the identifier commit leaves the test count and the
  production line coverage unchanged (an lcov diff against `main`); after
  the words commit, a case-insensitive grep for "folder" finds only the
  residue §5 keeps.
- The full gate, the mutation sweep and a whole-branch review on each PR,
  as usual.
