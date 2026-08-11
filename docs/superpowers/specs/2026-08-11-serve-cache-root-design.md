# A cache root that does not follow the working directory — design

**Status: settled in brainstorming on 2026-08-11. Built, and shipped on
`serve-cache-root`.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This closes the **"The page cache follows the working directory, and now that
costs more"** entry under `status.md`'s "Later". It is not an O-number: it was
filed as a future decision rather than as a defect found by a review, so
closing it is an edit to that entry rather than the five-part open-items
ritual.

## Where it came from

`ServeState`'s `cache_dir` is a *relative* path, set in one place:

```rust
// crates/taiyaku-serve/src/lib.rs:239
cache_dir: std::path::PathBuf::from(".taiyaku-cache/covers"),
```

So where the cache lands depends on where `taiyaku serve` was launched from.
That was cheap while the directory held covers alone. It is not cheap now:
since `?w=` shipped, the sibling `thumbs` directory holds every reading-size
page as well as every filmstrip tile, and a cold page costs the server a decode
and a resize at **126 ms median against 10 ms warm**, measured over 235 real
generations and recorded in `status.md`. Starting the server from a different
directory therefore produces a second cache and regenerates a whole volume.

Found by noticing 120 MB of pages under `web/` after a session that had
launched `serve` from there.

## 1. One field drives both caches, which is the reason this is small

`thumb_cache_dir()` is derived rather than stored:

```rust
// crates/taiyaku-serve/src/lib.rs:70
pub fn thumb_cache_dir(&self) -> std::path::PathBuf {
    self.cache_dir.with_file_name("thumbs")
}
```

`with_file_name` replaces the last component, so `<root>/covers` becomes
`<root>/thumbs` whatever `<root>` is. Every consumer reaches the disk through
one of those two paths — `opds.rs:344` for a cover, `routes.rs:437` for a sized
page, `routes.rs:509` for a tile — and each is handed a directory rather than
building one. So moving the root is a change to one line plus the code that
computes what that line is set to, and no call site moves.

**The leaf names stay `covers` and `thumbs`.** Renaming them would be a second
change riding along on the first, and the names are already load-bearing: the
sibling relationship is what `thumb_cache_dir()`'s doc comment argues for, on
the grounds that only one of the two caches is cheap to rebuild.

## 2. Where it lands: `$XDG_CACHE_HOME/taiyaku`, falling back to `$HOME/.cache/taiyaku`

Two alternatives were considered and both lost.

**Per work, beside the sidecar** — the shape `stump::cache::path_for` already
uses, resolving to `<sidecar-dir>/.taiyaku-cache/covers`. It has the strongest
precedent in the crate: the Stump resolution cache and the progress file both
live beside the sidecar, and it would keep the server's cache in the same tree
as the CLI's fingerprint cache. It lost on a mismatch `status.md`'s own
suggestion did not account for. `stump::cache::path_for` takes a `&Work` and is
called per work; `cache_dir` is **one directory for the whole server**, and
`serve` is handed N sidecars that may sit in N directories. There is no single
sidecar to resolve against. Making the cache per-work instead would turn a
field into a function of the work and change all four call sites — and it would
write into the corpus tree, which is on a NAS.

**Canonicalised against the first sidecar** — `works[0].path.parent()`, one
line, no signature change, no new flag. It lost because which sidecar wins is
arbitrary: `serve a.json b.json` and `serve b.json a.json` would produce
different caches for the same library, which trades a surprise about the
working directory for a surprise about argument order. It also still writes
into the corpus.

What ships is a **platform cache root**, which is immune to both the working
directory and the argument order. Under it, `covers/` and `thumbs/`, unchanged.

**Read from the environment directly, not via `std::env::home_dir()`.**
`home_dir()` was deprecated for years and un-deprecated in **1.85**, which is
this workspace's exact `rust-version`. It compiles under `#![deny(deprecated)]`
on this machine's 1.97.1 — verified, not recalled — but that says nothing about
1.85, and no 1.85 toolchain is installed here to check against. Reading
`$XDG_CACHE_HOME` and then `$HOME/.cache` is what the XDG base directory
specification says to do regardless, so the dependency is avoided rather than
merely worked around. A `dirs`-style crate was not considered seriously: this
workspace's dependency list is short enough to be quoted in `status.md`, and
this is five lines.

**Cost, stated rather than hidden:** the cache root and the `.taiyaku-cache`
convention diverge. A reader who knows `docs/sidecar.md` will look in
`.taiyaku-cache/covers` and find nothing. §6 is what that costs in
documentation.

## 3. Precedence, and a fallback that says so

```
--cache DIR  >  $XDG_CACHE_HOME/taiyaku  >  $HOME/.cache/taiyaku  >  .taiyaku-cache
```

The last is today's behaviour, kept as a fallback for the case where neither
variable is set.

