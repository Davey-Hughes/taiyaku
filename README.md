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
# --library NAME files the directory under that library, creating it, so two
# directories (Volumes and Chapters, say) share one library page and one
# grant; without it, a directory is a library of its own, and so can nest in
# another library's directory. A directory another library already holds is
# linked into NAME rather than refused: one directory, with one set of
# settings and one scan, in each library. Without --library it is refused,
# since no library is named to link it into.
# `taiyaku library libraries` lists the libraries. The derive that follows
# reads every directory whichever library holds it, so a chapter release
# whose files name their volume joins that volume's work: as another edition
# where the two directories' edition keys differ, and as more pages of the
# volume's edition where they share a key, as both do by default (`raw`)
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
#
# a work a later scan derives is served without a restart: every two seconds
# the server asks whether the library's structure has changed and reads it
# again if it has, and it asks at once when a request names a work it does
# not hold yet

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
# which lists every work, and every library's works twice: by the directories
# each library holds, and by work, each work with a member under them listed
# with every edition the grant allows. in the browser, About and the account
# screen show this address, built from the origin the page was loaded from,
# with a Copy address button only in a secure context (https, or localhost)
# that has the Clipboard API.
# use your account name as the username and an API key as the password —
# a key is shown once, because only its hash is stored:
taiyaku library key create davey --name ipad

# share some libraries rather than all of them. **a grant is a restriction**:
# a new account sees every library, so the FIRST grant is the one that narrows
# it, not the one that widens it. revoking its last grant leaves it seeing no
# library, never every library. `every-library` (or "Every library" on the
# Users screen) lifts the limit:
taiyaku library user grant-library alice Manga
taiyaku library user revoke-library alice Manga
taiyaku library user every-library alice
#
# a restricted account sees only the editions with a file its granted
# libraries show, and cannot read their pages either. an edition some of
# whose files its libraries do not show is
# narrowed: the account is shown only the files its libraries show, its pages
# numbered from 1, and Configuration → Split editions lists each edition a
# library shows some of and not all for an administrator. a work
# imported from a `taiyaku.json` sits under no root at all, so it is visible
# only to an unrestricted account. members scanned before this feature existed
# carry no root until the schema migration recovers it from their paths;
# anything it cannot recover stays unrooted and invisible to a restricted
# account, and `grant-library` says so when it finds any.

# take a backup. the database is the only copy of notes, collections, tags,
# external ids, users and read history — a sidecar export carries none of
# those, so it is not a backup and this is. SQLite's own `VACUUM INTO`, so the
# server does not have to stop:
taiyaku library backup                    # -> <db parent>/backups/library-<stamp>.db
taiyaku library backup --keep 7           # ...and delete all but the seven newest, leaving those taken before a migration
#
# there is no schedule inside the server. a cron entry runs this. `--out DIR`
# writes somewhere else, and the settings screen will not list what lands
# there — it can only compute the one directory beside the database.
#
# an open that migrates the database backs it up first, to
# <db parent>/backups/library-<stamp>-before-v<N>.db, N being the version it
# migrates to, and refuses to migrate if that backup fails. --keep never
# deletes one of those; delete them by hand. a migration that fails is tried
# again at the next open, which takes no new backup while the database file
# and its -wal are unchanged since the newest -before-v<N> one and that one
# opens at the database's version. unchanged is judged by the later of each
# file's modification and change times, so a write or a chmod after the
# backup makes the next open back up again, and so does a move where the
# filesystem sets the change time on a rename. a copy cut off part way stays
# there as a hidden .library-<stamp>-before-v<N>.db.partial, never listed or
# pruned; delete it by hand.
#
# restore is out of band on purpose: stop the server, move the file, start it.

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

**The rules and the roots are also reachable over HTTP now**, admin-only, under
`/api/library/rules` and `/api/library/roots`. Three of those routes do
something the CLI cannot: `POST /api/library/rules/preview` answers *what a
proposed rule list would do to the whole library* — series merged, works re-homed, works
orphaned, proposals raised, which files each rule matched and which it missed —
by running the real derivation against it and rolling back, so nothing is
written; and `POST /api/library/roots/{id}/scan` starts a scan in the server
rather than at a terminal, with `GET` on the same path reporting it. Removing a
root can state what it will take — works, reading positions, notes — before it
takes them, if the caller asks first: `DELETE .../roots/{id}?dry_run=true` runs
the identical count and rolls back, so a client can render it before sending
the plain `DELETE` that actually removes anything; sent without `?dry_run=true`,
a `DELETE` reports those same counts as it takes them, in one step.

