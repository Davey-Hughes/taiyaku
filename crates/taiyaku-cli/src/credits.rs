//! Signal 2: how close each page comes to a page in a *different* member of the
//! same edition, by dhash.
//!
//! **This is a diagnostic and never a classifier**, and that is a measurement
//! rather than a preference. Run over all 235 pages of `corpus/gotoubun-v01`'s
//! fan edition, its two tightest matches in the whole edition — Hamming
//! distance 2 — are the volume *cover*, which the release carries in two chapter
//! files. Its true positives, the three donation cards, sit at 14 to 24 rather
//! than near zero, because a near-black card has few strong gradients and
//! re-encoding flips the weak ones. And it misses both staff-credit pages, at
//! distances 93 and 99, because each appears exactly once.
//!
//! So it cannot decide, and gating signal 1 on it would veto the two true
//! positives signal 1 uniquely finds. What it is good for is telling a person
//! reviewing a candidate *why* a page recurs and where. See `docs/notes.md`,
//! "The dhash signal finds duplicated pages, which is not the same thing".

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use taiyaku_core::{PageHash, Sidecar, UnpairedRun, unpaired_runs};

use crate::fingerprint::hamming;
// One sidecar writer for the whole crate, shared with `align --write`. A second
// copy of the call is a second place to forget it goes through `atomic::write`.
use crate::write_sidecar;

/// How close one page comes to a page in another member of the same edition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recurrence {
    /// Index in the edition's flat page sequence.
    pub page: u32,
    /// Hamming distance over the 256-bit dhash, 0 to 256.
    pub distance: u32,
    pub nearest_member: usize,
    pub nearest_local: u32,
}

/// A page index or count in this tool's corpus, as `Recurrence` and
/// `classify` spell it.
///
/// # Panics
///
/// Panics if `n` exceeds `u32::MAX`. It cannot at any call site in this file:
/// each derives `n` from the length of, or an index into, a member's or an
/// edition's page-hash list — real archives in this tool's corpus run to a
/// few hundred pages, nowhere near `u32::MAX`.
fn page_index(n: usize) -> u32 {
    u32::try_from(n).expect("page index exceeds u32::MAX")
}

/// For each page, the nearest page in a *different* member.
///
/// Quadratic in the edition's page count, which is fine at the scale this runs
/// at — 235 pages is 27,495 comparisons of four `u64` xors — and worth keeping
/// simple rather than indexing, because the answer is printed for a person and
/// never sits on a request path.
///
/// An edition with fewer than two members reports nothing rather than falling
/// back to within-member neighbours: there is no recurrence to measure, and an
/// invented number would be read as one.
pub fn recurrences(members: &[Vec<PageHash>]) -> Vec<Recurrence> {
    if members.len() < 2 {
        return Vec::new();
    }

    // Flatten once, remembering which member each page came from.
    let mut flat: Vec<(usize, u32, PageHash)> = Vec::new();
    for (mi, pages) in members.iter().enumerate() {
        for (li, h) in pages.iter().enumerate() {
            flat.push((mi, page_index(li), *h));
        }
    }

    flat.iter()
        .enumerate()
        .filter_map(|(page, (mi, _, h))| {
            let (distance, nearest_member, nearest_local) = flat
                .iter()
                .filter(|(mj, _, _)| mj != mi)
                .map(|(mj, lj, g)| (hamming(*h, *g), *mj, *lj))
                .min_by_key(|(d, _, _)| *d)?;
            let page = page_index(page);
            Some(Recurrence {
                page,
                distance,
                nearest_member,
                nearest_local,
            })
        })
        .collect()
}

