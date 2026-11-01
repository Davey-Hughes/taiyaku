# Code history moved out of comments

Text moved verbatim out of code comments when they were trimmed to their why. Each entry names the file and the line of code the comment sat above.

## `crates/taiyaku-library/src/derive/tests.rs`, above `assert!(path.ends_with("Series/Extras/Art Book"), "{path}");`

over this project's own corpus it turned 107
members into 40, collapsing `incoming/bocchi`'s five editions into a single
439-page member.

## `crates/taiyaku-library/src/derive.rs`, above `fn produces(&self, beneath: &BTreeSet<String>) -> Produces {`

Those came apart on `Series/Extras/Art Book`: the test used to be "a volume
or a chapter", per Step 7b's rule, while [`classify`] admitted a
volume-less entry as a member anyway — so the container and its
contents both became members of one work, five pages counted as ten,
and `suppressed` said `0`.

## `crates/taiyaku-library/src/derive.rs`, above `if !crate::migrate::absorbed(conn, work_id, crate::migrate::LIBRARY)? {`

Before this, `derive` wrote both unconditionally on
every pass: a hand-set `volume = 4.5` came back as `1.0` after one
derive, and a hand-renamed series minted a second row and moved the
works to it, leaving the renamed one empty.

## `crates/taiyaku-library/src/export/tests.rs`, above `#[test]`

`absorbed` is keyed `(work_id, document)` with no user, so an in-place export
as reader 1 — the *default*, needing no flag at all — found no notes and no
position for them, marked both documents stale, and had the "never absorbed"
guard satisfied by reader 2's own import. Reader 2's `taiyaku.notes.json` was
overwritten with an empty document and their `taiyaku.progress.json` deleted.

## `crates/taiyaku-library/src/preview.rs`, above `struct PartialOverride {`

**A struct rather than the tuple this was**, which clippy's own
`type_complexity` refused once the two values joined it

## `crates/taiyaku-library/src/scan.rs`, above `struct Resolved {`

a first scan of 156 series on network
storage passes that without difficulty

## `crates/taiyaku-library/src/scan.rs`, above `#[derive(Default)]`

[`measure_dir`] already
threaded `unreadable: &mut Vec<String>`, and exclusion adds two more
collections to a signature that was at its limit.
