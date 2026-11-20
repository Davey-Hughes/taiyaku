# JPEG XL pages: a cap on the embedded colour profile (O350) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A JPEG XL page whose header declares an encoded ICC profile above 1 MiB is refused before jxl-oxide parses it, in the image hook and in `jxl::rebuilt_jpeg`, and O350 is closed.

**Architecture:** One check of taiyaku's own in `crates/taiyaku-serve/src/jxl.rs`, beside `metadata_fits`: join the page's codestream through the existing `boxes`, parse `jxl_oxide::ImageHeader`, and where the colour encoding wants an ICC profile read `enc_size` as a U64 and refuse it above the cap. Both call sites refuse as they refuse a metadata bomb. Tests run on hand-built codestreams of 21 to 23 bytes from a new test-only module; two tests in a process of their own read peak resident memory to show the zero-fill never happens.

**Tech Stack:** Rust; jxl-oxide 0.12.6 (`ImageHeader`), jxl-bitstream 1.1.0 (`Bitstream`, `ContainerParser`), jxl-oxide-common 1.0.0 (`Bundle`), image 0.25.9; cjxl/djxl 0.12 for one fixture.

**Spec:** `docs/superpowers/specs/2026-11-19-jxl-icc-cap-design.md` (approved; read it first, it is short).

**Worktree:** `/home/davey/projects/taiyaku/.claude/worktrees/jxl-icc-cap` (below, `$W`), branch `jxl-icc-cap`. **Evidence:** `/home/davey/projects/taiyaku/.superpowers/sdd/jxl-icc-cap-evidence/` (below, `$EV`), in the main checkout, never committed and never under `target/`. `$EV/plan-probe/` holds the scratch crate that checked this plan's bytes against jxl-oxide, and its output.

## Global Constraints

- The cap: an `enc_size` of 1,048,576 passes; 1,048,577 is refused. Tests pin it with literals (`1 << 20`, `(1 << 20) + 1`, `1 << 28`), never with the constant they test.
- A refusal is the metadata bomb's: the hook returns `ImageError::Limits(LimitError::from_kind(LimitErrorKind::InsufficientMemory))`, `rebuilt_jpeg` returns `None`.
- A header that ends before `enc_size` passes the check; so does a header that does not parse, and a page that wants no profile.
- No change to `METADATA_CAP`, the dimension limit, `max_alloc` or any dependency; no change to jxl-oxide.
- Comments say why, never history or measurements; no comment block over 8 lines; no date, PR number or 7+ character hex run in a comment (`scripts/check-comments.mjs`). Byte values live in code, not comments.
- Shell: no `cd` (it is zoxide); use `git -C`, `env -C` and absolute paths. zsh: `grep` is a function that skips binary files, so use `rg -a`; `>` is noclobber, so write logs with `>|`. Every test run takes `--test-threads=8`; another branch builds on this machine.
- Do not carry the spec's re-parse estimates ("about 128 MiB", "about 32 GiB") into `docs/status.md` or `docs/notes.md`: they assume one input byte per encoded byte, while a prefix-coded symbol can take up to 15 bits plus extra bits, and nothing here measures them.
- Docs name no branch commit; they describe the commit or give its UTC time.

## Review Focus

1. **An ordinary page with no profile** (nearly every page) must be untouched, even where the bits after its header would read as a huge size. Pinned by Task 1's `a_page_that_wants_no_colour_profile_is_not_read_for_one` and the existing `every_fixture_decodes_through_image_reader_as_djxl_does`.
2. **A page with a real, non-sRGB profile** must still decode. Pinned by Task 2.
3. **A page cut short inside its header or inside the declared size** must pass the check and be refused, as today, as a broken page, not as one over budget. Pinned by Task 1's `a_header_that_ends_before_its_profile_size_passes`.
4. **A container whose codestream is split across `jxlp` boxes**, with the header and the size each straddling two boxes, must be read as one codestream. Pinned by Task 1's shape tests (`every_shape`).
5. **The size asked before a decode** (`cover::fits_within` asks dimensions) must be refused before jxl-oxide reads the header. Pinned by Task 1's `the_size_of_a_page_declaring_a_colour_profile_over_the_cap_is_refused`.

