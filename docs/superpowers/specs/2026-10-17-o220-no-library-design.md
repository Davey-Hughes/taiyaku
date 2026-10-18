# O220: an account can be left seeing no library — design

**Date:** 2026-10-17. **Branch:** `o220-no-library`, from `main` at
`7a5b7f22`. **Closes:** O220. **Reverses:** O205's guard and the "last tick
cannot be cleared" half of O193 (§4, §7).

The user wants a library removable even when it is the only one some account
can see. They ruled on 2026-10-17 what that account sees afterwards: **nothing**,
until an administrator grants it a library. They chose this over two others:
letting the admin choose per account in the dialog, and widening the account
to every library after a warning. The redesign of Configuration → Libraries
(O219) comes after this, in its own PR, so that it builds on the new removal
behaviour rather than on the refusal this deletes.

---

## 0. What exists, read on 2026-10-17

Read at `7a5b7f22`. Nothing here was measured.

- An account's grant is its rows in `user_library` (migration 015). There is
  no other grant column. **No rows means every library**, and nothing can
  represent "sees no library". An admin sees every library whatever its rows
  say (`auth::Principal::from_account`), and the rows come back into force if
  it is demoted.
- The server carries the grant as `Principal.roots`, the union of the granted
  libraries' folders, where an empty list means unrestricted. `account::roots_for`
  builds it. `browse` (its root, member, pair and aligned clauses,
  `library_scope`, `narrowed`, `edition_visible`, `visible_editions`, `status`),
  `metadata.rs`, `routes.rs` (`list_works`, `refuse_if_hidden`,
  `on_the_grant`), `opds.rs` (whose `granted()` defaults a missing principal to
  the empty list) and `library_api.rs` all read the empty list as unrestricted.
- `account::Patch.libraries` takes `Some(vec![])` as "every library", and
  `revoke_library` revoking an account's last grant leaves it unrestricted.
- Four guards stop a grant becoming empty by accident:
  - `scan::removal_refusal`, `scan::library_removal_refusal` and Move's
    `relocate::refused_before` refuse, with 409 and a sentence naming the
    accounts (`scan::only_grant_holders`, `only_grant_of`), removing a
    library, removing its last folder or moving its last folder away. Each
    refuses the dry run too.
  - The `only_grant_refuses_delete` and `only_grant_refuses_move` triggers
    (migration 015) back them up with a fixed sentence, which reaches a
    client as a 500.
  - O205: a library with no folder cannot be granted (`account::ungrantable`).
  - The Users card will not clear the last tick (`UserRow.tsx`'s
    `toggleGrant`). Leaving "Only these" with nothing ticked saves nothing.
- `RemoveRoot.tsx` and `RemoveLibrary.tsx` ask the dry run when they open and
  draw a 409 as an alert with no confirm button. `MoveRoot.tsx` draws it under
  "See what this would do".
- Home tells "no library" from "not yet scanned" by `/api/library/status`
  alone (`EmptyState.tsx`). "No library yet" says nothing has been added to
  the server and how an administrator adds one.
- The CLI has `library user grant-library` and `revoke-library`. `list` prints
  a "granted:" suffix only for an account that holds grants.

## 1. The ruling

Removing a library, removing its last folder, and moving its last folder away
all go through whoever's only grant it is. An account left with no library
keeps its login and its settings, and sees no library until an administrator
grants one. Losing access deletes none of its reading progress or notes, but
removing a library or a folder deletes the works only it held, and every
account's positions and notes on them, as the dialog's counts say. **No
removal, move, revoke or emptied list widens a restricted account to every
library.**

## 2. The store

- Migration 017 adds `user.every_library INTEGER NOT NULL DEFAULT 1`, checked
  to 0 or 1. It sets 0 for every account that holds a `user_library` row and
  leaves 1 for the rest, so no account's view changes.
- A new account starts with 1, as today.
- An account with 0 sees the libraries its rows name, and **0 with no rows sees
  nothing**.
- The migration drops `only_grant_refuses_delete` and
  `only_grant_refuses_move`.
- Admins are unchanged: they see every library, and their column and rows wait
  for a demotion.

## 3. Access inside the server

- `Principal.roots` becomes `Principal.access`, an `account::Access` with two
  cases: `Every`, or `Only` these folders, where `Only` with no folder sees
  nothing. The type has no `Default`, so every place that builds one says
  which case it means, and `opds::granted()`'s missing principal stops
  meaning "everything".