/// The longest run at a member's last page that still reads as credits.
///
/// **1, and measured rather than provisional.** Surveyed 2026-08-07 with
/// `scripts/credit-survey.sh` over both fan editions the corpus holds —
/// `gotoubun-v01` and `kaguya-v01`, 461 pages, 15 unpaired runs, four
/// scanlation groups — and then by extracting every candidate and looking at
/// it. **Every credit page found anywhere is a run of exactly 1**, all five of
/// them, so there is no measured run of 2 or 3 to raise this for. The corpus's
/// only run of that length at all — Kaguya's pages 2 to 4 — is the volume's two
/// inside covers and the digital edition's own notice page, so the single piece
/// of evidence a longer bound could be tested against argues against it.
///
/// This is therefore not a compromise struck between two readings; 1 is the
/// only length anyone has measured. What it does *not* claim is that no group
/// prints a two-page credits spread: four groups is not the population, and
/// such a spread would go undetected with no sign that it had. See
/// `docs/notes.md`, "A second group's habits, and what the bound is worth".
const MAX_CREDIT_RUN: u32 = 1;

/// Is this unpaired run a scanlation group's own matter?
///
/// Position, then length, and **only signal 1** — `recurrences` is printed
/// beside every row and consulted by nobody, for the reason this module's own
/// doc comment gives.
///
/// The position test is `at_member_end` rather than "at either end of a
/// member". Measured on `corpus/gotoubun-v01`'s fan edition the end rule is
/// four for four and the first-index rule zero for four: what accumulates at a
/// member's *start* is front matter, which belongs in the reading flow.
fn is_candidate(run: &UnpairedRun) -> bool {
    run.at_member_end && run.pages.end - run.pages.start <= MAX_CREDIT_RUN
}

/// The pages the candidate runs cover, ascending.
///
/// Sorted here because `Edition::credits` is documented ascending and nothing
/// in the format enforces it. Today's only producer — `unpaired_runs` — walks
/// the page sequence forward and so is already in order, which is exactly why
/// the guarantee needs stating somewhere it can be tested: a later producer, a
/// survey or a hand merge, would break it silently.
///
/// Whole runs rather than each run's first page, so raising `MAX_CREDIT_RUN`
/// widens the list instead of leaving the rest of a two-page card unlisted.
///
/// `pub(crate)`, not private: this is the whole classification rule, with no
/// hashes or archives involved, so `corpus.rs`'s two gated corpus checks call
/// it directly on a real sidecar's `unpaired_runs` rather than reimplementing
/// `is_candidate`'s position-and-length test against a copy of it.
pub(crate) fn credit_pages(runs: &[UnpairedRun]) -> Vec<u32> {
    let mut pages: Vec<u32> = runs
        .iter()
        .filter(|run| is_candidate(run))
        .flat_map(|run| run.pages.clone())
        .collect();
    pages.sort_unstable();
    pages
}

/// The members named on the command line: every argument that is neither a
/// flag nor the value of one.
///
/// Flag values are excluded by value rather than by position. `--sidecar
/// taiyaku.json` puts a path in the argument list that is not a member, and it
/// is indistinguishable from one by shape alone — so what a flag consumed is
/// what gets taken out.
fn member_args(args: &[String], consumed: &[Option<&str>]) -> Vec<PathBuf> {
    args.iter()
        .filter(|a| !a.starts_with("--"))
        .filter(|a| !consumed.contains(&Some(a.as_str())))
        .map(PathBuf::from)
        .collect()
}

/// What a member is called in the table: the last component of the path the
/// sidecar records, which is the part that distinguishes one chapter from the
/// next.
fn label_of(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
}

/// What a `--emit-fixture`d fixture's `work` field holds: the corpus
/// directory a `--sidecar` path names, not the document's display title.
///
/// `corpus/gotoubun-v01/taiyaku.json`'s immediate parent directory is
/// `gotoubun-v01` — the slug the two checked-in fixtures key on
/// (`gotoubun-v01-en-fan.json` and `kaguya-v01-en-fan.json` both hold
/// `"work": "gotoubun-v01"` / `"work": "kaguya-v01"`, and the
/// `CreditFixture` test at `gotoubun_v01` asserts exactly that string).
/// `doc.work.title` is a different, unrelated string — the human display
/// title (`Go-Toubun no Hanayome v01`), which is why it is never the right
/// source for this field even though it is what `classify`'s printed line
/// carries under `work=`.
///
/// Falls back to the whole sidecar path when there is no usable parent
/// directory component — a bare `taiyaku.json` with no leading directory,
/// or a path whose parent is the filesystem root — the same fallback
/// `label_of` takes for a path with no file name of its own. Visibly wrong
/// rather than silently empty: a fixture nobody can trace to its corpus
/// directory is a defect worth noticing, and an empty string would not be
/// noticed until something downstream choked on it.
fn work_of(sidecar: &str) -> &str {
    Path::new(sidecar)
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or(sidecar)
}