`PUT /api/library/rules` is the third, and it is the first thing in this
product that *applies* a rule. Every other rules route wrote the rule table
and returned, so an edit changed the rules and nothing else until the next
scan; this one replaces the set and re-derives the library in a single
transaction, then reports what it actually did — against what the preview
predicted — and which accepted proposals it reopened. It is not the same as
`POST .../rules/import`, which mints fresh ids by design and so reopens every
accepted proposal whose rule it replaces: that is right for a rule document
carried in from another library and wrong for a save.

**The Libraries screen is built**, at `/config/libraries` — an admin-only
settings area beside the library, where a library is made, renamed, scanned and
removed from the browser rather than a terminal, and so is each directory in it;
`/config/roots` lands there, and so does a bare `/config`. It is a table of the
libraries, where a row opens that library's page, at its name
(`/config/libraries/<name>`), and a directory of a library has a page of its own
beneath it, which a wide screen draws beside the library's. A library is made on
`/config/new-library` from a name and one directory, typed or found with the
directory browser, which lists only beneath the directories the server was
started with (`--dirs`, or `TAIYAKU_DIRS`); more directories are added on the
library's page. A refusal — a name already taken, a name no address could hold
(`.` or `..`), a directory inside another of the same library, a path that
cannot be read — is drawn as the server's own sentence. A directory another
library already holds is not refused: adding it, or making a library from it,
links it, and it keeps the settings it has there, which both libraries then
share, as they share its scans. Its page says which other libraries hold it.
Removing a library or a
directory states what it will take — works, reading positions, notes — and
offers no confirm button at
all until those counts have arrived, and names any account the removal leaves
seeing no library. A directory another library holds has two removals: one
from this library alone, which deletes nothing and, like Move below, is
refused while a scan is queued in this library, or while a scan, a rule save,
a stamp, a move or another such removal is running, and **Remove this directory**,
from every library that holds it; removing a library leaves such a directory
to the libraries that still hold it. A directory's page has **Move…**, to another library or a new
one: a move takes the directory out of this library and puts it in the other,
and changes nothing else, so before it offers
the move it names the accounts that will gain and lose sight of the directory,
and it refuses a move while a scan is queued in either library, or while a scan,
a rule save, a stamp, another move or a removal from a library is running. A library that already holds
the directory is not offered. **Scan library** on a library
walks in turn each directory the library shows — its own, and each nested in
one of them that its walk would reach, each by its own rules — and derives
once. A scan started from the screen reports both halves of what it did in
full: the walk and the derivation, every field (a walk's **Taken over** only
when it is not zero), with what it *could not* do listed before what it did,
and an empty list drawn as `0` rather than left out — so "nothing was
unreadable this time" cannot be mistaken for "this version does not say".

**The rule editor is built too**, at `/config/rules` — the screen the whole
preview mechanism exists for. An ordered
list on the left and the rule being edited beside it, with the preview beneath
both; on a narrow screen the list comes first and the rule under it. Nothing
is written until Save, and the button says what saving will do — *apply to 71
works* — from a preview that is never stale, because the preview re-runs on
every gesture that changes the answer and on none that do not: a field commits
on blur or Enter and never while you type, since deriving the library to
answer a question about a half-typed regex costs a third of a second to get a
wrong answer. Four tabs read that preview: the impact, the files the selected
rule matched *and the ones it only just missed*, which of the seven fields each
rule decided, and any two rules that would decide one field differently — the
last carrying its count on the tab strip, so a conflict is never hidden behind
a tab nobody clicked. A rule an accepted proposal wrote is marked, and deleting
it says which proposal that reopens before it does anything.

**And the other two tabs are built**, at `/config/users` and `/config/backup`.
Users is a table of the accounts — Name, Role, Libraries, Last seen — where a
row opens that account's page, at its name (`/config/users/<name>`) and beside
the table on a wide screen, and an account is made at `/config/new-account`,
which opens beside the table on a wide screen too, from a name, a password typed
twice and a role. An account's page holds its role, whether it may download,
stream and write ComicInfo.xml into archives, which libraries it may see —
*Every library, including any added later*, or *Only these* with a tick per
library — and its open sessions and its API keys, each revocable one at a time
for the device that was lost. The account this browser is signed in as is
marked, and so is the session in front of you, so revoking one is not a guess.
The two things that would lock a library out of its own administration are
refused by the server, and the page says why beside each control a rule holds:
the last administrator who can log in can be neither removed nor demoted, and
nobody may demote themselves. Resetting somebody's password ends every session
they have and leaves their API keys alone; resetting your own is refused here
and belongs to your account screen at `/account`, which the refusal links to.
Removing an account states five counts and the name first, takes no work,
edition or member with it, and goes back to the table, which says the account
was removed.

