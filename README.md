# taiyaku

Read the same manga page in two editions at once.

`taiyaku` pairs up two releases of the same volume — a Japanese raw and an English
translation, say — and lets a reader swap between them without losing your place.
Press a key and the page in front of you becomes the original. Press it again and
you're back.

The name is 対訳 (*taiyaku*), the Japanese term for a bilingual parallel-text edition
where the original and its translation sit facing each other.

> **Status: the aligner works, and so does the reader.**
> `taiyaku align` pairs two editions and writes a sidecar, measured against eleven
> series; `taiyaku serve` hands an HTTP client that sidecar, its pages and a reading
> position, from local disk or a Stump instance; and the browser reader draws them —
> single-page, two-page and continuous-scroll modes, a swap between editions that
> never loses your place, hand corrections written back into the sidecar, and notes
> written on the raw that stay readable while you read the English. What is built,
> what is known broken, and what comes next are in
> [`docs/status.md`](docs/status.md).

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
# find what a library root actually holds and catalog it — series, editions,
# volumes, chapters — without touching any of it; --add remembers the root so
# a later run needs no flag. The commands below still take real file paths,
# not names
taiyaku library scan --add /srv/manga

# pair a Japanese raw against an English release and write the sidecar
taiyaku align raw-v01.cbz -- en-v01.cbz \
    --name-a raw --name-b en --write taiyaku.json

# one volume against the seven chapter files it corresponds to
taiyaku align raw-v01.cbz -- en-c001.cbz en-c002.cbz en-c003.cbz

# add a third edition to the same document — --write merges
taiyaku align raw-v01.cbz -- en-fan-c001.cbz --name-b en-fan --write taiyaku.json

# look at what it decided
taiyaku verify raw-v01.cbz -- en-v01.cbz --out report.html

# which of an edition's pages are the group's own matter — donation cards,
# staff credits — rather than the work? prints; --write puts them in the sidecar
taiyaku credits en-fan-c*.cbz --sidecar taiyaku.json

# where is the text on each page? prints; --write puts the blocks in a sibling
# <stem>.regions.json. it does not yet find them reliably — see docs/status.md,
# "What is known to be wrong or missing"
taiyaku regions raw-v01.cbz --sidecar taiyaku.json

# how well does it find them? scores what `regions` detects against boxes a
# person drew in a sibling <stem>.truth.json, as five outcomes per box rather
# than a hit-or-miss — a merged panel and a split word are different failures
taiyaku score --sidecar taiyaku.json raw-v01.cbz

# which pages should be labelled by hand in the first place?
taiyaku score --sample corpus

# reading raw page 47, which English page is in front of you?
taiyaku lookup taiyaku.json raw en 47

# and between two editions that were never aligned to each other directly
taiyaku lookup taiyaku.json en en-fan 45

# hand a reader the sidecar, its pages, and where you were, over HTTP —
# every path given is imported into an on-disk SQLite library first
# ($XDG_DATA_HOME/taiyaku/library.db, or --db PATH), so reading progress,
# notes and hand corrections persist there across restarts
taiyaku serve taiyaku.json --addr 127.0.0.1:8484

# with **no paths at all**, it serves every work the library holds — which is
# the invocation a scanned library uses, since `library scan` produces works
# that have no `taiyaku.json` to name on a command line:
taiyaku serve --addr 127.0.0.1:8484
#
# a work with no sidecar file is served from the database and corrected into
# it: a hand correction writes the `alignment` and `run` rows and no file,
# so its only copy lives there until `taiyaku library export` writes one out.

# **every route needs a credential.** Give the first account a password from
# the host's own shell before starting a server — there is no window in which
# an unclaimed server sits on the network waiting to be claimed:
taiyaku library user promote davey        # prompts for a password, twice
#
# it PROMOTES the account the library already has rather than adding one, so
# every note and reading position recorded before now belongs to it. Then:
curl -u davey:… http://127.0.0.1:8484/api/works

# behind a reverse proxy, name the proxy so X-Forwarded-For is believed from
# it and only from it — and so the session cookie carries Secure:
taiyaku serve taiyaku.json --addr 127.0.0.1:8484 --trusted-proxy 10.0.0.0/8
#
# **the proxy must set both forwarded headers itself.** naming it with
# --trusted-proxy tells taiyaku to believe them, and a proxy that does not
# write them forwards whatever the *client* sent under the same names:
#
#   proxy_set_header Host              $host;
#   proxy_set_header X-Forwarded-Host  $host;
#   proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
#
# without the first two, nginx's default rewrites Host to the upstream and
# taiyaku's CSRF check compares two different authorities — every login 403s.
# with --trusted-proxy but no X-Forwarded-Host of the proxy's own, a client can
# supply one. if TLS terminates at the proxy and nothing is forwarded, use
# --secure-cookies alone.

