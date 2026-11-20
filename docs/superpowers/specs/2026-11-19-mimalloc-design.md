# jemalloc as the server's allocator (O349)

Approved by the owner on 2026-11-19. Short design: the decisions below were
made in conversation; what is left to the builder is listed at the end.
Amended the same day by the owner's ruling on the purge delay, below, and
then by the owner's ruling for jemalloc in place of mimalloc, which
withdraws the purge delay.

## Intent

O349: memory taken by JPEG XL decodes stays resident after they end. On
2026-11-17 a release server reading all 185 pages of the Bisque Doll
original as JPEGs, four at a time, went from 46,356 kB resident at rest to
960,704 kB at its peak, held 956,560 kB 30 s after the last response and
754,880 kB about 4.5 minutes later (`docs/notes.md`, "JPEG XL pages: the
decoder, the clients and a real archive (2026-11-17)"). The image's runtime
is `debian:bookworm-slim`, so glibc's malloc, which keeps freed memory in
per-thread arenas. Nothing in taiyaku sets an allocator, calls
`malloc_trim` or `mallopt` (read in source, 2026-11-19). O57 met the same
class with password verifies.

The goal: memory freed after a burst goes back to the system soon after,
for every Rust allocation in the shipped binary.

## Approach

*Since the jemalloc ruling below, the allocator is jemalloc; each point here
holds for it as written for mimalloc, but the build, which that ruling
describes, and the licence: jemalloc's C is under a BSD-2-Clause-style
licence (its `COPYING`), and the `tikv-jemallocator` and `tikv-jemalloc-sys`
crates under MIT OR Apache-2.0 (`MIT/Apache-2.0` in their `Cargo.toml`).*

- The `mimalloc` crate as `#[global_allocator]` in the one binary,
  `crates/taiyaku-cli/src/main.rs`, which runs both the server and the CLI.
  The library crates' tests keep the system allocator; the cli crate's own
  tests run under mimalloc.
- C code (the bundled SQLite, libarchive) keeps glibc's malloc. That is
  safe only if nothing frees memory across the boundary; the builder
  confirms it.
- No cap on decode threads, no change to how many decodes run, no
  `malloc_trim`.
- mimalloc is MIT and builds its C (mimalloc 3 by the crate's default)
  through `cc`, which the image's builder stage (`rust:1-bookworm`) has.
  `deny.toml` checks advisories only, so it has no licence list to change.

## Ship criterion

Measured on the desktop, with release builds of main and of the branch, in
the same session, by the same run as the 2026-11-17 note: all 185 pages at
full size with Chrome 154's `Accept`, four at a time; resident memory at
rest, at peak, 30 s after the last response and 5 minutes after. Main is
measured again as the baseline rather than taken from the note.

- It ships if the branch's reading 30 s after the last response is within
  100 MiB of its own reading at rest, its peak is at most 10% above
  main's, and its run takes at most 10% longer than main's.
- If it fails any of the three, the work stops and the figures go to the
  owner. No allocator option is tuned in this PR but jemalloc's background
  threads, by the jemalloc ruling below.
- The NAS is not measured, as on 2026-11-17.

If it ships, O349 is closed in `docs/status.md` by the open-items ritual,
with the figures and a notes section.

## The purge delay (owner's ruling, 2026-11-19)

The first measurement failed criterion 1 and passed the other two
(`.superpowers/sdd/mimalloc-evidence/T2/verdict.md`): 30 s after the last
response the branch's two runs held 129,584 and 318,664 kB above their own
rest readings, against a limit of 102,400 kB. mimalloc 3.3.2 gives freed
memory back to the system only once its `purge_delay` has passed, 1000 ms
by default, and only when a later allocator call checks for it. A server
that gets no request after the burst may make no such call, and then keeps
the memory.

The owner ruled to set `MIMALLOC_PURGE_DELAY=0` in the image's runtime
stage, so that mimalloc gives memory back as it frees it, and to measure
again by the same run against the same three criteria, with the branch's
server given the setting the image gives it. If that measurement fails any
of the three, the work stops and the figures go to the owner, as before.

*Withdrawn by the jemalloc ruling below: the image no longer sets
`MIMALLOC_PURGE_DELAY`.*

## jemalloc in place of mimalloc (owner's ruling, 2026-11-19)

The second measurement, with `MIMALLOC_PURGE_DELAY=0`, passed criteria 1
and 2 and failed criterion 3
(`.superpowers/sdd/mimalloc-evidence/T2b/verdict.md`). 30 s after the last
response the branch's two runs held 79,324 and 52,860 kB above their own
rest readings, within the limit, but the branch's mean wall time was
59.855 s against main's 21.510 s: 2.78 times main's, against a limit of
1.10. With its default delay mimalloc gave memory back only on later
allocator calls, which an idle server may not make; with no delay it gave
memory back at a cost in time the criterion does not allow.

The owner ruled to try jemalloc in place of mimalloc, and to withdraw the
purge delay. jemalloc's background threads purge freed memory on a timer,
whether or not the server makes another allocator call. jemalloc is now
the chosen approach (approach 2 below), and mimalloc is withdrawn.

- `tikv-jemallocator` as `#[global_allocator]` where mimalloc was, with
  its `background_threads` feature, which compiles `background_thread:true`
  into jemalloc's defaults: an exported `malloc_conf` would need the unsafe
  code the workspace forbids.
- No other jemalloc option is set. In the jemalloc that `tikv-jemalloc-sys`
  bundles, `muzzy_decay_ms` is 0 by default, so a purge releases pages at
  once (`MADV_DONTNEED`, not the lazy `MADV_FREE`) and resident memory
  falls when it runs; `dirty_decay_ms` keeps its default of 10 s.
- jemalloc's symbols are prefixed (`_rjem_`), so C code keeps glibc's
  malloc, as it did under mimalloc.
- `tikv-jemalloc-sys` builds jemalloc's C with jemalloc's own `configure`
  and `make`, which the image's builder stage (`rust:1-bookworm`) has.

The ship criterion is unchanged: the same run against main, by the same
three criteria. If the branch fails any of them, the work stops and the
figures go to the owner.

## Approaches considered

1. **mimalloc (chosen first, withdrawn by the jemalloc ruling above).** Two
   crates in `Cargo.lock` and two lines; it covers every Rust allocation,
   not only JPEG XL decodes.
2. **jemalloc** (tikv-jemallocator) with its decay settings: a similar
   effect with a larger build and more settings to choose; no reason to
   prefer it. *Chosen by the jemalloc ruling above, after mimalloc failed
   one criterion with each setting.*
3. **glibc's `malloc_trim` after a burst, or `MALLOC_ARENA_MAX`:**
   glibc-only, needs a trigger for "after a burst" and unsafe FFI; capping
   arenas trades memory for contention.
4. **A cap on decode threads or concurrency:** lowers the peak, not what is
   kept. Deferred until a peak on the NAS shows a need.

## Non-goals

- No tuning of jemalloc's options but background threads, by the jemalloc
  ruling above; every other option keeps its default.
- No change to `cover::GENERATION_LIMIT`, `jxl::render_pool` or the
  fingerprint pass's workers.

## Left to the builder

The `tikv-jemallocator` version, and no feature beyond `background_threads`
unless one is needed to build; how the measurement harness is driven,
following the 2026-11-17 note; where the evidence is kept
(`.superpowers/sdd/mimalloc-evidence/`).