- Each site in §0 that reads the empty list as unrestricted is changed by the
  compiler finding it, not by a grep. An "only, none" access must produce SQL
  that matches nothing (a false clause), never a clause left out.
- **The invariant:** no list of folders, empty or not, stands for "every
  library" anywhere in the server or the web client.
- `account::access_for`, in place of `roots_for`, answers it from the column
  and the rows. `revoke_library` revoking the last grant leaves "only, none".

## 4. What goes

- `removal_refusal` and `library_removal_refusal`, which held only the
  only-grant refusal, and `refused_before`'s only-grant refusal. The 409 for a
  scan reserved on the library (`scan_jobs::removal_waits_for`, O196) stays.
- `only_grant_holders` stays: it now feeds the dry run's list of accounts
  left with no library (§5). `only_grant_of`'s advice sentence goes.
- The two triggers (§2).
- O205's guard. Granting a library with no folder grants nothing: an "only"
  account then sees nothing more, which is no longer a widening. Nothing
  public makes such a library anyway (`scan.rs` refuses a library without a
  folder), so the guard and its 409 go together.

## 5. Remove and Move

- The dry run still runs first. It now reports the accounts that will be
  left with no library: those with role `user` and `every_library = 0` whose
  only grant is the library being removed, or the library that removing or
  moving its last folder deletes. An administrator is never named, nor is an
  account set to every library that still holds a row.
- `RemoveLibrary`, `RemoveRoot` and `MoveRoot` name them below the counts
  (`libraries.ts`'s `noLibrarySentence`). `RemoveLibrary`'s reads: "alice can
  see only this library. After this, alice will see no library until an
  administrator grants one." `RemoveRoot` and `MoveRoot` say "this folder's
  library". Several accounts are named together, and the second sentence says
  "they". A single account is named again rather than given a pronoun.
- When the dry run succeeds, the confirm is offered whether or not it names
  anyone, and the real request does what the dry run described. A dry run the
  server refuses draws its sentence and no confirm, and `RemoveLibrary` holds
  its confirm while a folder is busy.

## 6. An account that sees no library

- `/api/library/status` says so, so the web can tell this from a server with
  no library. Home and the library page draw a new empty state: "No library
  has been shared with this account. An administrator can grant one in
  Configuration → Users." ("Administrator" matches the existing empty
  states.) On Home, an account that sees every library still gets the existing
  "No library yet" on a server with none.
- The sidebar lists no library. Search finds nothing. OPDS feeds stay valid
  but list no work and no library.
- A link to a work answers 404, as for any work outside an account's grant.

## 7. The Users card, its API and the CLI

- `PATCH /api/library/users/{id}` takes `{"every_library": true}` to lift the
  limit, or `{"libraries": [ids]}` to set it, where **`[]` now means no
  library**. Sending `libraries` sets "only these". A body with both keys is
  refused with 400. `user_json` sends `every_library` beside `libraries`.
- The card's "Only these" can have nothing ticked. Clearing the last tick
  saves, and the card of an account that is not an administrator says "This
  account sees no library."
- Choosing "Only these" on an "Every library" account saves at once as no
  library, so the account sees nothing until a box is ticked. That is the safe
  direction, and every tick already saves at once.
- Choosing "Every library" on an account with a limit still asks first (O195).
- CLI: `revoke-library` of the last grant says the account now sees no
  library. A new `library user every-library <user>` lifts the limit, since
  revoking every grant no longer does. `grant-library` on an "every library"
  account narrows it to that library, as it does today. `list` prints "every
  library", "granted: …" or "no library" for an account that is not an
  administrator.

## 8. Tests

- Each site in §0 gets a test with an "only, none" account that asserts it
  sees nothing. Each test is watched failing with that site hand-reverted to
  treat the account as unrestricted.
- The migration test: an account with rows becomes 0, one without stays 1, and
  each sees what it saw before.
- Removing an only-grant library, removing its last folder and moving that
  folder away each succeed, the dry run names the account, and the account
  then sees nothing (not everything).
- Web: clearing the last tick, switching to "Only these", the dialogs'
  sentence, and the new empty state, each against the server's answer.
- The usual gate, `scripts/mutants.sh --since main`, the SQLite 3.40.1 run,
  and a hand check on a scratch copy of the real library.

## 9. Docs

- `2026-10-05-libraries-design.md`'s statements that an empty grant is
  unrestricted point here.
- O220 closes. O205's and O193's rows each gain one line saying which part
  O220 reversed and why.

## 10. Not in this

- The Libraries screen redesign (O219).
- Any change to what an admin sees.