/// One unpaired run, as the table prints it.
///
/// Built up rather than formatted in place so the column widths can be taken
/// over the whole table, and so the layout can be tested without archives.
struct Row {
    page: u32,
    member: String,
    local: u32,
    member_pages: u32,
    run: u32,
    at_end: bool,
    /// `None` where the edition has one member and so has no recurrence to
    /// measure — printed as `-` rather than as a distance nobody computed.
    recurs: Option<u32>,
    nearest: Option<String>,
    credits: bool,
}

const HEADERS: [&str; 8] = [
    "page", "member", "local", "run", "at-end", "recurs", "nearest", "verdict",
];

/// Which columns read left-aligned: the two that hold names, and the verdict.
const LEFT: [bool; 8] = [false, true, false, false, false, false, true, true];

impl Row {
    fn cells(&self) -> [String; 8] {
        [
            self.page.to_string(),
            self.member.clone(),
            format!("{}/{}", self.local, self.member_pages),
            self.run.to_string(),
            if self.at_end {
                "yes".to_string()
            } else {
                "no".to_string()
            },
            self.recurs
                .map_or_else(|| "-".to_string(), |d| d.to_string()),
            self.nearest.clone().unwrap_or_else(|| "-".to_string()),
            if self.credits {
                "credits".to_string()
            } else {
                "-".to_string()
            },
        ]
    }
}

/// One row per unpaired run, widest cell deciding each column.
///
/// Widths are measured rather than fixed because a member is named after the
/// file it came from, and a scanlation group's file names run to fifty
/// characters — a fixed width either truncates them or wastes half the line on
/// a corpus whose members are called `c1.cbz`.
fn table(rows: &[Row]) -> String {
    let cells: Vec<[String; 8]> = rows.iter().map(Row::cells).collect();
    let mut widths = HEADERS.map(str::len);
    for row in &cells {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }

    let header = HEADERS.map(String::from);
    let mut out = String::new();
    for row in std::iter::once(&header).chain(cells.iter()) {
        let line: Vec<String> = row
            .iter()
            .zip(widths)
            .zip(LEFT)
            .map(|((cell, w), left)| {
                if left {
                    format!("{cell:<w$}")
                } else {
                    format!("{cell:>w$}")
                }
            })
            .collect();
        // Trailing pad on the last column is invisible in a terminal and
        // visible in a diff of captured output, so it comes off.
        let _ = writeln!(out, "{}", line.join("  ").trim_end());
    }
    out
}

/// A spoke uses the hub; the hub has to be told.
///
/// Extracted from `command` so the one case with no honest default — the
/// document's hub, asked for unpaired pages with no `--against` — can be
/// tested without an archive: it is a question about `doc.alignments` alone.
///
/// The suggestion list is filtered to alignments that actually touch
/// `edition` before taking "whichever side is not it" (I4). Without that
/// filter a direct edge between two spokes — neither of them the hub — still
/// mapped to `al.from`, an edition with no edge to the hub at all, which then
/// failed downstream with a different, more confusing refusal.
fn resolve_counterpart(
    doc: &Sidecar,
    edition: &str,
    against: Option<&str>,
) -> Result<String, String> {
    if let Some(name) = against {
        return Ok(name.to_string());
    }
    if edition != doc.hub {
        return Ok(doc.hub.clone());
    }

    let mut spokes: Vec<&str> = doc
        .alignments
        .iter()
        .filter(|al| al.from == edition || al.to == edition)
        .map(|al| {
            if al.from == edition {
                al.to.as_str()
            } else {
                al.from.as_str()
            }
        })
        .collect();
    spokes.sort_unstable();
    spokes.dedup();

    Err(if spokes.is_empty() {
        // The empty case: with nothing to suggest, the sentence has to end on
        // its own rather than trail off after a bare `--against:`.
        format!(
            "`{edition}` is this work's hub, so which edition it is unpaired against is \
             a choice rather than a default, and this document has no alignment naming an \
             edition to suggest. Name one with --against."
        )
    } else {
        format!(
            "`{edition}` is this work's hub, so which edition it is unpaired against is \
             a choice rather than a default. Name one with --against: {}",
            spokes.join(", ")
        )
    })
}