A client whose `Accept` names `image/jxl` at full size is sent the stored bytes and never reaches the hook; no code in this plan touches that path, and `routes::tests::jpeg_xl_by_what_the_client_accepts::a_jpeg_xl_page_is_sent_as_stored_to_a_client_that_names_it` already pins it.

## The hand-built codestream

Worked out from jxl-image 0.13.0, jxl-coding 1.0.1 and jxl-color 0.11.0, and run against jxl-oxide 0.12.6 before this plan was written (`$EV/plan-probe/output.txt`). Bits are written least significant first, each field's value in its bit count, then zero-padded to a byte.

| Bits | Value | Field |
|---|---|---|
| 16 | 0x0AFF | signature (bytes `ff 0a`) |
| 1, 5, 3 | 1, 0, 1 | `SizeHeader`: div8, h_div8 − 1 (8 px high), ratio 1 (as wide) |
| 1, 1 | 0, 0 | `ImageMetadata`: all_default, extra_fields |
| 1, 2 | 0, 0 | bit_depth: integer, selector 0 (8 bits) |
| 1 | 1 | modular_16bit_buffers |
| 2 | 0 | num_extra selector 0 (none) |
| 1 | 0 | xyb_encoded |
| 1, 1, 2 | 0, 1, 0 | colour_encoding: all_default 0, **want_icc 1**, colour_space RGB. *The no-profile page writes instead one bit, 1 (all_default, sRGB).* |
| 2 | 0 | extensions (U64 0) |
| 1 | 1 | default_m (header ends: bit 41, or 38 without a profile) |
| U64 | enc_size | selector 3 (2 bits), low 12 bits, then per further byte a 1 bit and 8 bits, then a 0 bit |
| 1 | 0 | lz77 off |
| 1, 2 | 1, 0 | simple context map, 0 bits per entry: all 41 contexts in one cluster |
| 1 | 1 | prefix codes |
| 4 | 15 | split_exponent 15: each token is its own value |
| 1, 4, 7 | 1, 7, 0 | alphabet of 1 + 2^7 + 0 = 129 symbols |
| 2, 2 | 1, 3 | simple prefix code (hskip 1), four symbols |
| 8 × 4 | 0, 1, 2, 128 | the symbols |
| 1 | 0 | tree_select 0: two bits each |
| 2 × 18 | codes | 128, 128, 128, 128, 1 (output_size 2^28), 0 (commands_size), then twelve 0 |

Each symbol's code is written as a 2-bit value: symbol 0 → 0, 1 → 2, 2 → 1, 128 → 3. The 18 symbols pass all of `read_icc`'s checks for every `enc_size` used here, so without the check jxl-oxide reaches the resize, fills it, then runs out of bits.

Expected bytes (the fixture test asserts the first two):

- `declaring(1 << 28)`, 23 bytes: `ff 0a 41 40 08 07 80 00 01 06 90 ff 00 0d 10 20 00 e8 5f 00 00 00 00`
- `wanting_no_profile()` (the same U64 of 2^28 after an all-default colour encoding), 22 bytes: `ff 0a 41 40 e4 00 10 20 c0 00 f2 1f a0 01 02 04 00 fd 0b 00 00 00`
- `declaring(1 << 20)`, 21 bytes: `ff 0a 41 40 08 07 80 00 03 c8 7f 80 06 08 10 00 f4 2f 00 00 00`; `(1 << 20) + 1` differs only in byte 5, `0f`.

Shapes: `in_jxlc(cs)` is `JXL_CONTAINER` then one `jxlc` box holding `cs`. `in_jxlp(cs)` is `JXL_CONTAINER` then three `jxlp` boxes, payloads `00 00 00 00` then `cs[..4]`, `00 00 00 01` then `cs[4..8]`, and `80 00 00 02` then `cs[8..]`: the header (bytes 0–5) straddles boxes one and two, `enc_size` (bytes 5–10) boxes two and three. No `ftyp` box: jxl-oxide 0.12.6 does not ask for one. Probe: jxl-oxide's own read of the 2^28 page, in all three shapes, raised peak resident memory by about 268 MB and failed "reader ended before parsing image header"; through `integration::JxlDecoder` the same page fails as `ImageError::Decoding`, which is what tells the cap's refusal (`Limits`) from jxl-oxide's.