**It is a fallback rather than a refusal**, and the argument is a precedent in
the same file. `cover.rs` already treats a cache directory that cannot be
created as non-fatal — `a_cover_survives_a_cache_directory_that_cannot_be_created`
is a test — because a cache is derived data and a page can always be
regenerated. A server that refuses to start because `$HOME` is unset would
convert a recoverable state into an unrecoverable one, which is the same trade
`stump::cache::load` refuses two files away.

**But it prints, once, on stderr.** The rule is already written down forty
lines above the site being changed — `lib.rs:199`, against the `cache_dir` line
at 239 — where `--refresh-stump` without
`--stump-url` says so rather than doing nothing: *"A flag that quietly does
nothing is worse than one that refuses: the person who typed it goes on
believing the stale answers were thrown away."* A default that quietly puts the
cache somewhere surprising is that same failure, and it is the failure this
whole branch exists to remove — so shipping a silent version of it in the
fallback branch would be self-defeating.

No `TAIYAKU_CACHE` environment variable. `--stump-url` has a
`TAIYAKU_STUMP_URL` fallback because a key is a secret that does not belong in
shell history; a cache directory is neither secret nor typed often, and
`$XDG_CACHE_HOME` is already the environment variable for this.

## 4. `--cache DIR` on `serve`, naming the root

`serve` is the only one of the four commands with a cache that cannot be
pointed anywhere: `align`, `verify`, `credits` and `duplicates` all take
`--cache`, and all four default it to the same relative `.taiyaku-cache`. Adding
it here is closing a gap rather than inventing a flag.

**It names the root, not the leaf.** `align --cache .taiyaku-cache` names the
directory the fingerprint cache is built under, so `serve --cache DIR` deriving
`DIR/covers` and `DIR/thumbs` is the same convention rather than a second one.
The alternative — naming `DIR` as the covers directory, matching the field's
current value — would make `--cache` and `thumb_cache_dir()` disagree about
what the argument means, and would put the tiles in a *sibling* of what the
user typed.

Four touch points. Three are in `taiyaku-cli/src/main.rs`: the usage line
(116), the `reject_unknown_flags` list (1097), and an `option_value` read
beside the existing `--addr` (1112). The fourth is
`taiyaku_serve::serve`, which gains a sixth parameter — under
`clippy::too_many_arguments`, which fires above seven, so the pedantic gate is
unaffected and no config struct is warranted yet.

## 5. Shape, so the decision is testable

```rust
/// What a cache root resolved to, and whether anything had to be assumed.
enum Resolution { Xdg(PathBuf), Home(PathBuf), Flag(PathBuf), Fallback(PathBuf) }

fn cache_root(flag: Option<&Path>, xdg: Option<&OsStr>, home: Option<&OsStr>) -> Resolution
```

Two properties, each with a reason.

**Pure in its inputs.** `serve()` does the `env::var_os` reads and passes the
results in. Tests then cover all four precedence branches without touching the
process environment — which in edition 2024 is `unsafe`, and which under
nextest's threads is racy besides. The alternative, a function that reads the
environment itself plus tests that set it, was rejected on both counts.

**The fallback is a variant, not an `eprintln!`.** `Resolution` carries which
branch fired so the caller decides whether to warn, and a test can assert the
branch. This is the shape mutation testing has already forced twice in this
workspace: `notes.md` records `reqlog`'s warn-once latch and `inventory`'s table
loop as survivors precisely because each was a decision inside a print
statement, with no return value any test of the component's output could see.
Writing the fourth branch as a bare `eprintln!` would file a fifth survivor by
hand.

**That argument turned out to reach one step further than this section did,
and the gap was found while building rather than while designing.** Carrying
the rule as a variant only moves the decision as far as the caller — and the
caller's half of it was still a bare `if let … { eprintln!(…) }` inside
`serve`, with nothing asserting that it prints on `Fallback` or stays silent
on the other three. A mutant deleting that block survived the suite, which is
precisely the failure this section cites `reqlog` and `inventory` for. So the
message itself is a returned value too — `notice(&Resolution) -> Option<String>`,
`Some` only for `Fallback` — and all four arms are asserted. The tests were
checked by mutating each arm by hand and confirming the assertion fails, not
by observing that they pass.

**One residue remains and is stated rather than papered over.** Deleting the
`if let Some(notice) = … { eprintln!("{notice}") }` from `serve` still survives,
because nothing captures `serve`'s stderr. Closing that needs a writer
parameter on `serve`, which is a larger change than this bought — and the
crate already carries the same residue at `lib.rs`'s `skipping_notice`, which
this now matches in shape rather than diverging from.

## 6. What this makes false, and what it retires

**This section said "five documentation sites" and was wrong twice.** The plan
found a sixth before any code changed, and the branch found two more while
running. The real figure is **one in `sidecar.md`, seven in `status.md`, plus
this document's own status line** — and the drift is left on the record rather
than quietly corrected, because it is the same failure `status.md`'s open-items
preamble has now logged three times against its own count word: a number
written once and not re-derived when the thing it counts grows.

What found the extra three is worth more than the number. None came from
reading the design again. The sixth came from running the plan's own
verification grep against the *unmodified* tree, which surfaced a passage the
design had not looked at. The seventh and eighth came from a reviewer reading
the corrected paragraph end to end and noticing it now contradicted itself
35 lines further down. A per-site list is checkable; a count is not.