# beside Stump: English editions come from the instance, raws from disk,
# resolved once at startup and cached beside the sidecar
TAIYAKU_STUMP_URL=https://stump.example TAIYAKU_STUMP_KEY=… \
  taiyaku serve corpus/gotoubun-v01/taiyaku.json --addr 0.0.0.0:8484

# point an OPDS reader at
#   http://<host>:8484/opds/v1.2/catalog
# with your account name as the username and an API key as the password —
# a key is shown once, because only its hash is stored:
taiyaku library key create davey --name ipad

# share part of a library rather than all of it. **a grant is a restriction**,
# and an account holding none sees every root — so the FIRST grant is the one
# that narrows an account, not the one that widens it:
taiyaku library user grant-root alice /shelf/manga
taiyaku library user revoke-root alice /shelf/manga
#
# a restricted account sees only the editions whose files sit under a root it
# was granted, and cannot read their pages either. a work imported from a
# `taiyaku.json` sits under no root at all, so it is visible only to an
# unrestricted account. members scanned before this feature existed carry no
# root until the schema migration recovers it from their paths; anything it
# cannot recover stays unrooted and invisible to a restricted account, and
# `grant-root` says so when it finds any.

# the same import, without starting a server — for a library `serve` has
# already populated, or a one-off migration
taiyaku library import taiyaku.json

# and back out again: every document a work round-trips through,
# taiyaku.json plus its <stem>.notes/progress/truth/library.json siblings —
# and only the ones there is anything to say: a work with no reading position,
# no labelling and no hand-fixed metadata exports two files, not five.
# --dir gives each work a subdirectory of its own, named for its slug, since
# every library names every sidecar taiyaku.json: ./export/<slug>/taiyaku.json
taiyaku library export go-toubun-no-hanayome --dir ./export
taiyaku library export --all --dir ./export

# what decides, as JSON — derivation's input, not its output: edit it and
# import it back to change how paths become series, editions, volumes and
# chapters, then scan again to apply it. Nothing here is hidden in code; the
# four defaults are ordinary rows, written once per database by migration 3
# and scoped to every root, not seeded per root by a scan
taiyaku library rules > rules.json
taiyaku library rules --import rules.json

# what a scan+derive proposed but did not decide — an inbox, not an action:
# "this directory may be the colour edition of that series", say
taiyaku library proposals

# accept one: writes a narrow rule scoped to its own root and re-derive picks
# it up; --dismiss records "no" so it stops being asked; --unaccept undoes
# an accept
taiyaku library proposals --accept 3
taiyaku library proposals --dismiss 4
```

To read in the browser, build the WebAssembly binding first, then run the reader
against `serve --db`. The browser opens on a **library** now, not a single work —
a grid of series with a continue strip above it, `react-router` resolving four
routes between the grid, a series page, search and the reader itself, and a
`Library` button in the reader's own chrome for the way back out. The binding is
generated and gitignored, but install order does
not decide whether `taiyaku-wasm` resolves: `taiyaku-wasm` is a `link:` symlink,
and pnpm creates it whether or not `pkg/` exists yet, dangling until it does. The
symlink is live — it reads through to whatever `pkg/` holds at the moment
something follows it, not to whatever was there when it was created. Build first
anyway, because `pnpm run dev` and `pnpm run build` follow that symlink as soon as
they start: `pkg/` needs to hold the real package by then, not by the time
`pnpm install` ran — see `web/README.md`, "The WASM dependency":

```sh
./scripts/build-wasm.sh          # generates crates/taiyaku-wasm/pkg
pnpm -C web install --frozen-lockfile

taiyaku serve --db taiyaku.db --addr 127.0.0.1:8080
pnpm -C web run dev              # http://localhost:5173
```

`pnpm -C web run dev` proxies `/api` and `/opds` through to `serve` rather than the server
enabling CORS — CORS would be a production-facing change made for a development
convenience. The proxy target defaults to `http://127.0.0.1:8080`, which is why the
`--addr` above is 8080 and not the 8484 used earlier on this page; set
`TAIYAKU_SERVE` to point it somewhere else.

Members may be `.cbz`, `.zip`, any other zip-based archive whatever it is named,
or a plain directory of page images. The container is identified by content
rather than by extension, because extensions lie in both directions — a zip is
often named `.cbr`, and a real RAR or 7-Zip archive is often named `.cbz`. Only
zip is read: the others are recognised so the error can name the format and say
to convert it, rather than reporting a corrupt zip. Several members on either
side of the `--` are concatenated before solving, which is how chapters map onto
a volume and how two tankōbon map onto one deluxe edition.

Before any alignment there is often a prior question — you have three copies of
volume 1 and want to know which one to keep. Two commands answer it, and neither
moves, renames nor deletes anything:

```sh
# which volumes in a release directory have more than one copy?
taiyaku inventory releases/

# for one such set, which copy is worth keeping?
taiyaku duplicates v001-c0 -- v001-c1 --against raw-v01.cbz
```