## Files

- Create `crates/taiyaku-serve/src/jxl/icc_pages.rs`: the hand-built pages (test-only).
- Modify `crates/taiyaku-serve/src/jxl.rs`: module doc (lines 1–8), `ICC_CAP` after `METADATA_CAP` (line 30), the hook's check (lines 61–67), `rebuilt_jpeg` (line 192), `codestream` and `icc_fits` after `metadata_fits` (ends line 228), `#[cfg(test)] mod icc_pages;` beside the other test modules (lines 280–283).
- Modify `crates/taiyaku-serve/src/jxl/tests.rs` (new tests after `an_empty_box_ahead_of_a_bomb_does_not_hide_it`, and one row in `every_fixture_decodes_through_image_reader_as_djxl_does`) and `crates/taiyaku-serve/src/jxl/process_tests.rs` (two tests at the end).
- Modify `crates/taiyaku-serve/fixtures/jxl/make.sh`, add `fixtures/jxl/icc.jxl` and `icc.djxl.png`, list them in `test_support::jxl_fixture` (`crates/taiyaku-serve/src/test_support.rs:4022`).
- Modify `docs/status.md`, `docs/notes.md`.

---

### Task 1: The cap, its two call sites, and the pages that prove it

**Files:** create `jxl/icc_pages.rs`; modify `jxl.rs`, `jxl/tests.rs`, `jxl/process_tests.rs`.

**Interfaces:**
- Produces, in `jxl/icc_pages.rs` (all `pub(super)`): `fn declaring(enc_size: u64) -> Vec<u8>` (a bare codestream wanting a profile of that encoded size); `fn wanting_no_profile() -> Vec<u8>`; `fn in_jxlc(codestream: &[u8]) -> Vec<u8>`; `fn in_jxlp(codestream: &[u8]) -> Vec<u8>`; `fn every_shape(codestream: &[u8]) -> [(&'static str, Vec<u8>); 3]` (`"bare"`, `"jxlc"`, `"jxlp"`). A private bit writer and one private builder taking `(wants_profile: bool, enc_size: u64)` sit behind them.
- Produces, in `jxl.rs` (private): `const ICC_CAP: u64 = 1024 * 1024;`; `fn codestream(page: &[u8]) -> std::borrow::Cow<'_, [u8]>`; `fn icc_fits(page: &[u8]) -> bool`.

- [ ] **Step 1: Write the pages and their fixture test.** Build `icc_pages.rs` from the table above; declare it in `jxl.rs`. In `tests.rs` add `the_hand_built_pages_declare_what_they_say`, which checks the pages apart from the code under test:
  - `declaring(1 << 28)` and `wanting_no_profile()` equal the byte arrays above (as `[0xff, 0x0a, …]` literals);
  - for `enc_size` in `[1 << 28, 1 << 20, (1 << 20) + 1]`: `jxl_oxide::ImageHeader::parse(&mut Bitstream::new(&page), ())` succeeds with `want_icc()` true and the next `read_u64()` is `Ok(enc_size)`; for `wanting_no_profile()` `want_icc()` is false and the next `read_u64()` is `Ok(1 << 28)`;
  - for each shape of `declaring(1 << 28)`, jxl-oxide's own container reading (`jxl_bitstream::ContainerParser::new().feed_bytes(&page)`, joining every `ParseEvent::Codestream` payload) gives back the bare codestream, and `in_jxlp`'s page holds exactly three `jxlp` boxes.

- [ ] **Step 2: Run it.** `mkdir -p $EV`, then `cargo test -p taiyaku-serve --lib jxl:: -- --test-threads=8 >| $EV/task1-pages.log 2>&1` (from `$W`: `env -C $W cargo …`). Expected: the new test passes (it checks the pages, not the cap); every other `jxl::` test still passes.

