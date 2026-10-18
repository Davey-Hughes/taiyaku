# O219: Configuration's libraries and accounts as tables — design

**Date:** 2026-10-17. **Branch:** `o219-config-tables`, from `main` at
`9013e067`. **Closes:** O219 (with the second PR), O199 (with the first).

Trying F3 on 2026-10-17, the user found Configuration → Libraries confusing,
cluttered and long: making a library took a name, a path, "Add to the list"
and "Make the library", and every library card repeated the same add-folder
form. After trying O220 the same day they widened the work to Users and gave
its direction: **a table with a row per library or account, where a row opens
that one's own edit page, not a list of cards.** The rest was decided with
them over two questions, built as a prototype, and tried on a scratch server;
they approved it ("it looks good", "ok it works").

---

## 0. What exists

- On `main`: `Libraries.tsx` draws the make form and a `LibraryCard.tsx` per
  library; each card draws a `RootRow.tsx` per folder with its settings,
  Scan folder, Move… and Remove…. `Users.tsx` draws the add form and a
  `UserRow.tsx` card per account. `/config` lands on Rules.
- On the branch, **as the user tried it**: the prototype, whose Libraries half and
  tab order are `37b06f0b`. These are the starting point; the plan holds
  them with tests and fixes §6, it does not rebuild them. They reuse
  `RemoveLibrary`, `RemoveRoot`, `MoveRoot`, `ScanReport`, `MoveCounts` and
  `FolderPicker` and change no server code: every page reads the existing
  list endpoints (`GET /api/library/libraries`, `GET /api/library/users`) and
  picks its item by id, because there is no single-item GET.

## 1. The rulings

1. Tables, a row opening its own page (the user's direction, above).
2. **A folder has its own page** under its library's. Chosen over folders that
   open in place on the library page, and over the library page showing every
   folder's settings.
3. **A library is made from a name and one folder**; more folders are added
   on its page. Chosen over keeping the list of folders on the make page.
4. **Libraries and Users lead the Configuration tabs**: Libraries, Users,
   Rules, Backup. A bare `/config` lands on Libraries (asked for after
   round 1).
5. **Asked for in the hand check of the finished build** (2026-10-18): the
   refusal of a folder that overlaps a held one names the held folder from
   inside the chosen one (`<chosen> holds `Oneshots`, which is already a
   folder of the library `Extras``), and the folder browser's Up says where
   it goes ("Up to extra", "Back to all browsable folders"). The refusal is
   the server's (`scan::vetted`), so the CLI prints it too.

## 2. Pages

| Route | Page |
|---|---|
| `/config/libraries` | table: Name · Folders · Works · Last scan; New library |
| `/config/libraries/new` | Name, one folder (typed or Browse…), Make the library, Cancel |
| `/config/libraries/:id` | back link; name; "N works in M folders"; Scan library, Rename…, Remove library…; Folders table (Path · Scan every · Last scan); Add folder… |
| `/config/libraries/:id/folders/:fid` | back link to the library; the folder's settings; Scan folder and its report; Move…; Remove… |
| `/config/users` | table: Name · Role · Libraries · Last seen; New account |
| `/config/users/new` | Name, Password, Role, Add |
| `/config/users/:id` | back link; sections Role & permissions, Libraries, Password, Sessions & keys, Remove… |

`/config/roots` still sends to `/config/libraries`.

## 3. Libraries

- A table row is one link (the name), stretched over the row, so the row is
  clicked and the name is tabbed to. "Last scan" is the latest of the
  library's folders', with its status when that is not ok, or "Never". With
  no library, no table: the "No libraries yet…" line.
- Making a library goes to its page. Add folder… opens the picker on the
  library page; the new folder joins the Folders table.
- A folder's settings save as they did in the card (on blur, or on change for
  the checkbox).
- Scan library stays on the library page; when a scan it watched ends, the
  report of each folder that scan held is drawn there under "What the scan found". Scan folder and
  its report are on the folder page. A scan anywhere in a library holds a
  folder's Scan folder, Move… and Remove…, so the folder page reads its
  library's scan status; while a library's removal is being confirmed or
  sent, its Folders table draws no links. A folder's removal or move holds the controls of its own
  page only. The server keeps the rest safe: a scan reads its folders again
  under the job lock, a removal of what has gone finds nothing, and a move is
  refused while a scan holds a folder.