/// The scanlation group `ComicInfo.xml` claims, from the first member that
/// carries one.
///
/// **Survey context only.** It names a group and never a page, so there is no
/// sound way for it to promote or demote a candidate, and nothing in
/// `is_candidate` or `credit_pages` reads it. It tells a reviewer whose habit
/// they are adjudicating.
///
/// Absent file, absent element and empty value are all `None` — an edition
/// whose ComicInfo carries `<ScanInformation></ScanInformation>` claims no
/// more than one carrying no element at all.
fn scan_information(entries: &[(String, Vec<u8>)]) -> Option<String> {
    const OPEN: &str = "<ScanInformation>";
    const CLOSE: &str = "</ScanInformation>";
    for (name, bytes) in entries {
        if !name.eq_ignore_ascii_case("ComicInfo.xml") {
            continue;
        }
        let text = String::from_utf8_lossy(bytes);
        let Some(start) = text.find(OPEN) else {
            continue;
        };
        let rest = &text[start + OPEN.len()..];
        let Some(end) = rest.find(CLOSE) else {
            continue;
        };
        let value = rest[..end].trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

/// The one fixed-field line a survey collects, and the only stdout this
/// command produces.
///
/// `work=` is last because a work title holds spaces and runs to end of line,
/// which leaves every field before it positionally fixed —
/// `duplicate-survey.sh` and `margin-survey.sh` established that discipline
/// and `the_work_title_is_last_so_its_spaces_cannot_shift_another_field`
/// holds it. `scan=` holds spaces too — "Psylocke Scans" — so it is quoted
/// and placed immediately before `work=`, and omitted entirely when absent so
/// a collector reading positionally sees a stable prefix either way.
#[allow(
    clippy::too_many_arguments,
    reason = "one argument per printed field, in the fixed order the line prints them in; a \
              bundled struct would still need every field named at the one call site and would \
              lose the field-order-is-the-line-order correspondence this function exists to keep"
)]
fn credits_line(
    edition: &str,
    against: &str,
    pages: u32,
    slots: u32,
    runs: usize,
    candidates: usize,
    scan: Option<&str>,
    work: &str,
) -> String {
    let scan = match scan {
        Some(group) => format!(" scan=\"{group}\""),
        None => String::new(),
    };
    format!(
        "credits: edition={edition} against={against} pages={pages} slots={slots} \
         runs={runs} candidates={candidates} bound={MAX_CREDIT_RUN}{scan} work={work}"
    )
}

