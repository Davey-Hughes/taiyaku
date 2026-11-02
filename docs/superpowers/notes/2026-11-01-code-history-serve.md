# Code history moved out of comments

Text moved verbatim out of code comments when they were trimmed to their why. Each entry names the file and where the comment sat in it.

## `crates/taiyaku-serve/src/cbz/tests.rs`, at the end of the file

`every_structural_boundary_is_clipped_correctly_on_both_sides` and
`every_split_of_the_output_reassembles_into_the_whole` used to live here,
driving `emit_range` — a hand-written second copy of the header/payload/
trailer walk `download.rs::generated` performs, kept only in this test
file. Whole-branch review, 2026-08-24 (Finding 3): that duplication is why
a real off-by-one in `generated`'s own walk (the `plan.local_header(index)`
placed before its `send` call, one byte off within a range window) passed
the whole suite — `emit_range` was never mutated, so it kept agreeing with itself regardless
of what the route actually did.

## `crates/taiyaku-serve/src/auth.rs`, above `Password,`

Folding the two into one `Header` variant made the account routes refuse
a correct password with a message asserting an API key had been used —
and left no non-browser path to mint a first key or change a password at
all.

## `crates/taiyaku-serve/src/config_api.rs`, above `pub(crate) async fn delete_root(`

because this task's own interface list caps it at four

## `crates/taiyaku-serve/src/config_api.rs`, above `const MAX_PROPOSED_RULES: usize = 200;`

(10 extra rules 47 ms, 40 → 401 ms,
80 → 1.34 s, 160 → 5.01 s, debug build, one-work fixture)

## `crates/taiyaku-serve/src/auth/tests.rs`, above `let expected = [`

`patterns.len() == route_count() - open_routes().len()` — which is `G == G`, since `route_count` is *defined* as guarded plus open.

## `crates/taiyaku-serve/src/curation.rs`, above `use std::sync::Arc;`

Mapping that
`Err` to a 400 answered `400 Bad Request :: reading scan entries for the
dry run: no such table: scan_entry` — measured, on the route a
rule-editing form calls at every keystroke: a client told to fix its input
while the database was broken.

## `crates/taiyaku-serve/src/stump/resolve.rs`, above `pub fn resolve_work(`

and, before this
fix, a single semantic refusal anywhere in a work silently degraded that
entire work to local-only, forever, on every start.
