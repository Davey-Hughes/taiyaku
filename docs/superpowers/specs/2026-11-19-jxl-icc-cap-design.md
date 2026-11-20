# JPEG XL pages: a cap on the embedded colour profile (O350)

Approved by the owner on 2026-11-19. Short design: the decisions below were
made in conversation; what is left to the builder is listed at the end.

## Intent

O350: jxl-oxide 0.12.6, through jxl-color 0.11.0, reads a JPEG XL page's
embedded ICC profile outside every limit taiyaku sets. Read in the source on
2026-11-19:

- `read_icc` (jxl-color 0.11.0, `src/icc/decode.rs`) reads the encoded
  profile's size, `enc_size`, as a U64 of at most 2^28, decodes at
  most 18 symbols, then calls `encoded_icc.resize(enc_size, 0)`: up to
  256 MiB zero-filled before the stream's bytes are read.
- Until a header is whole, jxl-oxide parses it again from its first byte
  after every 4 KiB of input (`JxlImageBuilder::read` in jxl-oxide's
  `lib.rs`, and `init_image` in its `image` integration), so each pass
  repeats the resize and the decode from the stream's start.
- A prefix code of a single symbol consumes no bits, so a file of a few
  dozen bytes can carry as many symbols as it declares.
- No limit taiyaku passes reaches it: `read_icc` and `decode_icc` take no
  allocation tracker, and the `image` integration builds its tracker with
  `usize::MAX`.

The branch's process tests have since reproduced the first point's zero-fill
(`docs/notes.md`, "JPEG XL pages: a cap on the embedded colour profile
(O350, 2026-11-19)"); the second point's re-parse was not measured.

The goal: a page whose header declares an encoded profile above a cap is
refused before jxl-oxide parses it, wherever taiyaku decodes a JPEG XL page.

## Approach

- A check of taiyaku's own, beside `jxl::metadata_fits`. It takes the page's
  codestream (the bare page, a `jxlc` box's payload, or the `jxlp` boxes'
  payloads joined without their 4-byte index, through the existing
  `jxl::boxes`), parses `jxl_oxide::ImageHeader` with jxl-bitstream, and,
  where the header's colour encoding wants an ICC profile, reads `enc_size`
  as a U64 and refuses it above the cap.
- The cap is 64 KiB of encoded profile: an `enc_size` of 65,536 passes,
  one more is refused. The 4 KiB re-parse's cost grows with the square of
  the declared size, so a cap well below the boxes' 16 MiB keeps it short.
  The cost of the cap: a genuine profile whose encoded size is over 64 KiB
  is refused with its page; none is known on a manga page. A lossless CMYK
  JPEG XL page carrying Ghostscript's `default_cmyk.icc` (187,484 bytes)
  declares an encoded profile of 187,433 bytes and is refused.
- **Amended by the owner's ruling of 2026-11-19**, after the whole-branch
  review: the cap was 1 MiB as first approved. A profile the cap passes is
  still read in full, then decoded and the result parsed, outside every
  limit, and the decoded profile can be about 36 times the encoded one
  (approach 4). Measured by the branch's process test on the encoded profile
  that decodes to the most, decoded and parsed once, in the first of two
  runs at each size: at 1 MiB the decode and the parse raised a process's
  peak resident memory by 153,677,824 bytes, at 64 KiB by 11,743,232, within
  the boxes' 16 MiB (`docs/notes.md`, "JPEG XL pages: a cap on the embedded
  colour profile (O350, 2026-11-19)"). A page through the hook, which
  decodes and parses the profile more than once, was not measured. The cap
  was lowered so that the worst profile it passes, decoded and parsed once,
  costs no more than a page's metadata boxes may.
- A header that ends before `enc_size` passes the check: jxl-oxide stops at
  the same place without the resize.
- Two call sites: the image hook's decoder, after its `metadata_fits` and
  before `JxlDecoder::with_thread_pool`; and `jxl::rebuilt_jpeg`, after its
  `metadata_fits`. `cover::fits_within`, `cover::decode_limited`,
  `fingerprint::decode_page` and the cut-short check reach jxl-oxide only
  through the hook, so the hook covers them.
- A refused page is refused the way a page whose metadata boxes exceed their
  cap is refused today, so routes, the fingerprint pass and `verify` treat
  it as they treat that page.
- A page sent as stored (a client whose `Accept` names `image/jxl`, at full
  size) is never parsed, so it is unaffected.

## Approaches considered

1. **The declared size, checked by taiyaku (chosen).** Small; it uses crates
   taiyaku-serve already depends on directly (jxl-bitstream,
   jxl-oxide-common, and `jxl_oxide::ImageHeader`, which jxl-oxide
   re-exports).
2. **The same check at 16 MiB, matching the boxes' cap.** Rejected: it
   bounds the fill but leaves a re-parse 65,536 times as costly as at 64 KiB
   (256 times as costly as at 1 MiB, the cap first approved), and, as the
   amendment above found, not the profile decoded from the stream and
   parsed.
3. **Patching jxl-oxide to give `read_icc` an allocation tracker**, as a
   fork or upstream. The right place in the long run, but a fork to keep and
   slower to land; whether to file it upstream is the owner's call, not part
   of this change.
4. **Checking the decoded size as well.** It is the first value inside the
   entropy-coded stream, so reading it means entropy-decoding. Rejected,
   though the reason first given here, that `decode_icc` writes only what
   the stream carries, is wrong: read again in jxl-color 0.11.0's source,
   a tag command writes up to 36 bytes and most read no data, and the tag
   loop returns at the end of the commands without comparing what it wrote
   against the decoded size declared. So a check of the declared size would
   not bound what is written; the encoded size does, in proportion, which is
   why the cap was lowered instead (the amendment above).

## Non-goals

- No change to the boxes' cap, the dimension limit or `max_alloc`.
- No upstream change to jxl-oxide.
- O349, memory kept after decodes, is a separate PR.

## Proof

- Before the fix, the defect is reproduced: a hand-built page declaring an
  `enc_size` of 2^28, decoded through the current hook, shows the
  zero-fill in the process's peak resident memory. This turns O350's "read
  in the source" into a measurement.
- Hand-built pages at 2^28, at the cap and one above it, bare and in a
  container (`jxlc`, and `jxlp` split across boxes), are refused or passed
  by the check. Through the hook, those over the cap are refused by the
  check, and those at the cap pass it and reach jxl-oxide, which refuses
  them as broken (`ImageError::Decoding`, not the cap's
  `ImageError::Limits`). The 2^28 page split across `jxlp` boxes is refused
  through `rebuilt_jpeg`.
- Each refusal test fails when the check is taken out, and fails because
  of the cap: jxl-oxide errors on a truncated hand-built page anyway, so a
  test that only asserts an error proves nothing. It asserts the check's
  own refusal, or that the zero-fill did not happen.
- A page made by `cjxl` carrying a real, non-sRGB ICC profile still
  decodes.
- O350 is closed in `docs/status.md` by the open-items ritual, with its
  notes bullet.

## Left to the builder

Names, including the cap constant's; where the tests sit; whether the
hand-built pages are built in the test or committed as bytes with their
recipe in the docs; and how the reproduction measures peak memory.