/// The post-fingerprint half of `command`: everything driven by the document
/// and the members' hashes, with no filesystem or archive access, so it can be
/// tested without either.
///
/// `fingerprinted` is `(digest, hashes)` per member, in whatever order the
/// caller fingerprinted them — the command line's order, which a shell glob
/// need not agree with. **Page order in the result comes from the matched
/// `Edition`'s own `members` list, never from this order** — `edition_of`
/// matched a sorted set, so the argument order it accepted said nothing about
/// which chapter is first, and a shell glob orders `c10.5` before `c1` in any
/// case. `page_order_comes_from_the_editions_own_member_order` pins this by
/// handing the two hashes in reverse and checking nothing moves.
///
/// `scan` is `scan_information`'s already-computed answer, passed in rather
/// than read here, for the same reason `fingerprinted` is: reading an archive
/// is the caller's job, so this stays testable without one.
fn classify(
    doc: &Sidecar,
    edition: &str,
    counterpart: &str,
    fingerprinted: &[(String, Vec<PageHash>)],
    scan: Option<&str>,
) -> Result<(Vec<Row>, String, Vec<u32>), String> {
    let runs = unpaired_runs(doc, edition, counterpart)?;

    let target = doc
        .editions
        .get(edition)
        .expect("caller resolved an edition of this document");

    let by_digest: BTreeMap<&str, &Vec<PageHash>> = fingerprinted
        .iter()
        .map(|(sha, hashes)| (sha.as_str(), hashes))
        .collect();
    let mut ordered: Vec<Vec<PageHash>> = Vec::new();
    for member in &target.members {
        let hashes = *by_digest
            .get(member.sha256.as_str())
            .expect("caller fingerprinted every member's sha256");
        // The two signals number pages independently — one from the archives,
        // one from the document's recorded counts — so a disagreement would
        // print one page's recurrence beside another page's run rather than
        // fail.
        if page_index(hashes.len()) != member.pages {
            return Err(format!(
                "`{}` holds {} pages but the sidecar records {} for it, so the two signals \
                 would be numbering different pages; re-run align to bring the document up \
                 to date",
                member.path,
                hashes.len(),
                member.pages
            ));
        }
        ordered.push(hashes.clone());
    }

    let recurring = recurrences(&ordered);
    let labels: Vec<&str> = target
        .members
        .iter()
        .map(|member| label_of(&member.path))
        .collect();

    let mut rows: Vec<Row> = runs
        .iter()
        .map(|run| {
            let near = recurring.iter().find(|r| r.page == run.pages.start);
            Row {
                page: run.pages.start,
                member: labels[run.member].to_string(),
                local: run.local.start,
                member_pages: target.members[run.member].pages,
                run: run.pages.end - run.pages.start,
                at_end: run.at_member_end,
                recurs: near.map(|r| r.distance),
                nearest: near.map(|r| format!("{}#{}", labels[r.nearest_member], r.nearest_local)),
                credits: is_candidate(run),
            }
        })
        .collect();
    // Shortest run first, so the rows a person actually has to judge are at the
    // top and a chapter-length gap is at the bottom where it belongs.
    rows.sort_by_key(|row| (row.run, row.page));

    let pages = credit_pages(&runs);
    // Fixed fields in a fixed order, the discipline `--stats` and `duplicates`'
    // `set:` line set, so a survey can read down a column. `work` is **last**,
    // and that placement is the whole reason it is safe to carry: a work title
    // holds spaces — `Go-Toubun no Hanayome v01` is three of them — and every
    // collector in `scripts/` splits these lines on whitespace and takes fields
    // positionally. Leading with it shifts every later field by an unknowable
    // amount; trailing with it leaves every field before it positionally fixed
    // and lets the free text run unambiguously to end of line. It stays on the
    // row rather than moving to a header because a collected table row has to
    // identify itself without the collector tracking state. `candidates` counts
    // pages rather than runs because it is the length of the list `--write`
    // writes; with `MAX_CREDIT_RUN` at 1 the two coincide.
    let line = credits_line(
        edition,
        counterpart,
        target.total_pages(),
        page_index(target.slots(doc.work.reading).len()),
        runs.len(),
        pages.len(),
        scan,
        &doc.work.title,
    );

    Ok((rows, line, pages))
}

/// The structural half of a `credit_fixture::CreditFixture`, as JSON.
///
/// `credit_pages` and `swept` are deliberately empty and false: they are a
/// person's adjudication, and a fixture whose verdicts came from the code
/// under test would assert nothing. `notes` is empty for the same reason.
fn emit_fixture_json(
    work: &str,
    edition: &str,
    against: &str,
    pages: u32,
    slots: u32,
    runs: &[UnpairedRun],
) -> String {
    let rows: Vec<serde_json::Value> = runs
        .iter()
        .map(|r| {
            serde_json::json!({
                "pages": [r.pages.start, r.pages.end],
                "member": r.member,
                "local": [r.local.start, r.local.end],
                "at_member_end": r.at_member_end,
                "slots": [r.slots.start, r.slots.end],
            })
        })
        .collect();
    let doc = serde_json::json!({
        "work": work,
        "edition": edition,
        "against": against,
        "pages": pages,
        "slots": slots,
        "swept": false,
        "runs": rows,
        "credit_pages": [],
        "notes": "",
    });
    serde_json::to_string_pretty(&doc).expect("a fixture is always serialisable")
}