The backup tab takes one now, lists what exists with a download link each, and
says the four things a button cannot: restore is out of band, a backup is the
whole database — password hashes included — a sidecar export is not a backup,
and the schedule is the cron entry above; the only backup the server takes by
itself is the one before a migration.

**Split editions** is the last tab, at `/config/split-editions`. It lists each
edition that a library shows some of and not all of, which an account granted
that library and not the others showing the rest is shown narrowed: for each
library, what an account granted it alone sees of the edition, and for each
file, its pages and the directories holding it, linked to their pages. Under
the list it says how to split one: give one of its directories its own
edition key (Edition key, on the directory's page), then scan that directory.

To read in the browser, build the WebAssembly binding first, then run the
reader against `serve --db`. The browser opens on a **library** now, not a
single work — a grid of series with a search bar and a continue strip above it,
and `react-router` resolving the routes between the grid, a series page and the
reader itself, besides the settings area above, the proposals inbox and the
account screen. A search runs over the library the page shows, every library on
`/`, and replaces the grid with its hits while the box holds a query; an old
`/search?q=` link lands there with its query. The sidebar lists the libraries
under a **Libraries** heading that leads to all of them, as the wordmark above
it does too, and keeps at its foot the build's label, then **More** (Proposals,
Configuration and Account for an administrator, and for anyone else the account
screen itself), then the theme, last so that no link sits in the window's
bottom-left corner, where a browser shows a hovered link's address. The label
is the build's short commit, with a `*` when tracked files differed from that
commit, or "unknown build", a link for every account to `/about`. That page
shows the commit
(linked into the repository when the build has one), the source link, the OPDS
catalog's address and each format taiyaku writes, with its version and a line
saying what it is, and links on to `/about/licence`, which gives taiyaku's
licence and the fonts', each above its full text, under a list of contents
linking to each. A page's **View** button holds the browse preferences that act
there — layout, edition selector, panel and density on a series page, the
continue strip and density on the library. The reader's own chrome has a
`Library` button for the way back out. The binding is generated and gitignored,
but install order does not decide whether `taiyaku-wasm` resolves:
`taiyaku-wasm` is a `link:` symlink, and pnpm creates it whether or not `pkg/`
exists yet, dangling until it does. The symlink is live — it reads through to
whatever `pkg/` holds at the moment something follows it, not to whatever was
there when it was created. Build first anyway, because `pnpm run dev` and `pnpm
run build` follow that symlink as soon as they start: `pkg/` needs to hold the
real package by then, not by the time `pnpm install` ran — see `web/README.md`,
"The WASM dependency":

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

## Running it on a server