- [ ] **Step 3: Write the two process tests and reproduce the defect.** In `process_tests.rs` add `fn peak_resident_bytes() -> u64` (the `VmHWM:` line of `/proc/self/status`, kB × 1024) and `const ZERO_FILL: u64 = 1 << 28;`, and two tests, each run through `alone` with its own name and `register()` first:
  - `a_page_declaring_a_huge_colour_profile_is_refused_before_jxl_oxide_fills_it`: page `declaring(ZERO_FILL)`; decode it through `ImageReader::new(Cursor::new(&page)).with_guessed_format()…decode()`; assert it is an `Err` and the peak rose by less than `ZERO_FILL / 2` (message: "the hook let jxl-oxide fill the declared profile: peak resident memory rose by {rise} bytes"). Then the control, measured afresh: `super::read_page(&page, usize::MAX)`, jxl-oxide with no check, is an `Err` and raises the peak by at least `ZERO_FILL / 2` ("the control: …").
  - `a_page_declaring_a_huge_colour_profile_is_not_rebuilt_and_never_filled`: the same with `in_jxlp(&declaring(ZERO_FILL))` and `rebuilt_jpeg(&page)`, which must be `None` with the peak below `ZERO_FILL / 2`; then the same control.

  Run `cargo test -p taiyaku-serve --lib jxl::process_tests::a_page_declaring -- --test-threads=8 >| $EV/reproduction.log 2>&1`. **Expected: both FAIL at their first memory assertion, the rise about 268 MB** (2^28 plus the allocator's change). This is the spec's reproduction, through the current hook and the current `rebuilt_jpeg`; keep the log, Task 3 quotes it.

- [ ] **Step 4: Write the remaining tests (they will not compile yet).** In `tests.rs`, through `icc_pages` and the existing `decode`, `outcome` and `refused_over_the_cap`; each assertion names the shape and size in its message:
  - `a_colour_profile_declared_over_the_cap_is_refused_in_every_shape`: for `enc_size` in `[1 << 28, (1 << 20) + 1]`, every shape: `!icc_fits(&page)`.
  - `a_colour_profile_declared_at_the_cap_passes_in_every_shape`: `1 << 20`, every shape: `icc_fits(&page)`.
  - `a_page_that_wants_no_colour_profile_is_not_read_for_one`: `wanting_no_profile()`, every shape: `icc_fits(&page)`.
  - `a_header_that_ends_before_its_profile_size_passes`: `declaring(1 << 28)[..5]` (cut in the header) and `[..8]` (cut in `enc_size`): `icc_fits` is true.
  - `a_page_declaring_a_colour_profile_over_the_cap_is_refused`: after `register()`, for `[1 << 28, (1 << 20) + 1]` × every shape, `refused_over_the_cap(&decode(&page))`.
  - `the_size_of_a_page_declaring_a_colour_profile_over_the_cap_is_refused`: `into_dimensions()` of `declaring(1 << 28)`, as `the_size_of_a_page_whose_metadata_inflates_past_the_cap_is_refused` does it.
  - `a_page_declaring_a_colour_profile_at_the_cap_reaches_jxl_oxide`: `decode(&declaring(1 << 20))` in every shape is `Err(ImageError::Decoding(_))`: jxl-oxide's own refusal of a page with no image, not the cap's.

  Run the Step 2 command into `$EV/task1-red.log`. Expected: does not compile, `icc_fits` not found.

- [ ] **Step 5: Implement.** In `jxl.rs`:
  - `codestream`: a page that starts with `JXL_CONTAINER` gives, in box order through `boxes`, each `jxlc` payload and each `jxlp` payload from its fifth byte (`payload.get(4..).unwrap_or_default()`), joined; any other page is borrowed whole (a bare codestream, or bytes jxl-oxide refuses at the signature).
  - `icc_fits`: parse `ImageHeader` from `jxl_bitstream::Bitstream::new(&codestream(page))`; a parse error passes; a header whose `colour_encoding.want_icc()` is false passes; otherwise `read_u64()`: an error passes, `Ok(n)` passes when `n <= ICC_CAP`. The doc comment says why each passing case is safe: jxl-oxide stops at the same place, before the resize.
  - Hook: `if !metadata_fits(&page) || !icc_fits(&page)`, before `JxlDecoder::with_thread_pool`; the comment above it says jxl-oxide also reads the header's profile there. `rebuilt_jpeg`: the same condition. `ICC_CAP`'s doc: real profiles are a few KB, and jxl-oxide fills the declared size before reading a byte of it, again after every 4 KiB until the header is whole (O350). Add a sentence on the profile to the module doc.

- [ ] **Step 6: Run.** The Step 2 command into `$EV/task1-green.log`; then `rg -a 'test result|FAILED|panicked' $EV/task1-green.log`. Expected: every `jxl::` test passes, the two process tests included.

- [ ] **Step 7: Lint, then commit.** `env -C $W cargo fmt --all`, `env -C $W cargo clippy -p taiyaku-serve --all-targets -- -D warnings`, `env -C $W node scripts/check-comments.mjs` (all clean), then `git -C $W add crates/taiyaku-serve/src/jxl.rs crates/taiyaku-serve/src/jxl/` and `git -C $W commit -m "JPEG XL: refuse a page whose header declares an ICC profile over 1 MiB"`.

- [ ] **Step 8: Watch each test fail against the defect it names.** Apply each mutation below to `jxl.rs` alone, run the Step 2 command into `$EV/mutation-<id>.log`, confirm with `rg -a '^test .*FAILED' $EV/mutation-<id>.log` that **every** test in its row fails (and, for M4 and M5, that the other site's tests pass), then restore with `git -C $W checkout -- crates/taiyaku-serve/src/jxl.rs`. A test that survives its mutation is a finding: report it and fix the test, never the mutation.

  | Id | Mutation | Must fail |
  |---|---|---|
  | M1 | `ICC_CAP`'s `1024 * 1024` as `1024 + 1024` | `…at_the_cap_passes_in_every_shape`, `…at_the_cap_reaches_jxl_oxide` |
  | M2 | `n <= ICC_CAP` as `n < ICC_CAP` | the same two |
  | M3 | `icc_fits`'s body as `true` | `…over_the_cap_is_refused_in_every_shape`, `…over_the_cap_is_refused`, `the_size_of_…`, both process tests |
  | M4 | the hook's `\|\| !icc_fits(&page)` removed | `…over_the_cap_is_refused`, `the_size_of_…`, `…refused_before_jxl_oxide_fills_it` (the direct tests and the `rebuilt_jpeg` process test pass) |
  | M5 | `rebuilt_jpeg`'s `\|\| !icc_fits(page)` removed | `…is_not_rebuilt_and_never_filled` only |
  | M6 | `codestream` keeps a `jxlp` payload's 4-byte index | the jxlp shape of `…over_the_cap_is_refused_in_every_shape` and of `…over_the_cap_is_refused` |
  | M7 | `codestream` stops after the first `jxlc`/`jxlp` box | the jxlp shape of the same two |
  | M8 | `codestream` borrows every page whole (a container read as bare) | the jxlc and jxlp shapes of the same two |
  | M9 | `want_icc()` inverted | `…over_the_cap_is_refused_in_every_shape`, `a_page_that_wants_no_colour_profile_is_not_read_for_one` |
  | M10 | a `read_u64` error refuses | `a_header_that_ends_before_its_profile_size_passes` (the cut at 8) |
  | M11 | an `ImageHeader` parse error refuses | `a_header_that_ends_before_its_profile_size_passes` (the cut at 5) |

  After M11, `git -C $W status --porcelain` must print nothing. Keep a line per mutation (id, failing tests, first failure message) in `$EV/mutations.md`.

---

### Task 2: A page with a real colour profile still decodes

**Files:** modify `crates/taiyaku-serve/fixtures/jxl/make.sh`; add `fixtures/jxl/icc.jxl`, `fixtures/jxl/icc.djxl.png`; modify `src/test_support.rs` (the `fixtures!` list), `src/jxl/tests.rs`.

**Interfaces:** consumes `icc_fits` (Task 1); produces the fixture names `"icc.jxl"`, `"icc.djxl.png"`.

- [ ] **Step 1: Add the recipe to `make.sh`**, after the `trailing` block. Run as written in `$EV/plan-probe/mk-icc.sh`, twice, with byte-identical output (4,022 bytes, a 516-byte profile, `enc_size` 357, decoded by jxl-oxide identical to djxl's PNG):
  ```bash
  page 8 "$tmp/icc-page.png"
  enc -d 0 -x color_space=RGB_D65_DCI_Rel_SRG "$tmp/icc-page.png" "$tmp/p3.jxl"
  djxl --quiet "$tmp/p3.jxl" "$tmp/p3.png" --icc_out="$tmp/p3.icc"
  magick "$tmp/icc-page.png" -profile "$tmp/p3.icc" -define png:exclude-chunk=date,time PNG24:"$tmp/icc.png"
  enc -d 0 "$tmp/icc.png" "$out/icc.jxl"
  ```
  Its comment (why, at most three lines): libjxl's own Display P3 profile, embedded in a PNG; cjxl keeps a profile whole on a lossless page and turns it into an enum encoding on a lossy one. Do not put `"${png[@]}"` after `-profile`: `-strip` would drop it. Add `icc` to the `for f in … ; do ref "$f"; done` loop, and to the facts at the end: `jxlinfo "$out/icc.jxl" | grep -q 'ICC profile' || fail "icc.jxl carries no ICC profile"`, `same "$out/icc.djxl.png" "$tmp/icc.png" || fail "icc.jxl is not lossless"`, and a bare-codestream check like `lossy.jxl`'s.

- [ ] **Step 2: Make the two files.** `mkdir -p $EV/make-out`, `bash $W/crates/taiyaku-serve/fixtures/jxl/make.sh $EV/make-out` (exit 0), then copy only `icc.jxl` and `icc.djxl.png` into `$W/crates/taiyaku-serve/fixtures/jxl/`. `git -C $W status --porcelain` must list only `make.sh` and the two new files. Add both names to `jxl_fixture`'s list, in order after `"grey.jxl"`.

- [ ] **Step 3: Write the tests.** Add `("icc", 0)` to `every_fixture_decodes_through_image_reader_as_djxl_does`'s list (lossless, so exact). Add `a_page_with_a_real_colour_profile_passes_the_check`: the fixture checks first, apart from `icc_fits` (`icc.jxl` starts with `JXL_CODESTREAM`; after `register()`, `ImageReader…into_decoder()`'s `icc_profile()` is `Ok(Some(_))` and not empty), then `icc_fits(jxl_fixture("icc.jxl"))`.

- [ ] **Step 4: Run.** The Task 1 Step 2 command into `$EV/task2-green.log`. Expected: all `jxl::` tests pass.

- [ ] **Step 5: Watch them fail.** As in Task 1 Step 8, logs `$EV/mutation-M12.log` and `-M13.log`, restoring `jxl.rs` after each by `git -C $W checkout -- crates/taiyaku-serve/src/jxl.rs` (Task 1's work is committed): **M12** `ICC_CAP` as `16` (below any real profile); **M13** `icc_fits` refuses every page whose header wants a profile. Each must fail `a_page_with_a_real_colour_profile_passes_the_check` and `every_fixture_decodes_through_image_reader_as_djxl_does` (the `icc` row). Add them to `$EV/mutations.md`.

- [ ] **Step 6: Lint and commit.** As Task 1 Step 7; `git -C $W add` the four files and `commit -m "JPEG XL: a page with a real Display P3 profile still decodes"`.

---

### Task 3: Close O350 and run the gate

**Files:** `docs/status.md`, `docs/notes.md`. Read `/home/davey/.config/.claude/projects/-home-davey-projects-taiyaku/memory/taiyaku-open-items-ritual.md` first. Every figure written here is a claim a later check verifies: write few, each from a log in `$EV`.

- [ ] **Step 1: The sweep on the way in.** From `$W`: `rg -c '^\| O[0-9]' docs/status.md` (rows), `rg -c '^\| O[0-9]* \| \[x\]' docs/status.md` (closed), `rg -n '[0-9]{4} tests|Twenty-six of the' docs/status.md`, `rg -n -A1 '^What remains:' docs/status.md`. Expect 388 rows, 221 closed, "Six crates, 4335 tests", "Twenty-six of the 4335", "one hundred and sixty-seven open items". If main has moved, work from what the sweep prints.

- [ ] **Step 2: `docs/notes.md`.** Append a section at the end, `## JPEG XL pages: a cap on the embedded colour profile (O350, 2026-11-19)`: the hand-built page (23 bytes, built by `jxl/icc_pages.rs`, how it passes `read_icc`'s checks), the reproduction (the peak rise `$EV/reproduction.log` shows, for the hook and for `rebuilt_jpeg`, before the check; after it the same tests pass, which bounds the rise below 128 MiB), the cap and where it sits, the shapes, the real P3 fixture, and that the decoded size is not checked (the spec's approach 4). Name the mutations by id and test, with no counts beyond what the logs show.

- [ ] **Step 3: `docs/status.md`, the five parts.**
  1. The O350 row: `[x]`; the What cell opens "**Closed by the colour profile cap PR, 2026-11-19: a page whose header declares an encoded ICC profile above 1 MiB is refused before jxl-oxide reads it.**", says what `jxl::icc_fits` reads and where it is called, that `cover::fits_within`, `cover::decode_limited`, `fingerprint::decode_page` and the cut-short check are covered through the hook and a page sent as stored is not parsed, gives the reproduction's one figure with the notes section's name, then keeps the old text after "*As filed:*". Site: `jxl::icc_fits`, `jxl::register`, `jxl::rebuilt_jpeg`, `cover::fits_within`. Opened by: "the JPEG XL pages PR, 2026-11-17; closed by the colour profile cap PR". The row keeps exactly six `|` characters.
  2. No annotation: the item closes, it does not narrow.
  3. The bullet "**O350 — jxl-oxide reads an embedded ICC profile outside every limit. Open.**" (under "JPEG XL pages of 2026-11-17") becomes "… **Closed.**" with two or three sentences on what was done; the group's opening sentence stays as written.
  4. The test count, both sites ("Six crates, N tests", "Twenty-six of the N"): 4335 plus the tests this branch adds (`git -C $W diff main -- crates | rg -c '^\+\s*#\[test\]'`; eleven if the plan was followed as written). The gate's status-test-count step checks the figure.
  5. "Two hundred and twenty-one are now closed — … O337 and O377 — leaving one hundred and sixty-seven." becomes "Two hundred and twenty-two … O337, O350 and O377 — leaving one hundred and sixty-six.", and "What remains: the one hundred and sixty-seven open items" (the word wraps a line) becomes "one hundred and sixty-six".

- [ ] **Step 4: The sweep on the way out, and the bare O-number.** Re-run Step 1's four commands: 388 rows, 222 closed, and both count words agree with them. Then `rg -n 'O350' docs web/src` and read every hit: the O130 row, the JPEG XL group's opening sentence, the 2026-11-17 notes paragraph and that branch's plan are history and stay as written; anything else that says O350 is open is corrected.

- [ ] **Step 5: The gate.** From anywhere (the script runs in the checkout of its working directory, so `env -C` sets it):
  ```bash
  RUST_TEST_THREADS=8 NEXTEST_TEST_THREADS=8 flock /run/user/1000/taiyaku-gate.lock env -C /home/davey/projects/taiyaku/.claude/worktrees/jxl-icc-cap -u TAIYAKU_CORPUS scripts/check-all.sh >| /home/davey/projects/taiyaku/.superpowers/sdd/jxl-icc-cap-evidence/gate.log 2>&1
  ```
  Never pipe the gate itself. Then `rg -a -n 'all green|NOT MEASURED|^error|FAILED' $EV/gate.log` and quote the "all green" line (and every NOT MEASURED item under it, if the line reads "all green, except NOT MEASURED:"). A red status-test-count step names the real figure: correct both sites and run the gate again. A red browser project with no failed assertion: read `/proc/loadavg` and the crash log before suspecting the branch.

- [ ] **Step 6: Commit.** `git -C $W add docs/status.md docs/notes.md` and `commit -m "Close O350: the colour profile cap, with its notes"`. Never `--amend`.
