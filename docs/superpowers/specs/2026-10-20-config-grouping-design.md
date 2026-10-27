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
  a bordered box that holds rows, or a table as its whole content. An
  optional slot at the end of the heading's line holds the section's own
  actions, which end where the box's rows end.
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
whose boxes are each as wide as what they hold (§4); only its headings and
boxes change.

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

A box's rows are its padding and its edge in from its outer edge, one
length (`--settings-box-inset`), and what lines up with them from outside
the box is placed by that length: a heading's actions end on it.

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
  when it was last seen); the locked notice under it. Sections Role &
  permissions (Role; Can download, Can stream and Can stamp, each a row with
  its help), Libraries (the every-library or chosen-libraries choice as a
  row, the grant list below it), Password (New password, then Confirm
  password, and Set password in a row of its own at the box's foot;
  "Password set" is said from a polite live region, as "Password changed" is
  on `/account` and in the reader's account panel), Sessions & keys (the
  tally as the description; Revoke at each row's end; a Revoke whose row
  goes with focus on it hands focus to the nearest row's Revoke in that
  list, the next, else the one before, and with none left to the section's
  heading); then the removal box. The last-admin reason is two sentences,
  one beside Role and one beside Remove…. A write answered with no sentence
  of the server's, such as a proxy's page for a 502, is said as what was not
  done and the status: "A session for sam was not revoked: the server
  answered 502".
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
cards become sections with the heading outside the box, under a visible
h1 title. PR 3's pages, as built once the user had tried them
(2026-10-25):

