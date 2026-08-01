# taiyaku

Read the same manga page in two editions at once.

`taiyaku` pairs up two releases of the same volume — a Japanese raw and an English
translation, say — and lets a reader swap between them without losing your place.
Press a key and the page in front of you becomes the original. Press it again and
you're back.

The name is 対訳 (*taiyaku*), the Japanese term for a bilingual parallel-text edition
where the original and its translation sit facing each other.

> **Status: the aligner works, and the reader has a back end and no front end.**
> `taiyaku align` pairs two editions and writes a sidecar, measured against eleven
> series; `taiyaku serve` hands an HTTP client that sidecar, its pages and a reading
> position. Nothing draws them yet. What is built, what is known broken, and what
> comes next are in [`docs/status.md`](docs/status.md).

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

## Using it

```sh
# pair a Japanese raw against an English release and write the sidecar
taiyaku align raw-v01.cbz -- en-v01.cbz --split-spreads \
    --name-a raw --name-b en --write taiyaku.json

# one volume against the seven chapter files it corresponds to
taiyaku align raw-v01.cbz -- en-c001.cbz en-c002.cbz en-c003.cbz

# add a third edition to the same document — --write merges
taiyaku align raw-v01.cbz -- en-fan-c001.cbz --split-spreads \
    --name-b en-fan --write taiyaku.json

# look at what it decided
taiyaku verify raw-v01.cbz -- en-v01.cbz --out report.html

# reading raw page 47, which English page is in front of you?
taiyaku lookup taiyaku.json raw en 47

# and between two editions that were never aligned to each other directly
taiyaku lookup taiyaku.json en en-fan 45

# hand a reader the sidecar, its pages, and where you were, over HTTP
taiyaku serve taiyaku.json --addr 127.0.0.1:8484

# beside Stump: English editions come from the instance, raws from disk,
# resolved once at startup and cached beside the sidecar
TAIYAKU_STUMP_URL=https://stump.example TAIYAKU_STUMP_KEY=… \
  taiyaku serve corpus/gotoubun-v01/taiyaku.json --addr 0.0.0.0:8484

# point an OPDS reader at
#   http://<host>:8484/opds/v1.2/catalog
```

Members may be `.cbz`, `.zip`, any other zip-based archive whatever it is named,
or a plain directory of page images. The container is identified by content
rather than by extension, because extensions lie in both directions — a zip is
often named `.cbr`, and a real RAR or 7-Zip archive is often named `.cbz`. Only
zip is read: the others are recognised so the error can name the format and say
to convert it, rather than reporting a corrupt zip. Several members on either
side of the `--` are concatenated before solving, which is how chapters map onto
a volume and how two tankōbon map onto one deluxe edition.

`--split-spreads` handles the case where one edition joins a double-page spread into
a single wide image and the other splits it in two. It is worth passing almost
always — measured across the corpus, unhandled spreads make a correct alignment look
broken.

`--stats` prints the figures the answer was reached from: what calibration measured
of the two editions, and what shape the alignment came out. A refusal with no numbers
beside it is not reviewable, and these are the numbers.

A work can hold any number of editions. One is the **hub** and every other aligns
only to it, so adding the fifth costs one alignment rather than four; a lookup
between two editions with no alignment between them composes through the hub and
says so. Measured on the primary corpus volume at five editions: 0.91% of pages
resolve directly but not through the hub, and nothing resolves to two different
places.

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

## Contributing

`main` is **linear** — no merge commits — and every commit on it is an **atomic unit**: it builds
and passes the gate on its own. Work happens on a feature branch, which lands as **one squashed
commit**.

    scripts/check-all.sh          # the local mirror of CI — fmt, clippy, tests, wasm build
    scripts/land.sh               # land the current branch (opens an editor for the subject)
    scripts/land.sh -- --no-wasm  # same, skipping the wasm build

`land.sh` refuses a dirty tree, a `main` that differs from `origin/main`, and a branch that is
behind `main`. It then squash-merges, runs `scripts/check-all.sh` **on the merged tree before the
commit exists**, and commits only if that passes — which is what makes "every commit on `main`
passes CI" a property rather than a hope. The branch is deleted once its tree matches `main`.

A plain `git merge --squash` discards every commit message on the branch, so `land.sh` prefills the
message with all of them under a `--- Squashed from N commits ---` marker. Delete what you do not
want; what is left is kept verbatim. Losing the reasoning is not the price of a tidy graph.

Once per clone, so a non-fast-forward merge fails rather than quietly creating a merge commit:

    git config merge.ff only && git config pull.ff only

That is convenience — `.git/config` is untracked and binds nobody. The remote is the real
constraint: it allows only squash and fast-forward merges, so a PR merged in the web UI cannot
produce a merge commit either.

## Licence

GPLv3. See [LICENSE](LICENSE).