- A removal or a move goes up one level, with the existing report drawn there:
  a removed folder to its library's page, a removed library or a folder whose
  move or removal took its library to the table.
- The sidebar's Libraries list follows a make, rename, removal or move without
  a reload, as O200 has it on `main`.

## 4. Users

- Libraries column: "Every library", the granted libraries' names, or "None";
  an administrator whose grants are limited reads "Every library, as an
  administrator", because an administrator sees every library. Last seen is
  the latest session's, or "—". The row marks "this is you" and a locked
  account.
- Making an account goes to its page. Removing one goes back to the table,
  which says "Removed <name>."
- The account page is the card's content under section headings, unchanged in
  what each control does.

## 5. Across both

- **Phone width:** under 36rem each table row is a stacked block, its name
  above its values, each value with its column's words ("2 folders", "Last
  scan: Never"), and no page scrolls sideways at 390 px.
- **Focus:** an action that changes page puts focus on the line that says what
  happened (a removal's or move's report, "Removed <name>.") or else on the
  new page's heading. An unknown or stale id draws "No such library.", "No
  such folder in this library." or "No such account" with the back link, and focus goes there.
- **Leaving a page keeps an edit:** a folder setting edited and not yet saved
  is saved when its page goes, by any route: a link, a tab, or the browser's
  Back. A form with its own button (Rename…, a password, the make forms) is
  dropped unsent on leaving, as any unsent form is.
- **An answer that lands on another page** — a removal's or a move's report,
  or the refusal of any write (a make, a rename, an added folder, a scan, a
  removal, a move or a save), sent from a page the person has left — is drawn
  on the page they are on, without taking
  focus there (a report is announced instead), and goes on the next change of
  page as any report does. Once another Configuration tab has
  replaced the screen, it is drawn nowhere, as on `main`.
- A failed list read draws the existing alert, not an empty table.
- Lists are read again on each change of page.

## 6. What the prototype got wrong

Found by the prototype's builder on the live server after the user's try;
each is fixed with a test watched failing first.

1. Make the library sends the path as it stood before the last keystroke: a
   fast Enter sent `…/Trial-` for `…/Trial-a` (`NewLibrary.tsx`, the path
   reaches it through an effect in `FolderPicker.tsx`). It also checks the
   path trimmed and sends it untrimmed.
2. A folder setting typed and left by the browser's Back is lost (§5).
3. Making a library or an account, removing an account, and an unknown id
   leave focus on `body` (§5).
4. When a library scan ends, the library page draws every folder's last
   report, including one not from that scan.
5. `FolderPage.tsx` states that the server refuses a scan overlapping another;
   unchecked. Check it and make the comment true or remove it.
6. At 390 px the stacked rows read "last scan Never" and "last seen —"; the
   add-folder picker's Cancel sits on a line of its own; comments in
   `RootRow.tsx` and `AccountPage.tsx` still say "card".

## 7. Two PRs

The first carries Libraries, the tab order and the shared table styles; the
second carries Users. Each is cut so `main` works between them: the first
leaves `Users.tsx` and `UserRow.tsx` working as they do on `main`, under
their own route.

## 8. Tests

The card components' jsdom suites are moved to the pages they became, render
them under their routes, and keep every behaviour they hold. New tests hold
§2–§6: each table's columns, row link and empty or failed state; every
navigation after an action and where focus lands; the unknown-id line; a
setting saved on leaving by Back; the stale path; the 390 px layout in the
browser project. Every new test is watched failing against the defect it
names.

## 9. Docs and rows

O199's row closes and O219's narrows to Users until the second PR; O102, O202, O203 and O209 are re-read against
the new pages and narrowed or kept with what still holds. Docs that describe
the two screens are found by grep and updated.

## 10. Not in this

- **Times stay the server's strings** (UTC, no zone shown), as the cards drew
  them.
- **Folder and account ids in the URLs are row ids, which the store reuses
  after a removal**, so
  a bookmark to a removed folder or account can later open another (a folder
  is checked against its library). A setting saved on leaving goes to the id
  its page was drawn with, so if another administrator removes the folder and
  a new one takes its id while the box is typed in, the edit lands on the new
  folder, as a blur save could on `main`. Accepted for an administrator's
  screen; names were not used because a rename would break the link.
- No server change beyond that refusal's wording, no single-item GET, no
  link between a library's configuration page and its reading page
  (`/?library=<id>`).
