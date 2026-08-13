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
mod tests {
    use super::*;
    use crate::credit_fixture::CreditFixture;

    #[test]
    fn write_without_a_sidecar_is_refused() {
        let args: Vec<String> = ["a.cbz", "--write"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let err = command(&args).expect_err("refuses");
        assert!(
            err.contains("--write") && err.contains("--sidecar"),
            "the refusal must name both flags: {err}"
        );
    }

    #[test]
    fn a_valued_flag_with_no_value_is_refused_before_any_member_is_read() {
        // O15's rule. The member path does not exist, so a version that resolved
        // options late would fail on a missing file instead of ever refusing.
        let args: Vec<String> = ["no-such-file.cbz", "--sidecar"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let err = command(&args).expect_err("refuses");
        assert!(
            err.contains("--sidecar") && !err.contains("no-such-file"),
            "the flag must be refused before the member is opened: {err}"
        );
    }

    #[test]
    fn a_misspelled_flag_is_refused() {
        // O21's rule.
        let args: Vec<String> = ["a.cbz", "--wrote"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let err = command(&args).expect_err("refuses");
        assert!(err.contains("--wrote"), "the refusal must name it: {err}");
    }

    #[test]
    fn no_members_is_refused_with_the_usage() {
        let err = command(&[]).expect_err("refuses");
        assert!(
            err.contains("credits"),
            "the refusal must show the usage: {err}"
        );
    }

    #[test]
    fn against_without_a_sidecar_is_refused() {
        let args: Vec<String> = ["a.cbz", "--against", "raw"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let err = command(&args).expect_err("refuses");
        assert!(
            err.contains("--against") && err.contains("--sidecar"),
            "the refusal must name both flags: {err}"
        );
    }

    #[test]
    fn emit_fixture_without_a_sidecar_is_refused() {
        let args: Vec<String> = ["a.cbz", "--emit-fixture", "out.json"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let err = command(&args).expect_err("refuses");
        assert!(
            err.contains("--emit-fixture") && err.contains("--sidecar"),
            "the refusal must name both flags: {err}"
        );
    }

    #[test]
    fn no_sidecar_at_all_is_refused_naming_the_flag() {
        let args: Vec<String> = ["a.cbz"].iter().map(ToString::to_string).collect();
        let err = command(&args).expect_err("refuses");
        assert!(err.contains("--sidecar"), "unhelpful: {err}");
    }

    #[test]
    fn a_missing_sidecar_file_is_refused_before_any_member_is_fingerprinted() {
        // I2. `no-such-file.cbz` is never opened: if it were, the refusal
        // would come from `taiyaku_read::member_digest` instead and would
        // name the member rather than the sidecar path.
        let args: Vec<String> = [
            "no-such-file.cbz",
            "--sidecar",
            "/nonexistent-dir/taiyaku.json",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let err = command(&args).expect_err("refuses");
        assert!(
            err.contains("nonexistent-dir") && !err.contains("no-such-file"),
            "the sidecar path must be refused before the member is opened: {err}"
        );
    }

    #[test]
    fn the_table_widens_to_its_names_and_says_nothing_it_did_not_measure() {
        // No cell in this fixture holds a space, so splitting on whitespace
        // recovers the cells and states the columns without pinning the
        // padding, which is asserted separately below.
        fn cells(line: &str) -> Vec<&str> {
            line.split_whitespace().collect()
        }

        // The `-` in the recurs column is the case that matters: an edition of
        // one member has no recurrence to report, and a `0` there would read as
        // a perfect match.
        let rows = [
            Row {
                page: 95,
                member: "a-very-long-chapter-name.cbz".to_string(),
                local: 51,
                member_pages: 52,
                run: 1,
                at_end: true,
                recurs: Some(14),
                nearest: Some("c4.cbz#29".to_string()),
                credits: true,
            },
            Row {
                page: 1,
                member: "c0.cbz".to_string(),
                local: 1,
                member_pages: 44,
                run: 42,
                at_end: false,
                recurs: None,
                nearest: None,
                credits: false,
            },
        ];
        let out = table(&rows);
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 3, "a header and one row each: {out}");
        assert_eq!(cells(lines[0]), HEADERS);
        assert_eq!(
            cells(lines[1]),
            [
                "95",
                "a-very-long-chapter-name.cbz",
                "51/52",
                "1",
                "yes",
                "14",
                "c4.cbz#29",
                "credits"
            ]
        );
        assert_eq!(
            cells(lines[2]),
            ["1", "c0.cbz", "1/44", "42", "no", "-", "-", "-"],
            "an unmeasured recurrence prints as `-`, never as the 0 that would \
             read as a perfect match"
        );

        // The member column is as wide as the widest member, so every column
        // after it lines up rather than being pushed along by one long name.
        // `local` is right-aligned, so it is the right edges that agree.
        let ends_at =
            |line: &str, cell: &str| line.find(cell).expect("cell is in the line") + cell.len();
        assert_eq!(
            ends_at(lines[1], "51/52"),
            ends_at(lines[2], "1/44"),
            "the local column ends in the same place on both rows: {out}"
        );
    }

    /// An unpaired run of `pages` at `local` within a member, flagged or not.
    ///
    /// The slot range is set equal to the page range: nothing this module does
    /// reads it, and a fabricated spread would only invite a reader to trust a
    /// number the fixture invented.
    fn run(page: u32, local: u32, length: u32, at_member_end: bool) -> UnpairedRun {
        UnpairedRun {
            slots: page..page + length,
            pages: page..page + length,
            member: 0,
            local: local..local + length,
            at_member_end,
        }
    }

    #[test]
    fn only_a_short_run_at_a_members_last_page_is_a_candidate() {
        // All four quadrants of the rule, so neither half can be dropped
        // without a failure. The bound is `MAX_CREDIT_RUN`, not a literal:
        // Task 6 raises it from a survey and this test must move with it.
        let over = MAX_CREDIT_RUN + 1;
        assert!(is_candidate(&run(10, 5, MAX_CREDIT_RUN, true)));
        assert!(
            !is_candidate(&run(10, 5, over, true)),
            "a run past the bound is too long to be a card, wherever it sits"
        );
        assert!(
            !is_candidate(&run(10, 0, MAX_CREDIT_RUN, false)),
            "a member's first page is front matter, and front matter is the work"
        );
        assert!(!is_candidate(&run(10, 0, over, false)));
    }

    #[test]
    fn a_written_list_comes_out_ascending() {
        // Runs handed over out of order, which `unpaired_runs` does not do
        // today and which nothing in the format would catch if a later
        // producer did. The list goes through `to_json` and back rather than
        // being asserted in memory, because "written" is the claim.
        let runs = [
            run(200, 9, 1, true),
            run(7, 3, 1, true),
            run(100, 4, 3, false), // not a candidate: must not appear at all
            run(42, 1, 1, true),
        ];
        let pages = credit_pages(&runs);
        assert_eq!(pages, vec![7, 42, 200], "sorted, and candidates only");

        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "fan": {
              "label": "fan",
              "members": [{ "path": "c1.cbz", "sha256": "c1", "pages": 300 }],
              "spreads": []
            }
          },
          "hub": "fan",
          "alignments": []
        }"#;
        let mut doc = Sidecar::from_json(text).expect("fixture parses");
        doc.editions
            .get_mut("fan")
            .expect("the fixture has it")
            .credits = pages;

        let written = Sidecar::from_json(&doc.to_json()).expect("round-trips");
        let credits = &written.editions["fan"].credits;
        assert_eq!(credits, &vec![7, 42, 200]);
        assert!(
            credits.windows(2).all(|pair| pair[0] < pair[1]),
            "the written list must ascend: {credits:?}"
        );
    }

    /// `--write` must not be able to leave a short sidecar behind.
    ///
    /// The same property O2 established for progress and `atomic.rs` now owns
    /// for everything that writes beside a sidecar: the destination is
    /// *replaced* by a rename, never opened for truncation. A crash cannot be
    /// staged in a test, so this pins the mechanism that rules it out — the
    /// idiom `progress.rs`'s own atomicity test set.
    ///
    /// It is observed by **inode identity** rather than by taking write
    /// permission off the existing file, which is what `progress.rs` still
    /// does. `atomic.rs`'s own test records why that approach was abandoned
    /// there: on a filesystem that ignores the read-only bit, or under root,
    /// the permission-denied write it distinguishes "replaced" from
    /// "truncated" by simply succeeds, and the test prints a skip line libtest
    /// captures — a green that measured nothing. A rename gives the path a
    /// different inode; an in-place write keeps the same one, whoever is
    /// running and whatever the filesystem enforces.
    ///
    /// The fixture carries a **second** edition and an alignment between the
    /// two, because that is what is actually at stake: `credits --write`
    /// rewrites the whole document to change one list on one edition, so the
    /// half-written file this rules out would take the other edition's members
    /// and the runs joining them with it.
    #[test]
    fn writing_a_credits_list_replaces_the_sidecar_rather_than_truncating_it() {
        let dir = std::env::temp_dir().join("taiyaku-test-credits-write");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("taiyaku.json");
        let sidecar = path.to_str().expect("utf-8 temp path").to_string();

        let mut doc = fan_and_raw();
        std::fs::write(&path, doc.to_json()).expect("the fixture is written");

        #[cfg(unix)]
        let before = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(&path).expect("stat").ino()
        };

        doc.editions
            .get_mut("fan")
            .expect("the fixture has it")
            .credits = vec![3];
        let wrote = write_sidecar(&sidecar, &doc);

        let read_back = std::fs::read_to_string(&path).expect("the destination is readable");

        #[cfg(unix)]
        let after = {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(&path).expect("stat").ino()
        };

        // Nothing may be left lying beside the sidecar. A failed write and a
        // failed rename each clean up after themselves rather than leaving one
        // file per attempt in the reader's own library directory.
        let strays: Vec<String> = std::fs::read_dir(&dir)
            .expect("the temp dir is readable")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.contains(".tmp"))
            .collect();

        let _ = std::fs::remove_dir_all(&dir);

        wrote.expect("the write succeeds");
        #[cfg(unix)]
        assert_ne!(
            before, after,
            "a rename replaces the destination with a different inode; an \
             in-place truncating write would have kept the same one"
        );
        assert!(strays.is_empty(), "temp files left behind: {strays:?}");

        // Whole, not half: the new list is there and everything the command
        // never touched came through with it.
        let written =
            Sidecar::from_json(&read_back).expect("the destination holds a whole document");
        assert_eq!(written.editions["fan"].credits, vec![3]);
        assert_eq!(
            written.editions["raw"].members.len(),
            1,
            "the untouched edition must survive the rewrite"
        );
        assert_eq!(
            written.alignments.len(),
            1,
            "the alignment joining the two must survive the rewrite"
        );
    }

    /// Distinct hashes at known Hamming distances, built by setting the low
    /// `n` bits of an all-zero hash rather than by writing literals nobody can
    /// check by eye. `PageHash` has neither `Default` nor a bit-flip method,
    /// so this builds the raw `[u64; 4]` words directly instead of widening
    /// the fingerprint type's public API for a test's convenience.
    fn hash_with_bits_set(n: u32) -> PageHash {
        let mut words = [0u64; 4];
        for bit in 0..n {
            words[(bit / 64) as usize] |= 1 << (bit % 64);
        }
        PageHash(words)
    }

    #[test]
    fn the_nearest_neighbour_is_never_in_the_same_member() {
        // Member 0 holds two pages one bit apart. Member 1 holds one page eight
        // bits away. The within-member pair is closer and must be ignored:
        // a chapter repeating its own page says nothing about recurrence.
        let members = vec![
            vec![hash_with_bits_set(0), hash_with_bits_set(1)],
            vec![hash_with_bits_set(8)],
        ];
        let found = recurrences(&members);
        let page0 = found.iter().find(|r| r.page == 0).expect("page 0 reported");
        assert_eq!(
            page0.distance, 8,
            "the 1-bit neighbour is in the same member"
        );
        assert_eq!(page0.nearest_member, 1);
        assert_eq!(page0.nearest_local, 0);
    }

    #[test]
    fn pages_are_numbered_across_the_whole_edition_not_within_a_member() {
        let members = vec![
            vec![hash_with_bits_set(0), hash_with_bits_set(40)],
            vec![hash_with_bits_set(1)],
        ];
        let found = recurrences(&members);
        assert_eq!(
            found.iter().map(|r| r.page).collect::<Vec<_>>(),
            vec![0, 1, 2],
            "member 1's only page is the edition's page 2"
        );
    }

    #[test]
    fn a_single_member_edition_reports_nothing() {
        // Nothing to recur against. Reporting distance 0 here, or the nearest
        // page within the member, would invent a signal.
        let members = vec![vec![hash_with_bits_set(0), hash_with_bits_set(1)]];
        assert!(recurrences(&members).is_empty());
    }

    #[test]
    fn nearest_member_and_local_are_actually_computed_across_three_members() {
        // Three members, so "the other member" is not a well-defined guess
        // and neither is `1 - mi`. Page 0 (member 0, all-zero hash) is
        // farthest from member 1's single page, farther still from member
        // 2's first page, and nearest of all to member 2's *second* page —
        // a nonzero local index in a member that isn't simply "the other
        // one". Distances 50, 60, and 5 are all distinct, so no tie-break
        // can paper over a wrong answer.
        let members = vec![
            vec![hash_with_bits_set(0)],
            vec![hash_with_bits_set(50)],
            vec![hash_with_bits_set(60), hash_with_bits_set(5)],
        ];
        let found = recurrences(&members);
        let page0 = found.iter().find(|r| r.page == 0).expect("page 0 reported");
        assert_eq!(
            page0.distance, 5,
            "member 2's second page is the true nearest neighbour"
        );
        assert_eq!(
            page0.nearest_member, 2,
            "nearest neighbour is in member 2, not member 1 or `1 - mi`"
        );
        assert_eq!(
            page0.nearest_local, 1,
            "nearest neighbour is member 2's second page, not its first"
        );
    }

    /// A minimal one-edition document, for the tests below that only care
    /// about edition matching and the hub refusal, never about pages.
    fn one_edition(name: &str, sha: &str) -> Sidecar {
        let text = format!(
            r#"{{
              "taiyaku": 1,
              "work": {{ "title": "W", "reading": "rtl" }},
              "editions": {{
                "{name}": {{
                  "label": "{name}",
                  "members": [{{ "path": "m.cbz", "sha256": "{sha}", "pages": 1 }}],
                  "spreads": []
                }}
              }},
              "hub": "{name}",
              "alignments": []
            }}"#
        );
        Sidecar::from_json(&text).expect("fixture parses")
    }

    #[test]
    fn no_matching_edition_is_refused_by_name() {
        let doc = one_edition("fan", "c1");
        let err = crate::edition_of_digests(&doc, "taiyaku.json", &["zzz".to_string()])
            .expect_err("refuses");
        assert!(
            err.contains("taiyaku.json") && err.contains("zzz"),
            "the refusal must name the sidecar and the digest it could not place: {err}"
        );
    }

    /// A document with four editions: `hub`, two spokes joined to it directly,
    /// and a third alignment — `spokeA` to `spokeB` — that touches neither.
    /// The buggy version of `resolve_counterpart` mapped every alignment to
    /// "whichever side is not the hub", so `spokeA`-`spokeB` still contributed
    /// `spokeA` even though that edge never reaches the hub at all.
    fn hub_with_a_spoke_to_spoke_edge() -> Sidecar {
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "W", "reading": "rtl" },
          "editions": {
            "hub":    { "label": "hub",    "members": [{ "path": "h.cbz", "sha256": "h", "pages": 1 }], "spreads": [] },
            "spokeA": { "label": "spokeA", "members": [{ "path": "a.cbz", "sha256": "a", "pages": 1 }], "spreads": [] },
            "spokeB": { "label": "spokeB", "members": [{ "path": "b.cbz", "sha256": "b", "pages": 1 }], "spreads": [] },
            "spokeC": { "label": "spokeC", "members": [{ "path": "c.cbz", "sha256": "c", "pages": 1 }], "spreads": [] }
          },
          "hub": "hub",
          "alignments": [
            { "from": "spokeA", "to": "spokeB", "by": "solver", "runs": [] },
            { "from": "hub", "to": "spokeC", "by": "solver", "runs": [] }
          ]
        }"#;
        Sidecar::from_json(text).expect("fixture parses")
    }

    #[test]
    fn the_hub_refusal_only_offers_editions_that_actually_touch_it() {
        let doc = hub_with_a_spoke_to_spoke_edge();

        let err = resolve_counterpart(&doc, "hub", None).expect_err("refuses");

        assert!(err.contains("hub"), "the refusal must name the hub: {err}");
        assert!(
            err.contains("spokeC"),
            "the edition actually joined to the hub must be offered: {err}"
        );
        assert!(
            !err.contains("spokeA") && !err.contains("spokeB"),
            "a spoke-to-spoke edge that never touches the hub must not be offered as an \
             answer, since it has no edge to the hub for `unpaired_runs` to find: {err}"
        );
    }

    #[test]
    fn the_hub_refusal_ends_cleanly_with_no_alignments_at_all() {
        let doc = one_edition("hub", "h");

        let err = resolve_counterpart(&doc, "hub", None).expect_err("refuses");

        assert!(
            !err.trim_end().ends_with("--against:"),
            "a document with nothing to suggest must not trail off after a bare \
             `--against:`: {err}"
        );
    }

    #[test]
    fn against_is_used_verbatim_even_for_the_hub() {
        // `--against` is a choice the caller made, not a default this
        // function has to justify — so it is returned unchecked, hub or not.
        let doc = one_edition("hub", "h");
        assert_eq!(
            resolve_counterpart(&doc, "hub", Some("elsewhere")).unwrap(),
            "elsewhere"
        );
    }

    /// A two-member `fan` edition aligned against a one-member `raw` hub,
    /// with `c1`'s last page (page 3) unpaired — the same shape
    /// `taiyaku-core::credits`'s `a_run_at_a_members_final_index_is_flagged_at_its_end`
    /// fixture uses, reused here because `classify` needs a real alignment to
    /// call `unpaired_runs` through.
    fn fan_and_raw() -> Sidecar {
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "Reversed Order Co.", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 8 }],
              "spreads": []
            },
            "fan": {
              "label": "fan",
              "members": [
                { "path": "c1.cbz", "sha256": "c1", "pages": 4 },
                { "path": "c2.cbz", "sha256": "c2", "pages": 4 }
              ],
              "spreads": []
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "fan", "by": "solver",
              "runs": [{"a":"0-2","b":"0-2"},{"a":"3"},{"a":"4-7","b":"4-7"}] }
          ]
        }"#;
        Sidecar::from_json(text).expect("fixture parses")
    }

    #[test]
    fn page_order_comes_from_the_editions_own_member_order_not_the_argument_order() {
        // fan page 3 — c1's last — is the one unpaired page. Handing
        // `classify` the two members' hashes in the opposite order from the
        // document's own `members` list (c2 before c1) must not move that
        // page onto c2, nor change either the row or the printed line.
        let doc = fan_and_raw();
        let c1_hashes: Vec<PageHash> = (0..4).map(hash_with_bits_set).collect();
        let c2_hashes: Vec<PageHash> = (20..24).map(hash_with_bits_set).collect();

        let forward = vec![
            ("c1".to_string(), c1_hashes.clone()),
            ("c2".to_string(), c2_hashes.clone()),
        ];
        let reversed = vec![("c2".to_string(), c2_hashes), ("c1".to_string(), c1_hashes)];

        let (rows_fwd, line_fwd, pages_fwd) =
            classify(&doc, "fan", "raw", &forward, None).expect("classifies");
        let (rows_rev, line_rev, pages_rev) =
            classify(&doc, "fan", "raw", &reversed, None).expect("classifies");

        assert_eq!(
            pages_fwd, pages_rev,
            "the page list must not depend on argument order"
        );
        assert_eq!(
            line_fwd, line_rev,
            "the printed line must not depend on argument order"
        );
        let simplify = |rows: &[Row]| -> Vec<(u32, String)> {
            rows.iter().map(|r| (r.page, r.member.clone())).collect()
        };
        assert_eq!(simplify(&rows_fwd), simplify(&rows_rev));

        let row = rows_fwd
            .iter()
            .find(|r| r.page == 3)
            .expect("page 3 is reported");
        assert_eq!(
            row.member, "c1.cbz",
            "page 3 is c1's own last page in the document's member order"
        );
    }

    #[test]
    fn the_work_title_is_last_so_its_spaces_cannot_shift_another_field() {
        // `Reversed Order Co.` holds two spaces, which is the whole point: the
        // collectors in `scripts/` split on whitespace and index fields
        // positionally, so a title anywhere but the end silently moves every
        // field after it by a count nobody can predict from the format alone.
        let doc = fan_and_raw();
        let fingerprinted = vec![
            ("c1".to_string(), vec![hash_with_bits_set(0); 4]),
            ("c2".to_string(), vec![hash_with_bits_set(0); 4]),
        ];

        let (_, line, _) = classify(&doc, "fan", "raw", &fingerprinted, None).expect("classifies");

        assert!(
            line.starts_with("credits: edition=fan against=raw"),
            "the fixed fields must lead the line: {line}"
        );

        // Every field before `work=` sits at a known index, whatever the title
        // holds. Asserted as the whole prefix rather than as "work is last", so
        // a field inserted after `bound=` fails here rather than drifting.
        let fields: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(
            &fields[..8],
            &[
                "credits:",
                "edition=fan",
                "against=raw",
                "pages=8",
                "slots=8",
                "runs=1",
                "candidates=1",
                "bound=1",
            ],
            "the fixed fields must be positionally stable: {line}"
        );
        assert_eq!(
            line.split_once(" work=").map(|(_, title)| title),
            Some("Reversed Order Co."),
            "the title must run unambiguously to end of line, spaces and all: {line}"
        );
    }

    #[test]
    fn scan_information_is_read_from_comicinfo_and_absent_where_there_is_none() {
        let with = br#"<ComicInfo><Series>X</Series><ScanInformation>Cirno9baka</ScanInformation><PageCount>17</PageCount></ComicInfo>"#;
        assert_eq!(
            scan_information(&[("ComicInfo.xml".into(), with.to_vec())]),
            Some("Cirno9baka".into())
        );
        let without = br#"<ComicInfo><Series>X</Series></ComicInfo>"#;
        assert_eq!(
            scan_information(&[("ComicInfo.xml".into(), without.to_vec())]),
            None,
            "a ComicInfo with no ScanInformation is absent, not empty"
        );
        assert_eq!(
            scan_information(&[("001.jpg".into(), vec![])]),
            None,
            "an archive with no ComicInfo at all is absent"
        );
    }

    #[test]
    fn the_scan_field_precedes_work_so_a_group_name_with_spaces_cannot_shift_it() {
        // `work=` holds spaces and runs to end of line, which is why it is last.
        // A group name holds spaces too - "Psylocke Scans" - so `scan=` cannot be
        // last as well. It is quoted, and it sits before `work=`.
        let line = credits_line(
            "en-fan",
            "raw-mono",
            226,
            231,
            9,
            1,
            Some("Psylocke Scans"),
            "Kaguya-sama v01",
        );
        let scan_at = line.find("scan=").expect("scan= is present");
        let work_at = line.find("work=").expect("work= is present");
        assert!(scan_at < work_at, "scan= must precede work=: {line}");
        assert!(
            line.contains("scan=\"Psylocke Scans\""),
            "a group name with a space must be quoted: {line}"
        );
    }

    /// A path a flag consumed is not also a member, and a flag is not a member
    /// at all.
    ///
    /// The two filters fail in opposite directions and both are silent. Keeping
    /// the flags would refuse a real invocation for having no members; keeping
    /// only the flag values would try to fingerprint `taiyaku.json`.
    #[test]
    fn a_path_a_flag_consumed_is_not_also_a_member() {
        let args: Vec<String> = [
            "one.cbz",
            "--sidecar",
            "taiyaku.json",
            "--against",
            "raw",
            "two.cbz",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();

        let members = member_args(&args, &[Some("taiyaku.json"), Some("raw"), None]);

        assert_eq!(
            members,
            vec![PathBuf::from("one.cbz"), PathBuf::from("two.cbz")]
        );
    }

    /// A three-member fan edition against a one-member raw hub, with fan pages
    /// 3 to 5 — the whole back half of `c1` — unpaired.
    ///
    /// `fan_and_raw`'s single unpaired page cannot separate a run's *length*
    /// from its arithmetic: a run of one is `4 - 3`, and `4 / 3` is also one.
    /// This fixture's run is three pages long, where subtraction, addition and
    /// division give 3, 9 and 2.
    fn fan_and_raw_with_a_long_run() -> Sidecar {
        let text = r#"{
          "taiyaku": 1,
          "work": { "title": "Long Run", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 10 }],
              "spreads": []
            },
            "fan": {
              "label": "fan",
              "members": [
                { "path": "c1.cbz", "sha256": "c1", "pages": 6 },
                { "path": "c2.cbz", "sha256": "c2", "pages": 4 }
              ],
              "spreads": []
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "fan", "by": "solver",
              "runs": [{"a":"0-2","b":"0-2"},{"a":"3-5"},{"a":"6-9","b":"6-9"}] }
          ]
        }"#;
        Sidecar::from_json(text).expect("fixture parses")
    }

    /// A row reports the run it came from and the recurrence at *its own* page.
    ///
    /// Both are printed beside each other and neither is asserted anywhere
    /// else: the length is arithmetic on a range, and the recurrence is a
    /// lookup by page number into a list that holds one entry for every page in
    /// the edition — so a lookup that matched the wrong way round still finds
    /// something, and prints another page's nearest neighbour beside this one's
    /// run.
    #[test]
    fn a_row_carries_its_runs_length_and_the_recurrence_at_its_own_page() {
        let doc = fan_and_raw_with_a_long_run();
        // `hash_with_bits_set` puts two hashes exactly |a - b| bits apart, so
        // the distances below are chosen rather than observed. Page 0 is 5 bits
        // from c2's first page; page 3 is 1 bit from c2's *third*. A row that
        // took page 0's recurrence would report 5 and `c2.cbz#0`.
        let c1: Vec<PageHash> = [0, 10, 20, 30, 40, 50]
            .iter()
            .map(|n| hash_with_bits_set(*n))
            .collect();
        let c2: Vec<PageHash> = [5, 150, 29, 160]
            .iter()
            .map(|n| hash_with_bits_set(*n))
            .collect();
        let fingerprinted = vec![("c1".to_string(), c1), ("c2".to_string(), c2)];

        let (rows, _, _) = classify(&doc, "fan", "raw", &fingerprinted, None).expect("classifies");

        assert_eq!(rows.len(), 1, "one unpaired run");
        let row = &rows[0];
        assert_eq!(row.page, 3);
        assert_eq!(row.run, 3, "pages 3, 4 and 5");
        assert_eq!(row.recurs, Some(1), "page 3's nearest, not page 0's");
        assert_eq!(row.nearest.as_deref(), Some("c2.cbz#2"));
        assert!(
            !row.credits,
            "three pages is longer than any credits card measured"
        );
    }

    #[test]
    fn the_gotoubun_fixture_reproduces_the_four_candidates_notes_md_records() {
        let fixture = CreditFixture::gotoubun_v01();
        assert_eq!(fixture.work, "gotoubun-v01");
        assert_eq!(fixture.edition, "en-fan");
        assert_eq!(fixture.against, "raw-mono");
        assert_eq!(
            (fixture.pages, fixture.slots),
            (235, 240),
            "docs/notes.md's `credits:` line for gotoubun-v01 en-fan against \
             raw-mono, under \"The two `credits:` lines\""
        );
        assert!(
            !fixture.notes.is_empty(),
            "notes carries why page 43 is unreachable and why c002 local 40 is \
             excluded; a fixture that lost it would lose the reasoning"
        );
        let tally = fixture.tally();
        assert_eq!(
            tally.candidates,
            vec![95, 168, 198, 225],
            "signal 1's candidates on gotoubun-v01 en-fan are fixed by extraction, \
             not by whatever the rule currently emits"
        );
        assert_eq!(tally.hits, 4, "all four candidates are true positives");
        assert_eq!(
            tally.known, 5,
            "five credit pages are known to exist, and page 43 is the one signal 1 \
             cannot reach"
        );
    }

    #[test]
    fn the_kaguya_fixture_reproduces_its_one_candidate_and_it_is_a_false_positive() {
        let fixture = CreditFixture::kaguya_v01();
        let tally = fixture.tally();
        assert_eq!(
            tally.candidates,
            vec![212],
            "c10 19/20 is the one page signal 1 takes here"
        );
        assert_eq!(
            tally.hits, 0,
            "page 212 is the publisher's colour announcement, not the group's matter"
        );
        assert_eq!(tally.known, 0, "this group prints no credit page at all");
    }

    #[test]
    fn the_kobayashi_fixture_reproduces_its_nine_candidates_and_the_run_of_two_it_cannot_reach() {
        let fixture = CreditFixture::kobayashi_v01();
        let tally = fixture.tally();
        assert_eq!(
            tally.candidates,
            vec![16, 31, 46, 59, 74, 91, 104, 119, 134],
            "signal 1's candidates on kobayashi-v01 en-fan are fixed by \
             extraction, not by whatever the rule currently emits"
        );
        assert_eq!(tally.hits, 9, "all nine candidates are true positives");
        assert_eq!(
            tally.known, 10,
            "ten credit pages are known to exist. Page 152 is the tenth and the \
             one signal 1 cannot reach: c10 ends on a run of two, the publisher's \
             greyscale colophon at 151 and the group's card at 152, and \
             MAX_CREDIT_RUN = 1 rejects both"
        );
    }

    #[test]
    fn the_kobayashi_v03_fixture_reproduces_its_eight_candidates_and_the_two_it_cannot_reach() {
        let fixture = CreditFixture::kobayashi_v03();
        let tally = fixture.tally();
        assert_eq!(
            tally.candidates,
            vec![20, 35, 48, 61, 76, 106, 121, 136],
            "signal 1's candidates on kobayashi-v03 en-fan are fixed by \
             extraction, not by whatever the rule currently emits"
        );
        assert_eq!(tally.hits, 8, "all eight candidates are true positives");
        assert_eq!(
            tally.known, 10,
            "ten credit pages are known to exist. Pages 91 and 152 are the two \
             signal 1 cannot reach, and NOT for kobayashi-v01's reason: neither \
             sits in any unpaired run at all, so no bound could admit them. The \
             solver paired both, and signal 1 only ever reads unpaired runs. \
             That is gotoubun-v01 page 43's ceiling, and these are its second \
             and third measured instances — the case where a credits page \
             happens to resemble something in the other edition well enough to \
             be matched"
        );
    }

    #[test]
    fn the_kaguya_v08_fixture_reproduces_its_one_candidate_and_it_is_a_false_positive() {
        let fixture = CreditFixture::kaguya_v08();
        let tally = fixture.tally();
        assert_eq!(
            tally.candidates,
            vec![37],
            "signal 1's one candidate on kaguya-v08 en-fan is fixed by \
             extraction, not by whatever the rule currently emits"
        );
        assert_eq!(
            tally.hits, 0,
            "page 37 is signal 1's second false positive ever — this edition, \
             worked by three scanlation groups, carries no credit page at all"
        );
        assert_eq!(tally.known, 0, "no group prints a credit page here");
    }

    /// `domestic-v01` is a direct counterexample to `UnpairedRun::at_member_end`'s
    /// own doc comment, not merely a miss.
    ///
    /// That comment says the first-index version of the position rule "is zero
    /// for four — what accumulates at a member's start is front matter",
    /// measured on `corpus/gotoubun-v01`'s fan edition alone. Here it is five
    /// for five: all five of this edition's known credit pages — 2, 54, 95,
    /// 130 and 157 — sit at the *start* of a chapter (local index 1 in four of
    /// five, local 2 in the first), and signal 1's rule reads only a member's
    /// *last* index, so it structurally cannot see any of them. Candidates is
    /// empty, not merely short of five.
    #[test]
    fn the_domestic_v01_fixture_is_a_direct_counterexample_to_the_first_index_claim() {
        let fixture = CreditFixture::domestic_v01();
        assert_eq!(fixture.work, "domestic-v01");
        assert_eq!(fixture.against, "en-official");

        assert!(
            fixture.runs().iter().all(|r| !r.at_member_end),
            "every one of this fixture's unpaired runs ends short of its \
             member's last index, which is *why* signal 1 finds nothing here: \
             is_candidate reads at_member_end and nothing else"
        );

        let tally = fixture.tally();
        assert_eq!(
            tally.candidates,
            Vec::<u32>::new(),
            "signal 1 proposes zero candidates on domestic-v01 en-fan"
        );
        assert_eq!(tally.hits, 0, "zero candidates, so zero hits");
        assert_eq!(
            tally.known, 5,
            "five credit pages are known by extraction: 2, 54, 95, 130 and 157 \
             — direct counterexample to UnpairedRun's doc comment, which was \
             measured on gotoubun-v01 alone and says the first-index version of \
             the rule 'is zero for four'. Here it is five for five at a \
             member's start"
        );
        assert_eq!(
            fixture.credit_pages,
            vec![2, 54, 95, 130, 157],
            "the five pages themselves, in ascending order"
        );
    }

    /// `solo-leveling-v01` is the first edition in the corpus where signal
    /// 1's input does not exist, rather than its rule failing to reach a page
    /// it can see.
    ///
    /// The alignment is `REFUSED` — 0 whole pairings of 321 slots, because 61
    /// vertical scroll strips do not correspond to 314 print pages — so there
    /// are no meaningful unpaired runs for signal 1 to read at all. Six
    /// credit pages are known by extraction (0, 20, 21, 39, 40 and 60, two
    /// per chapter), and signal 1 proposes none of them. A future reader
    /// seeing "0 candidates, 6 known" without this context would reasonably
    /// mistake it for the rule breaking; it is instead the absence of the
    /// input the rule needs.
    #[test]
    fn the_solo_leveling_v01_fixture_has_no_alignment_for_signal_1_to_read() {
        let fixture = CreditFixture::solo_leveling_v01();
        let tally = fixture.tally();
        assert_eq!(
            tally.candidates,
            Vec::<u32>::new(),
            "signal 1 proposes zero candidates on solo-leveling-v01 en-fan — \
             not because its rule misses these nine pages, but because the \
             alignment underneath it is REFUSED (0 of 321 slots paired), so \
             there is no meaningful unpaired-run input for signal 1 to read"
        );
        assert_eq!(tally.hits, 0, "zero candidates, so zero hits");
        assert_eq!(
            tally.known, 9,
            "nine credit pages are known by extraction, by far the densest \
             edition in the corpus: THREE per chapter, at local 0, n-2 and \
             n-1 — a staff banner at each chapter's start, a patron thank-you \
             listing donor tiers, and a recruiting card at each chapter's end. \
             Six were found first and the patron pages only later, when a \
             threshold sweep flagged them as false positives and they turned \
             out not to be. This edition has yielded more every time it was \
             looked at, which is why the unswept editions' credit_pages are \
             floors rather than counts"
        );
    }

    #[test]
    fn the_corpus_figures_are_twenty_three_of_twenty_five_precision_and_thirteen_of_fifteen_recall()
    {
        let mut candidates = 0usize;
        let mut hits = 0usize;
        let mut known_swept = 0usize;
        let mut hits_swept = 0usize;
        for fixture in CreditFixture::all() {
            let tally = fixture.tally();
            candidates += tally.candidates.len();
            hits += tally.hits;
            if tally.swept {
                known_swept += tally.known;
                hits_swept += tally.hits;
            }
        }
        assert_eq!(
            (hits, candidates),
            (23, 25),
            "precision: twenty-three of twenty-five candidates across the corpus \
             are true positives — wave 2 adds dungeon-meshi-v01's two true-positive \
             candidates at 40 and 65, while frieren-v01 proposes nothing and \
             domestic-v01 proposes nothing, so neither moves this figure. No wave-2 \
             edition adds a false positive beyond kaguya-v01's and kaguya-v08's. \
             Wave 3 adds solo-leveling-v01, which proposes nothing either — its \
             alignment is REFUSED, so signal 1 has no input to read — and so it \
             does not move this figure"
        );
        assert_eq!(
            (hits_swept, known_swept),
            (13, 15),
            "recall, over swept editions only: thirteen of the fifteen known \
             credit pages are found — gotoubun's paired page 43 and kobayashi-v01's \
             run-of-two page 152 are the two signal 1 cannot reach. All three wave-2 \
             editions carry \"swept\": false (design's §8) so none of them move this \
             figure, even domestic-v01's five known-but-unreached pages. \
             solo-leveling-v01 also carries \"swept\": false, so its six known \
             pages are excluded here too, despite none being reachable"
        );
    }

    #[test]
    fn work_of_derives_the_corpus_directory_not_the_display_title() {
        // The value the two checked-in fixtures key on: `work` is the corpus
        // directory slug, never the human display title
        // (`doc.work.title` holds `Go-Toubun no Hanayome v01`, an unrelated
        // string with spaces in it). This is the derivation `command` skipped
        // when it passed `&doc.work.title` straight to `emit_fixture_json`
        // instead — a bug the old test for `emit_fixture_json` could not have
        // caught, since it passed a literal `"gotoubun-v01"` rather than
        // exercising this function.
        assert_eq!(work_of("corpus/gotoubun-v01/taiyaku.json"), "gotoubun-v01");
        assert_eq!(work_of("corpus/kaguya-v01/taiyaku.json"), "kaguya-v01");
        assert_eq!(
            work_of("/abs/path/to/corpus/gotoubun-v01/taiyaku.json"),
            "gotoubun-v01",
            "an absolute path derives the same slug from its immediate parent"
        );
    }

    #[test]
    fn work_of_falls_back_to_the_whole_path_with_no_usable_parent() {
        // A bare filename with no directory component, and a path whose
        // parent is the filesystem root: neither has a directory-name
        // component to hand back. The fallback must stay non-empty and
        // visibly wrong rather than silently emit "".
        assert_eq!(work_of("taiyaku.json"), "taiyaku.json");
        assert_eq!(work_of("/taiyaku.json"), "/taiyaku.json");
    }

    #[test]
    fn an_emitted_fixture_leaves_the_verdicts_empty_for_a_person_to_fill() {
        let runs = vec![run(95, 51, 1, true), run(1, 1, 42, false)];
        let json = emit_fixture_json("gotoubun-v01", "en-fan", "raw-mono", 235, 240, &runs);
        let back: crate::credit_fixture::CreditFixture =
            serde_json::from_str(&json).expect("an emitted fixture must parse as one");
        assert_eq!(back.runs.len(), 2, "every run is emitted, candidate or not");
        assert!(
            back.credit_pages.is_empty(),
            "the tool must not supply the verdicts it is being measured against"
        );
        assert!(
            !back.swept,
            "an edition is not swept until someone sweeps it"
        );
        assert!(
            back.runs.iter().all(|r| r.slots.is_some()),
            "an emitted run knows its slot range, unlike a hand-written one"
        );
    }

    #[test]
    fn emit_fixture_with_no_value_is_refused_before_any_member_is_read() {
        let err = command(&["--emit-fixture".into()]).unwrap_err();
        assert!(
            err.contains("--emit-fixture"),
            "the refusal must name the flag: {err}"
        );
    }
}