The sites below are the five this design identified. The three it missed are
`status.md`'s M5a tile-cache measurement, its "what would reopen it" referent,
and its closing "the count of caches sharing that tree is still two" — all
three inside the same O11 paragraph, which is the paragraph this change
touches hardest.

Four of the five are corrections; the fifth is a warning that stops being
needed, which is worth naming rather than silently deleting.

- `docs/sidecar.md:70-71` — *"`taiyaku serve` keeps a second cache beside it, at
  `.taiyaku-cache/covers`"*. Corrected.
- `docs/status.md:206` — the `serve` usage line gains `--cache DIR`.
- `docs/status.md:1255` — *"relative to the server's working directory"*.
  Corrected, and it is the sentence that describes the defect.
- `docs/status.md:3489-3499` — the "Later" entry becomes a shipped record,
  kept rather than deleted, for the reason that section already gives.
- `docs/status.md:1265-1269` — *"two different caches share that tree… Deleting
  the parent throws away both, and only one is cheap to rebuild."* **This stops
  being true.** After the change `.taiyaku-cache/` holds the CLI's fingerprint
  cache alone, so a person clearing it can no longer throw away the expensive
  half by accident. The paragraph should record that the hazard was retired by
  this change rather than simply vanish, since a warning that disappears without
  explanation reads as an oversight.

**O11 itself stays open.** This branch changes *where* the cache lives, not
whether it grows or is ever pruned, and O11 is about the latter. Its measured
figures — 165 KB of covers, 1.12 MiB of tiles per 195-page edition — are
unaffected by a move.

## 7. Testing

Unit, in `taiyaku-serve`:

1. `cache_root` returns `Flag` when a flag is given, whatever the environment
   holds — including when both variables are set.
2. `Xdg` when no flag and `$XDG_CACHE_HOME` is set, and the path ends
   `taiyaku`.
3. `Home` when no flag and only `$HOME` is set, and the path ends
   `.cache/taiyaku`.
4. `Fallback` when neither is set, at the relative path today's code uses.
5. `thumb_cache_dir()` is a sibling of `cache_dir` under a resolved root —
   the existing invariant, re-asserted at the new root rather than assumed to
   survive.

Integration, in `crates/taiyaku-cli/tests/cli.rs`, which already spawns the
binary:

6. `serve --cache` with no value is refused, exits 1, and leads stderr with
   `taiyaku: ` — the contract that file already asserts for other refusals.
7. An unknown flag is still refused after `--cache` joins the accepted list,
   which is the regression `reject_unknown_flags` exists to prevent.

**No test asserts a real home directory.** A test that reads `$HOME` passes on
this machine and says nothing about any other, which is the shape of fixture
`notes.md` keeps recording as unable to fail.

**Seven was the design's figure; twelve shipped**, and the same rule as §6
applies — the list is checkable, the count was not. Two more came from §5's
`notice`, covering its four arms between them — `the_pinned_rules_are_silent`
takes the three that say nothing, `the_fallback_names_the_path_and_the_flag`
the one that speaks — two from pinning claims the design had assumed
(`covers` is a leaf under the root, and an empty variable is unset — the
second of which is the one that would have let the defect back in silently,
since `Path::new("").join("taiyaku")` is relative), and one advertises the
flag in the usage text. 7 + 2 + 2 + 1 = 12. That breakdown put *three* against
`notice` until a review re-added it: one addend over its own total, in the
paragraph that had just cited §6 on lists being checkable where counts are
not, and one section after §6 puts count-drift on the record as this
repository's recurring failure. A thirteenth was not written but *repaired*:
`taiyaku-cli`'s existing `tests::serve_refuses_an_unknown_flag` asserts
`serve`'s flag alphabet as an exact string, so adding `--cache` broke it.
Neither this design nor the plan named that test; `cargo test --workspace`
did.

## 8. What is deliberately not done

**No migration.** Existing `.taiyaku-cache/covers` and `.taiyaku-cache/thumbs`
contents are left where they are. Both are content-addressed and gitignored, a
rebuild costs 126 ms a page against a cache measured at 165 KB and 1.12 MiB per
edition, and a migration is code that runs once and is then wrong forever. The
old directories can be deleted by hand; nothing reads them after this change.

**No pruning**, per §6 — that is O11.

**No change to the CLI's `.taiyaku-cache` default.** `align` and the other three
write a fingerprint cache that is genuinely per-corpus-tree and that a person
runs from the directory they are working in. Moving it is a separate question
with a separate answer, and bundling it would put an unrelated behaviour change
in this branch.

## 9. What could not be measured

Whether `std::env::home_dir()` is deprecated at the workspace MSRV of 1.85. No
1.85 toolchain is installed on this machine and none was installed to find out,
because §2's decision makes the answer irrelevant — the environment is read
directly either way. Recorded so that a later branch reaching for `home_dir()`
knows the question was left open rather than answered in its favour.