The repository builds one image: the server, with the web app it serves.
CI pushes it as `forge.daveynet.xyz/davey/taiyaku:sha-<first 7 of the commit>`
on every merge to `main` whose pipeline passes, and as `:latest` when that
commit is main's tip. Every Sunday, and on a manual dispatch on `main`, it
rebuilds main's tip with a fresh `runtime` stage and re-pushes its `sha-` tag
and `:latest`, so patched Debian packages ship in a week with no merge; a
dispatch on another branch runs the checks and skips the image job. It listens
on 8484 and runs as uid 99, gid 100 (Unraid's `nobody:users`). It needs three mappings:

| Container | What | Back it up? |
| --- | --- | --- |
| `/config` | `taiyaku/library.db` and `taiyaku/backups/`: accounts, reading positions, notes, corrections | yes — it is the only copy |
| `/cache` | covers, tiles and reading-size pages | no — all of it regenerates |
| `/media` | the share your libraries' directories are in | it is your library |

A compose file in the shape this repository is meant to be deployed with:

```yaml
services:
  taiyaku:
    image: forge.daveynet.xyz/davey/taiyaku:latest
    container_name: taiyaku
    restart: unless-stopped
    ports:
      - 55006:8484
    environment:
      TAIYAKU_TRUSTED_PROXY: ""   # empty trusts nobody; fill it in as described below
    volumes:
      - /mnt/user/appdata/taiyaku/config:/config
      - /mnt/user/appdata/taiyaku-cache:/cache
      - "/mnt/user/media 1:/media"
```

`/config` and `/cache` must be writable by 99:100. Create them before the first
start, because a directory Docker creates for you is `root:root`. A root-owned
`/config` makes the container exit with a permission error; a root-owned
`/cache` only warns and keeps serving, regenerating every page each time:

```sh
mkdir -p /mnt/user/appdata/taiyaku/config /mnt/user/appdata/taiyaku-cache
chown 99:100 /mnt/user/appdata/taiyaku/config /mnt/user/appdata/taiyaku-cache
```

A root with `keeps_documents` turned on (it is off by default) writes hand
corrections beside it, in `.taiyaku/`, and for that its directory must be
writable by 99:100; when it is not, they stay in the database and the save
answers `persisted: false`. (Nothing schedules scans yet; a later change adds
that.)

Give the first account a password from the host:

```sh
docker exec -it taiyaku taiyaku library user promote davey
```

Then sign in and make the libraries on Configuration → Libraries. Each is made
from a name and one directory, picked in a browser that starts at `/media`: the
image sets `TAIYAKU_DIRS=/media`, and the browser lists nothing outside it. More
directories are added on the library's page with Add directory…. Two
directories, `/media/Manga/Volumes` and `/media/Manga/Chapters` say, are derived
together whichever libraries hold them, so a chapter release whose files name
their volume joins that volume's work: as another edition where the two
directories' edition keys differ, and as more pages of the volume's edition
where they share a key, as both do by default (`raw`). Scan library on a
library's page walks each directory the library shows and derives once.

`TAIYAKU_DIRS` is a `:`-separated list, because a directory's path can hold a
space or a comma. `taiyaku serve --dirs DIR`, repeatable, sets the same
list on the command line and wins over the variable. `serve` refuses to start
while `TAIYAKU_FOLDERS`, the variable's old name, is set.

The image does not create `/media`, so a container that maps nothing there
browses nothing, and `serve` prints each path it was given that is not a
directory. A `TAIYAKU_DIRS` that is set and empty replaces the image's
default and lists nothing. A deployment that mapped its share at `/library`
before sees an empty browser until it maps `/media` or sets `TAIYAKU_DIRS`
to where the share is. A relative directory is taken from the one the
server runs in.

The same from the host, for a script:

```sh
docker exec taiyaku taiyaku library scan --add /media/Manga/Volumes --library Manga
```

`--library` names the library the directory is filed under, and creates it.
A name is read without its surrounding white space and without regard to ASCII
case, so `--library manga` files a directory under the library `Manga`. Left
out, the directory becomes a library named for the directory, `Volumes` here.
A directory another library already holds is linked into the named library and
keeps its settings; left out, such a directory is refused.

The first scan reads every archive once to hash it, so on a large library it
takes as long as reading the whole share.

**The database is in WAL mode**, so a backup tool that copies files must stop
the container first. `taiyaku library backup` is the alternative that does not
need one.

**Behind Nginx Proxy Manager**, turn "Cache Assets" and "Block Common
Exploits" off. The first caches script and font files in a shared cache while
ignoring `Cache-Control`; the second refuses any query parameter whose value
starts with `/`. Put this in the host's Advanced tab:

```nginx
real_ip_header CF-Connecting-IP;

location / {
    proxy_pass http://192.168.1.69:55006;
    proxy_http_version 1.1;
    proxy_set_header Host              $http_host;
    proxy_set_header X-Forwarded-Host  $http_host;
    proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header X-Real-IP         $remote_addr;
    proxy_read_timeout 300s;
    proxy_send_timeout 300s;
    proxy_buffering off;
}
```

Why the whole `location`, and not just the one header taiyaku needs:
- A `location /` in the Advanced tab replaces NPM's own, and that is the only
  way in. NPM's own location sets headers of its own, and nginx then ignores
  any `proxy_set_header` written at server level.
- `$http_host`, not `$host`, keeps the port. With `$host`, a proxy on a port
  other than 443 makes every login answer 403, because the browser's `Origin`
  carries the port and `Host` would not.
- `real_ip_header CF-Connecting-IP;` is for a name proxied through Cloudflare.
  NPM's default reads `X-Real-IP`, which Cloudflare does not send, so without
  the line taiyaku sees a Cloudflare address for every visitor and its login
  rate limiter keys them all together. With no Cloudflare in front, the line is
  harmless. It must sit **outside** the `location` block, as above: nginx
  applies its real-IP module once per request, at server level first, and NPM
  already sets `real_ip_header X-Real-IP` at http level, which the server
  inherits, so a line inside the location loses to an `X-Real-IP` header the
  visitor supplies. Run `docker exec <NPM's container> nginx -t` after saving
  (`NginxProxyManager` on this NAS; `docker ps` shows yours).

`TAIYAKU_TRUSTED_PROXY` names the address the proxy's connections arrive from,
as taiyaku sees it, so there is no value to copy from here. Start with it empty,
as in the compose block, which starts and trusts nobody (session cookies are not
marked `Secure` until it is set). Then:

1. Load the site once through the proxy, and read `docker logs taiyaku`. It
   prints a warning that a request carried `X-Forwarded-For` from an address,
   and that address is the proxy as taiyaku sees it (usually the Docker
   network's gateway, not NPM's own address).
2. Put that address in `TAIYAKU_TRUSTED_PROXY` and recreate the container.
3. Load the site again. The warning must not appear for the new requests. If a
   warning still names an address, that address is not covered by the value you
   set.

Taiyaku prints each of the two warnings once per start, so after step 2 look
for new lines, not old ones.

Updates arrive through WUD watching `:latest` (labels `wud.watch=true`,
`wud.watch.digest=true`, `wud.trigger.include=docker.redeploy`). To roll back,
pin a `:sha-…` tag and remove the labels while it is pinned. A `:sha-…` pin names
the code, not one image: a refresh re-pushes the `sha-` tag of the commit it built
(main's tip when it started) with patched packages, so a pin to the current tip
can change under you. To freeze an image, pin its digest instead
(`image: forge.daveynet.xyz/davey/taiyaku@sha256:…`).

Rolling back across a database migration does not work by pinning alone: an
older image refuses a database a newer taiyaku has written ("written by a newer
taiyaku") and, under Docker, restarts in a loop. Go back across a migration by
restoring the backup the server took before it. An open that migrates first
writes `backups/library-<stamp>-before-v<N>.db` beside the database, where `N`
is the version it migrated to, and refuses to migrate if that backup fails. The
file holds the database as it stood, so an image whose schema version is below
`N`, and at least the version the file holds, can open it. `--keep` never
deletes one; delete them by hand. A migration that fails also restarts the
container in a loop, and each start tries it again; a start takes no new
backup while the database file and its `-wal` are unchanged since the newest
`-before-v<N>` backup and that backup opens at the database's version, so the
loop does not fill the disk with copies of one database. Unchanged is judged
by the later of each file's modification and change times: a write or a
`chmod` after the backup makes the next start back up again, and so does a
restore by `mv` where the filesystem sets the change time on a rename, since a
move keeps the modification time.

The label at the sidebar's foot names the build that is running: the first
seven characters of its commit, the same seven CI tags the image with as
`:sha-<7>`. A `cargo build` in a clone asks git for HEAD itself, and marks it
with a `*` (`abc1234*`, which About spells out as "with uncommitted changes")
when tracked files differ from it; untracked files do not count. It asks again
when HEAD moves or git's index changes (a commit, a checkout, a stage), or when
a crate's `src` or `Cargo.toml`, or the workspace's `Cargo.toml` or
`Cargo.lock`, changes; not on every build. So an edit anywhere else (under
`web/`, `docs/` or a crate's `tests/`, say) can carry no `*` until then, and an
edit undone can keep its `*`. Where git is missing, refuses the repository,
or there was no repository yet, none of git's state is watched, so once git
can answer, the label reads "unknown build" until one of those Rust files
changes.

`TAIYAKU_COMMIT`, as the full 40-character lowercase hash, wins over git and
is never marked; anything else in it is ignored. Where neither gives a commit,
the label reads "unknown build". An image build needs `TAIYAKU_COMMIT`,
because `.git` is not in the image's build context, and CI builds the image
with the commit passed in. These commands pass the commit explicitly; the
`cargo build` form names HEAD as a plain build does, but a commit passed in is
never marked, so it drops the `*` from a changed tree:

```sh
docker buildx build --build-arg TAIYAKU_COMMIT=$(git rev-parse HEAD) -t taiyaku .
TAIYAKU_COMMIT=$(git rev-parse HEAD) cargo build --release -p taiyaku-cli
```

On a buildx builder with the `docker-container` driver, give the first
command `--load` too, or its result stays in the builder's cache and never
reaches the local image store.

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

There is exactly one exception, and it is opt-in twice over: the **ComicInfo
write-back** rewrites a work's archives to carry the metadata taiyaku holds.
It needs a per-account capability that no account has by default — not even an
administrator — and it is confirmed per work, against a list naming every file
it will rewrite and their total size. Nothing else in the product writes to
your library, and nothing does it as a side effect of saving something else.

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

taiyaku is GPL-3.0-or-later. See [LICENSE](LICENSE).

The fonts it ships are OFL-1.1: Shippori Mincho B1, Zen Kaku Gothic New, and
the marks subset cut from Zen Kaku Gothic New. The app's `/about/licence`
shows each licence with its full text.