/// `taiyaku credits` — propose the credit pages of one edition.
///
/// Prints; writes the sidecar only under `--write`, the discipline `inventory`
/// and `duplicates` set. `--emit-fixture PATH` writes a second, unrelated
/// file — the structural half of a `credit_fixture::CreditFixture` — and does
/// so independently of `--write`, since a fixture is not the sidecar. Both
/// signals are reported on every row — including the rows nothing classifies,
/// since signal 2 is what a person judges a candidate *by* — but only signal 1
/// classifies, for the reason this module's own doc comment gives.
///
/// The table goes to stderr and the one fixed-field `credits:` line to stdout,
/// so a survey can collect the lines without the tables, exactly as
/// `duplicates` splits its tables from its `set:` line.
pub fn command(args: &[String]) -> Result<(), String> {
    const KNOWN: &[&str] = &[
        "--sidecar",
        "--against",
        "--cache",
        "--write",
        "--emit-fixture",
    ];
    crate::reject_unknown_flags("credits", args, KNOWN)?;

    // Every valued flag resolves before a member is opened, so a typo does not
    // cost the seconds it takes to fingerprint an edition.
    let sidecar = crate::option_value(args, "--sidecar", "a path to a taiyaku.json")?;
    let against = crate::option_value(args, "--against", "an edition name")?;
    let cache = crate::option_value(args, "--cache", "a directory")?;
    let emit_fixture =
        crate::option_value(args, "--emit-fixture", "a path to write the fixture to")?;
    let write = args.iter().any(|a| a == "--write");

    if write && sidecar.is_none() {
        return Err("--write needs --sidecar: there is nothing to write the list into".into());
    }
    if against.is_some() && sidecar.is_none() {
        return Err("--against names an edition of a sidecar, so it needs --sidecar too".into());
    }
    if emit_fixture.is_some() && sidecar.is_none() {
        return Err(
            "--emit-fixture writes the runs a sidecar's alignment produced, so it needs \
             --sidecar too"
                .into(),
        );
    }

    // Four consumed flags now, not three. A path this list forgets becomes a
    // member, and a member that is really a flag argument fingerprints as a
    // missing file rather than refusing.
    let members = member_args(args, &[sidecar, against, cache, emit_fixture]);
    if members.is_empty() {
        return Err(USAGE.into());
    }

    // Refused rather than ignored, the rule `align` applies to every flag that
    // cannot take effect. What classifies a page is the alignment, and the
    // alignment lives in the sidecar; signal 2 alone is a diagnostic this
    // module is explicit about never letting decide, so a run without a
    // sidecar could only print a table of `-`.
    let Some(sidecar) = sidecar else {
        return Err(
            "credits needs --sidecar: a page is proposed because an alignment never reaches \
             it, and the alignment is what the sidecar holds"
                .into(),
        );
    };

    // I2: read and parse before a single member is fingerprinted. A mistyped
    // path or an unreadable document is the same class of mistake as a wrong
    // flag, and belongs on the same side of the cost line — before the
    // seconds a fingerprint pass takes, not after them.
    let text = std::fs::read_to_string(sidecar).map_err(|e| format!("{sidecar}: {e}"))?;
    let mut doc = Sidecar::from_json(&text)?;

    // Through the same cached path `cached_edition` is built from, so a member
    // already fingerprinted by `align` costs nothing here. Per member and in
    // pages, rather than the flat slot list `cached_edition` returns:
    // `recurrences` needs the members kept apart to tell a recurrence from a
    // chapter repeating its own page, and both signals index pages.
    //
    // `ComicInfo.xml` is gathered in this same loop, member by member, rather
    // than in a pass of its own: the cache above only ever keeps a member's
    // page hashes, so it can answer nothing about a non-page entry and
    // `member_entry` opens the archive on every run, cache hit or miss. There
    // is no already-open handle downstream of this loop for a later pass to
    // reuse either. Folding the read in here is what "already being opened"
    // can mean given that: one loop over `members`, not two — and the extra
    // open is accepted because `scan` is survey context that classifies
    // nothing, so it costs a survey and never a page turn.
    let cache_root = PathBuf::from(cache.unwrap_or(".taiyaku-cache"));
    let mut fingerprinted: Vec<(String, Vec<PageHash>)> = Vec::new();
    let mut comic_infos: Vec<(String, Vec<u8>)> = Vec::new();
    for member in &members {
        let digest = taiyaku_read::member_digest(member)?;
        let (pages, _hit) = crate::cache::member_pages(&cache_root, member)?;
        fingerprinted.push((digest, pages.iter().map(|page| page.hash).collect()));
        if let Some(bytes) = taiyaku_read::member_entry(member, "ComicInfo.xml")? {
            comic_infos.push(("ComicInfo.xml".to_string(), bytes));
        }
    }
    let scan = scan_information(&comic_infos);

    // Content, not paths: `edition_of` compares the sorted sha256 set, which is
    // the idiom `merge_spoke`'s own guard states. A renamed but byte-identical
    // archive is still this edition.
    let digests: Vec<String> = fingerprinted.iter().map(|(sha, _)| sha.clone()).collect();
    let edition = crate::edition_of_digests(&doc, sidecar, &digests)?;
    let counterpart = resolve_counterpart(&doc, &edition, against)?;

    let (rows, line, pages) = classify(
        &doc,
        &edition,
        &counterpart,
        &fingerprinted,
        scan.as_deref(),
    )?;

    eprint!("{}", table(&rows));

    if let Some(path) = emit_fixture {
        // Recomputed rather than threaded out of `classify`: the runs it
        // built `rows` from carry no `slots` range and name a member by its
        // printed label rather than its index, so `Row` cannot supply what a
        // `CreditFixture` needs back. `unpaired_runs` is pure structure over
        // `doc.alignments` — no archive, no fingerprint — so asking it twice
        // costs nothing an edition's page count would notice.
        let runs = unpaired_runs(&doc, &edition, &counterpart)?;
        let target = doc
            .editions
            .get(&edition)
            .expect("edition_of_digests named an edition of this document");
        let json = emit_fixture_json(
            work_of(sidecar),
            &edition,
            &counterpart,
            target.total_pages(),
            page_index(target.slots(doc.work.reading).len()),
            &runs,
        );
        std::fs::write(path, json).map_err(|e| format!("{path}: {e}"))?;
    }

    if write {
        // Wholesale rather than merged: the field holds no hand entries, so a
        // re-run after the bound changes must leave nothing stale.
        let count = pages.len();
        doc.editions
            .get_mut(&edition)
            .expect("edition_of_digests named an edition of this document")
            .credits = pages;
        write_sidecar(sidecar, &doc)?;
        eprintln!("wrote {count} credits page(s) for `{edition}` to {sidecar}");
    }

    // M6: printed only once whatever `--write` promised has actually happened.
    // On an unwritable target `write_sidecar` above returns before this is
    // reached, so a failed run never leaves a well-formed `credits:` line on
    // stdout for a survey to collect.
    println!("{line}");

    Ok(())
}

const USAGE: &str = "\
taiyaku credits <member>... --sidecar FILE [--against EDITION] [--cache DIR] [--write] \
[--emit-fixture PATH]

Propose the pages of one edition that are a scanlation group's own matter
rather than the work. Prints what it found; writes the sidecar only under
--write, and --emit-fixture PATH writes a fixture file independently of it.";

#[cfg(test)]
mod tests;