They print what they found and leave the acting to a person. `inventory` groups
candidate copies by the volume number in the name, and prints a group whose name it
could not parse whether or not it is duplicated — a candidate that vanishes from the
output is the silence this whole project is written against. `duplicates` then runs
the protocol over one set: signatures first, then every unordered pair solved once,
then a clustering on identity and a ranking of what is genuinely distinct.
`--against` names a reference edition, which is the only thing that can tell two
copies that refuse each other apart.

One edition joins a double-page spread into a single wide image where the other
splits it in two, and `align` splits the joined one by default — measured across the
corpus, unhandled spreads make a correct alignment look broken, so there is no case
for making that opt-in. `--no-split-spreads` gives the page-level diagnostic
instead; it cannot write a sidecar, because a sidecar records which pages are joined
spreads and that path does not detect them.

`--stats` prints the figures the answer was reached from: what calibration measured
of the two editions, and what shape the alignment came out. A refusal with no numbers
beside it is not reviewable, and these are the numbers.

A work can hold any number of editions. One is the **hub** and every other aligns
only to it, so adding the fifth costs one alignment rather than four; a lookup
between two editions with no alignment between them composes through the hub and
says so. Measured on the primary corpus volume at five editions: 0.91% of pages
resolve directly but not through the hub, and nothing resolves to two different
places.

`taiyaku topology <hub>... -- <spoke>... -- <spoke>...` checks that bet on your own
material: it solves the star and the all-pairs the star replaces, and reports what
composition loses. It compares unsplit pages rather than the sidecar's slots, so a
spread one edition joins and another splits is simply lost to it — which is why it
reports 2.12% on that same volume against the 0.91% above. The two measure the same
property and are not expected to agree.

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
and passes the gate on its own. Work happens on a feature branch and lands as **one squashed
commit**, through a **pull request**. `main` is protected: it does not accept a direct push, so the
PR is the only way in.

    git switch -c my-change
    scripts/check-all.sh                # the local mirror of CI — all four jobs
    scripts/check-all.sh --no-wasm      # same, skipping the wasm job
    scripts/check-all.sh --no-web       # same, skipping the web job
    scripts/check-all.sh --corpus-debug # corpus checks in debug, for overflow checks
    git push -u origin my-change
    tea pr create                   # or open it in the web UI

Run `scripts/check-all.sh` before pushing anyway, even though CI runs the same four jobs on the PR.
It is the same gate several minutes earlier, and it is the reason a red PR is unusual rather than
routine. It prints what it did **not** measure as its last line rather than a bare `all green`, so a
skipped corpus, a missing `wasm-pack` or an absent `web/node_modules` is stated rather than assumed
away.

The lockfile check is mirrored too: `pnpm -C web install --frozen-lockfile --lockfile-only`
verifies `package.json` and the lockfile agree without touching `node_modules` — measured at
roughly 240ms, cheap enough to run on every pass (unless `--no-web` skips the job outright). It
runs ahead of the `web/node_modules` check, not inside it, so a fresh clone still gets it — that
is exactly the state where drift is likeliest to have gone unnoticed. Before the move to pnpm this
was the one gap: the equivalent `npm ci` would have deleted a working `node_modules` on every gate
run, so `package.json` and the lockfile drifting apart passed here and failed the forge, and a
developer had to run the check by hand after changing dependencies. pnpm has a command that does
only the check and nothing else, which is what closed it. See `docs/ci.md` for the measurement and
how it was verified.

The forge enforces the shape that used to be a local script's job: merges are squash-only, so a PR
cannot produce a merge commit or a string of fixup commits on `main`, and the branch is deleted on
merge. "Every commit on `main` passes CI" survives intact by a different route: a PR cannot merge
while its branch is behind `main`, so the head that CI tested and the tree that lands are the same
tree. Falling behind does not quietly weaken the gate — it blocks the merge until you rebase.

**The PR description is the commit message.** `.forgejo/default_merge_message/` overrides the
default squash message, which would otherwise append a `Reviewed-on:` trailer pointing at the PR.
The squashed commit is instead the PR title as its subject and the PR description as its body,
verbatim — so write the description as the commit message you want on `main`, not as a note to the
reviewer.

`SQUASH_TEMPLATE.md` therefore stops at its title line, and the blank line after it is load-bearing.
Forgejo already prepends the description to the squash prefill on its own, so naming
`${PullRequestDescription}` in the template body wrote it a second time — the shape the merges
between `#14` and `#27` carried until their messages were rewritten. Deleting the trailing blank line
does not undo that either: the template is split on its *first* newline, and a file with no second
part falls back to the default body, which is the `Reviewed-on:` trailer this template exists to
suppress.

Once per clone, so a non-fast-forward `git pull` fails rather than quietly creating a merge commit:

    git config merge.ff only && git config pull.ff only

That is convenience — `.git/config` is untracked and binds nobody. Branch protection on the forge
is the real constraint.

## Licence

GPLv3. See [LICENSE](LICENSE).
