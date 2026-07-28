# taiyaku

Read the same manga page in two editions at once.

`taiyaku` pairs up two releases of the same volume — a Japanese raw and an English
translation, say — and lets a reader swap between them without losing your place.
Press a key and the page in front of you becomes the original. Press it again and
you're back.

The name is 対訳 (*taiyaku*), the Japanese term for a bilingual parallel-text edition
where the original and its translation sit facing each other.

> **Status: design only.** There is no working code in this repository yet. The
> design lives in [`docs/`](docs/). Read that first if you're wondering whether this
> is worth your time.

## The problem

Two editions of the same volume never line up page-for-page. Japanese raws are
usually scanned from tankōbon; English releases are often assembled per chapter from
magazine runs. Between any two releases you'll hit:

- a constant page offset from differing front matter
- credit pages, scanlator notes, and ads inserted on one side only
- double-page spreads joined in one edition and split in the other
- colour inserts present in one edition and greyscaled or dropped in the other

So "show me page 47 of the other edition" is not a lookup. It's an alignment problem.

## The approach

Panel geometry is identical across editions — the artwork is the same artwork, and
only the text inside the bubbles differs. That makes page-to-page similarity easy to
measure with structural hashes that ignore high-frequency text detail. The real work
is *sequencing*: recovering a monotonic page correspondence with insertions and
deletions on both sides, which is a well-understood dynamic-programming problem.

Alignment is computed once, offline, and written to a sidecar file that any reader can
consume. Three layers, each a superset of the last:

| Layer | Produces | Unlocks |
| ----- | -------- | ------- |
| **L0** | page ↔ page correspondence with gap markers | whole-page swap, side-by-side |
| **L1** | per-pair affine transform | zoom and pan preserved across a swap |
| **L2** | matched text regions | tap a bubble to see the original line |

L1 is the leverage point. Once you have a transform mapping one page onto its
counterpart, matching bubbles across editions stops being a matching problem and
becomes arithmetic on transformed centroids.

Written in Rust (edition 2024). The core compiles to WebAssembly, so the same alignment
and reading logic runs in the browser and, later, inside a native shell.

## Design principles

**The sidecar is the product.** The alignment format is meant to be readable by
anything — this reader, [Komga](https://komga.org), [Stump](https://stumpapp.dev), or
something not written yet. The bespoke reader here exists to prove the format is
useful, not to become the only way to use it.

**Wrong alignments must be visible and fixable.** Any automatic pairing gets some
pages wrong. An alignment you cannot inspect or correct is worse than none, because it
silently shows you the wrong page and you never find out. A correction UI is part of
the core, not a nice-to-have.

**Your library is not modified.** Archives are read, never rewritten. Alignment lives
beside them and can be deleted without loss.

## Licence

GPLv3. See [LICENSE](LICENSE).