- **Rules**: header "Rules" with no description, not at the settings measure
  nor at any measure of the screen's: from the breakpoint up, where Rules is
  wide enough for the list beside the rule's pane, each box is as wide as what
  it holds, all starting where the other Configuration pages' content starts,
  and the room past them is on the right (the user's rulings of 2026-10-26).
  The list and the rule being edited are each an untitled box, side by side
  where they fit. The list is as wide as its widest rule, up to
  the settings measure, 40rem; a rule's name wraps between its words and after
  a Rust path's `::` (`taiyaku_read::volume_number`), and inside a piece only
  where the piece is wider than its whole line. The rule's pane is as wide as
  its sentence while no rule is picked, or as its form once one is: the four
  boxes under Sets side by side, each 12 characters wide, for the short values
  they hold (a longer one scrolls in its box), and the pattern and the note
  each on a row of their own, so the longest seeded rule's note is whole in
  its box; no select gives the form width, since scope's options are absolute
  paths. Where the screen is too narrow for both panes at their own width,
  what gives way first depends on what the pane holds. With no rule picked,
  the pane's sentence wraps first, the pane down to 20rem, and then the list.
  With a rule picked, the list wraps its names first, so the form keeps its
  own width, and then the pane gives way to whatever room the list leaves (the
  user's ruling of 2026-10-26, which accepted that the list reflows when a
  rule is picked). The list never gives way past its longest piece unless that
  piece is too wide for the cap or to leave the rule's pane 14rem beside it,
  and then the piece breaks inside. The preview is beneath both, in a box titled Preview,
  at least as wide as the two together (the user's pick) and wider only where
  a tab holds something wider, a table or a path, up to `main`'s width, where
  a table scrolls in its box and a path breaks anywhere; the panes keep their
  own width whatever the preview's. Its heading's line ends with "Previewing…"
  once a preview has lasted 200 ms; until then the words are laid out unseen,
  so drawing them moves nothing. What is said in sentences, but for the
  pane's while no rule is picked, wraps at the width the boxes set, never
  wider than a reading measure, and gives them no width of its own. The save's re-derive disclosure, then Save, Import and Export,
  with Import's help under the three, follow at the foot, Export beside
  Import, on their words' baseline wherever they share a line, each wrapping
  whole. Below the breakpoint, and above it where Rules is narrower than
  36rem, the boxes are one column, each as wide as the page's column: 36rem
  holds the seeded rules' list whole, a gap and the rule's pane at 14rem,
  under either pointer, from a 17px root font up (at 16px and below, the
  breakpoint leaves Rules more room than the list needs), and a larger text size, which
  widens the sidebar and what the boxes hold but not the breakpoint, leaves
  Rules that room only on a wider window. A failure is said as what was not
  done and why: "The rules were not saved: …", and the same for a load, a
  preview and an import, "The rules cannot be exported: …", and for a picked
  file that is not JSON, "The rules were not imported: the file is not a rules
  document".
- **`/account`**: header "Account"; sections Password (Current password, New
  password, Confirm password, and Change password at the box's foot), API
  keys (what a key is for as the description, the catalog's address with
  Copy address where the boxes' other buttons are, at its row's end, or at
  its start where a narrow box stacks its rows (the row is drawn empty while
  the address is read, and says so where it could not be; wherever no Copy
  address is drawn, an unseen one keeps its room, so in each state the row
  is at least as tall as a line with Copy address in it), New key and Create
  sharing the password boxes' column, then the one-time secret's box and a
  row per key with Revoke) and Signed in (a row per session with Revoke, and
  Sign out at the foot). The reader's account panel draws the same controls
  in its own look, its confirm box labelled Confirm. What a create or a
  password change says moves nothing: "Key made: `<name>`. …" and "Password
  changed. …" are said from visually hidden polite regions in API keys and
  in Password, and the key's line never says the secret. A made key's box
  and its row are drawn together, by the listing after the create, which
  holds Create until it lands, and revoking the shown key takes its box away
  with its row, so each moves the page once. A listing of the keys or the
  sessions that lands after a later one has been drawn is not drawn, since
  it was read first; a later one that lands first does, in the render that
  draws it, what each earlier one still out would have done beside its list.
  The secret's caption names its key ("Key “tablet”: copy this now — …").
  Where the clipboard can be written the secret's box is a button that
  copies it, as Copy address does, and draws "Copied" on itself in its
  caption's place for a moment, moving nothing; elsewhere it is text to
  select. When a Revoke's row goes with focus on it, focus goes to the
  Revoke of the nearest row left in that list, the next, else the one
  before, passing over the current session's; with none left, to the New key
  box or to Signed in's heading, so that in the panel it stays in the
  dialog. A refused Create or Change password is said in a callout (below)
  in the server's own sentence, such as "a key needs a name, so the right
  one can be revoked later", or, for an answer with none, as what was not
  done and the status ("The key was not made: the server answered 502"), or,
  where no answer came, in the browser's words, such as "Failed to fetch",
  and goes when a box of that form is edited, focus leaves the form, or the
  next attempt is sent. A revocation's or a sign-out's failure is said above
  the sections, always as what was not done and then why, in the server's
  sentence, the status or the browser's words ("That key was not revoked: no
  such key").
- **Confirm password**, wherever a new password is typed: `/account` and the
  panel, an account's page and New account. While the two differ, the button
  that sends the password is held (below), and "The two passwords do not
  match." is said in a callout under Confirm password once the confirm is
  neither empty nor the start of the password, wherever focus is. A press of
  that button, or Enter in a box, while the two differ sends nothing and
  says the line too, though the confirm is empty or the start of the
  password, until either box is edited; then the rule before holds again.
  That needs a button a press can reach: Set password is `disabled` while
  its New password is empty, and New account's Add while the name or the
  password is empty, a name clashes, or the add waits, and there a press or
  Enter says nothing. The line's callout takes a press, as a refusal's does,
  so that a press on it reaches nothing beneath it, neither the held button
  nor Role's select, which Tab still reaches (the user's ruling of
  2026-10-26). The row under Confirm password that the line lies over,
  Change password's on `/account` and in the panel and Set password's on an
  account's page, keeps the room the line takes there, said or not, its
  words drawn in it unseen: where they take more than one line, as on a
  narrow phone or with larger text, that row is as much taller as they need,
  so the line lies under Confirm password and over no box, a press on
  Confirm password reaches it, and nothing moves when the line comes or
  goes; where they take one, the row is no taller than its button makes it
  (the user's ruling of 2026-10-26).
- **Callouts** on these forms (the user's rulings of 2026-10-26; where a
  refusal with no room under its row lies is the controller's call): a callout
  is drawn right under the row of the control it answers and lies over the
  rows of its section below that row, as far as its words need, covering each
  row's content whole, so no control and no line of words is left half
  covered, but for the foot of Role's help on New account (below). It stays
  inside its box's content, a padding's width in from the box's sides and its
  rows' own padding in from its top and foot, so a callout whose words fit in
  the box never meets its edge nor the next section's heading. Where its
  words would pass the last row's content, a refusal lies over the rows above
  the row it answers instead, not over that row, whose box
  is edited to take it away, except where they have no room, where it is laid
  up over it; the line saying two passwords differ never is, since the row
  under it keeps its room (above). So a refused Create lies over the shown
  key's box, else the first key, and, with no key listed or too long for the
  rows under New key, over the rows above New key, the address's row among
  them; a refused Change password and the line lie over Change password, which
  they cover whole, and a refusal too long for that room lies over the rows
  above Confirm password instead; on an
  account's page the line lies over Set password; and on New account it lies
  over Role's select, centred on it, over no box: where the select is wide
  enough for its words, on one line, over the select alone, with Role's label
  and help, Add and Cancel in view; where its words take two lines, as on a
  narrow phone or with larger text, over the foot of Role's help as well. A
  refusal whose foot would be past the edge of the view, as in the panel on a
  short window or on a phone, scrolls the page or the panel just far enough to
  show it whole; elsewhere nothing scrolls. A control focused under a callout
  has its focus ring drawn on the callout.
- **About**: header "About"; sections This build (the commit, the source and
  the catalog's address, a `dl` drawn as the box's rows), Formats (the table
  in a box) and Licence: "taiyaku is GPL-3.0-or-later." and "Its fonts are
  OFL-1.1.", a row each, each licence a link to its own on the licence page,
  the fonts' to the first font's. A failed request is said as "This build’s
  details were not loaded: …", in the server's sentence, or its status where
  it has none, or the browser's words where no answer came.
- **Licence**: header "Licence" with ‹ About; the contents under it; a
  section per licence whose heading carries its fragment; "↑ Back to top" at
  the end of each heading's line and at each box's foot, the two ending on
  one edge where the box's rows end, which puts the page's top in view and
  focus on its title, drawn landed on as a heading a contents link lands on
  is. Texts that do not arrive are said as "The licence texts were not
  loaded; reload the page to try again.", never in the browser's words,
  which name the chunk's address.
- **The sidebar's wordmark** (the user's ruling of 2026-10-26): a link to
  `/`, where the sidebar's Libraries heading goes, named "taiyaku,
  Libraries". At rest it is drawn as the plain wordmark was, where it was;
  it takes the sidebar links' focus ring, and under the pointer its name is
  underlined. The phone bar's wordmark is the same link, named the same. On
  a coarse pointer each reaches 44px to the touch from a target centred on
  it, drawn where it stood and over no other control. Opening the drawer
  gives focus to the Libraries heading's link, with Close one Shift+Tab
  back and the wordmark one more (the controller's calls). Login's wordmark
  stays a heading's words.

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
drawn as soon as its start answers. PR 3's waits are drawn the same way:
Rules' "Saving…", "Importing…" and "Previewing…", and the held look of
`/account`'s Change password and Create.

**A control that a wait or a hold takes while it has focus keeps focus**
(the user's ruling of 2026-10-23), where a `disabled` control that has focus
drops it to the page in Chromium. It is held by `aria-disabled`, through
`HeldButton` for a button, and a press, a choice or an edit of it is turned
down while it is held, as the account page's checkboxes and radios are held
(O159). A held button, select or text box is drawn as a `disabled` one is,
under forced colours too. An account's Remove… stays drawn and held while
its confirm is open, as a library's and a directory's do. The exceptions are
Save, Make the library and Add, their Cancels, the picker's Add directory,
Set password while its box is empty, as a set leaves it, and See what this
would do once the library chosen leaves the list: these are held by
`disabled`, and move the focus they had to the name, path or password box,
or to Move to. While the two passwords differ, Change password, Set password
and New account's Add are held by `aria-disabled`, through `HeldButton`, so
that they keep focus and a press reaches them, a mouse's where the line does
not lie over them, to be turned down and to say why (above), where nothing
holds them by `disabled` first. New account's Add
is held so for the mismatch alone: an empty name or password, a name clash,
and the add's own wait still hold it by `disabled`, and the focus a clash or
that wait takes goes to the name box.

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
