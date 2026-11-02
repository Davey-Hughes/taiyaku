//! What a detector found, against what a person drew.
//!
//! **Five outcomes per truth box, not an IoU pass or fail** — one-to-one,
//! merged, split, fragment, missed — and the reason is recorded rather than
//! assumed. The detector's two known failure modes both
//! score as a flat miss under IoU@0.5 and it cannot tell them apart:
//! `docs/notes.md` records *both* on one edition at one gap — panels
//! over-merged, and "blocks" that are vertical slices of one word. A single
//! overlap number reports "recall 0.0" for each and says nothing about which.
//!
//! This is the region-evaluation scheme from document-layout analysis
//! (Shafait/Breuel, ICDAR). The payoff is that "this edition is finding
//! panels" becomes a merge count on the first line of output.
//!
//! Native-only, and not in `taiyaku-core`: nothing in the browser scores
//! anything, and putting it there would be speculative.

use taiyaku_core::{Kind, Region, TruthRegion};

/// What became of one truth box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// One prediction, one truth box, and each covers most of the other.
    OneToOne,
    /// Its prediction is mostly something else: it also swallows other
    /// labelled boxes, or it swallows this one inside a far larger area — the
    /// whole-panel-around-one-balloon case (`docs/notes.md`,
    /// "What the pixels said, and the noun this section had wrong").
    Merged,
    /// Two or more predictions cover it.
    Split,
    /// Its prediction sits inside it and covers little of it — "a **letter
    /// fragment**, a vertical slice of one word" (`docs/notes.md`,
    /// "What the pixels said, and the noun this section had wrong").
    Fragment,
    /// Nothing significantly overlaps it.
    Missed,
}

/// One page's result. `outcomes` is parallel to the *scored* truth boxes —
/// ignored ones are not in it, and are counted separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scored {
    pub outcomes: Vec<Outcome>,
    pub spurious: usize,
    pub ignored: usize,
}

/// How much of the smaller box must lie inside the larger for the two to count
/// as describing the same thing.
///
/// **A starting value, not a measurement.** It gets settled the way
/// `scripts/region-survey.sh`'s own header says constants get settled in this
/// repository — by drawing the proposed matches over the pages and looking at
/// them, not from aggregate output alone. Asserting this as measured before
/// that is done is a defect.
pub const COVERAGE: f64 = 0.5;

/// Area of the overlap between two page-fraction boxes.
///
/// A box with a non-finite coordinate overlaps nothing. `f64::max` and
/// `f64::min` *discard* NaN rather than propagate it — `f64::NAN.max(v)` is
/// `v` — so without this guard an intersection against a NaN box collapses
/// onto the other box's own edges and reports a full overlap, turning
/// corrupt input into a perfect hit.
fn intersection(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> f64 {
    if [a.0, a.1, a.2, a.3, b.0, b.1, b.2, b.3]
        .iter()
        .any(|v| !v.is_finite())
    {
        return 0.0;
    }
    let x0 = a.0.max(b.0);
    let y0 = a.1.max(b.1);
    let x1 = (a.0 + a.2).min(b.0 + b.2);
    let y1 = (a.1 + a.3).min(b.1 + b.3);
    (x1 - x0).max(0.0) * (y1 - y0).max(0.0)
}

/// Whether two boxes describe the same thing — **association only**.
///
/// Against the **smaller** of the two areas rather than the union, which is
/// what makes a merge distinguishable from a miss: a page-spanning prediction
/// covers a small balloon almost entirely while its union with that balloon is
/// nearly all prediction, so a union-based rule scores the pair as unrelated
/// and the merge disappears into the miss column.
///
/// Deliberately one-directional, and it must stay that way. Requiring *both*
/// directions here would unassociate the split case — each of three slices
/// covers 30% of the one word it slices — and the box would report `Missed`
/// when three predictions are sitting on it. Which of the two directions is
/// short is a question about the *shape* of an association that already
/// exists, so it is asked in [`classify_single_cover`], not here.
fn significant(t: &TruthRegion, p: &Region, coverage: f64) -> bool {
    let t_box = (t.x, t.y, t.w, t.h);
    let p_box = (p.x, p.y, p.w, p.h);
    let smaller = (t.w * t.h).min(p.w * p.h);
    if smaller <= 0.0 {
        return false;
    }
    let inter = intersection(t_box, p_box);
    // Overlap is required, not merely a satisfied ratio. `COVERAGE` is a
    // provisional constant a human is expected to sweep, and at `0.0` the
    // ratio alone is vacuously true for *every* pair on the page including
    // disjoint ones — a sweep through zero would print a flawless page rather
    // than an obvious error. At zero this reads "any overlap at all".
    inter > 0.0 && inter / smaller >= coverage
}

/// The shape of a single association: which way round the coverage is short.
///
/// `reach` is how many *scored* truth boxes the prediction covers. Deciding
/// on that count alone is the defect this instrument exists to catch: a
/// prediction can be a whole panel and still touch exactly one labelled
/// balloon, and the table in `docs/notes.md`,
/// "What the pixels said, and the noun this section had wrong", lists such
/// panels. Counting says 1:1 and credits the detector for returning panels.
/// Geometry says the prediction is 99% something else.
fn classify_single_cover(t: &TruthRegion, p: &Region, reach: usize, coverage: f64) -> Outcome {
    if reach > 1 {
        return Outcome::Merged;
    }
    // Both areas are strictly positive and finite: `significant` returned true
    // for this pair, and it refuses a non-positive smaller area and a
    // non-finite coordinate.
    let inter = intersection((t.x, t.y, t.w, t.h), (p.x, p.y, p.w, p.h));
    let of_prediction = inter / (p.w * p.h);
    let of_truth = inter / (t.w * t.h);
    if of_prediction >= coverage && of_truth >= coverage {
        Outcome::OneToOne
    } else if of_prediction < coverage {
        // Mostly something else — a panel, art and all, around one balloon.
        Outcome::Merged
    } else {
        // Inside the box, covering little of it — a slice of one word.
        Outcome::Fragment
    }
}

/// Whether an *ignored* truth box absorbs a prediction.
///
/// A zero-area ignored box is a click without a drag on the ignore hatch, and
/// `significant` refuses it along with every other degenerate box. Refusing it
/// here would convert the one labelling slip that *forgives* a prediction into
/// a false positive — the slip would move the score in the punishing
/// direction, which is exactly backwards for a mis-click on the hatch. A
/// zero-area ignored box therefore still absorbs any prediction containing its
/// point. A zero-area *scored* box is left refused: it has no area to be found
/// a fraction of, so there is no honest outcome to award it.
fn absorbs(t: &TruthRegion, p: &Region, coverage: f64) -> bool {
    if t.w * t.h <= 0.0 {
        return p.w > 0.0
            && p.h > 0.0
            && [p.x, p.y, p.w, p.h, t.x, t.y].iter().all(|v| v.is_finite())
            && (p.x..=p.x + p.w).contains(&t.x)
            && (p.y..=p.y + p.h).contains(&t.y);
    }
    significant(t, p, coverage)
}

/// Score one page.
///
/// Ignored truth boxes are removed from scoring but still absorb predictions:
/// a prediction landing on a sound effect flagged `ignore` is neither a hit
/// nor a false positive, which is the whole point of the flag.
#[must_use]
pub fn score_page(truth: &[TruthRegion], predicted: &[Region], coverage: f64) -> Scored {
    let scored: Vec<&TruthRegion> = truth.iter().filter(|t| !t.ignore).collect();
    let ignored_boxes: Vec<&TruthRegion> = truth.iter().filter(|t| t.ignore).collect();

    // For each scored truth box, which predictions cover it.
    let covers: Vec<Vec<usize>> = scored
        .iter()
        .map(|t| {
            predicted
                .iter()
                .enumerate()
                .filter(|(_, p)| significant(t, p, coverage))
                .map(|(i, _)| i)
                .collect()
        })
        .collect();

    // For each prediction, how many scored truth boxes it covers.
    let mut reach = vec![0usize; predicted.len()];
    for hits in &covers {
        for &i in hits {
            reach[i] += 1;
        }
    }

    let outcomes = covers
        .iter()
        .zip(&scored)
        .map(|(hits, t)| match hits.as_slice() {
            [] => Outcome::Missed,
            [only] => classify_single_cover(t, &predicted[*only], reach[*only], coverage),
            _ => Outcome::Split,
        })
        .collect();

    // A prediction is spurious only if it reaches no truth box at all —
    // scored or ignored.
    let spurious = predicted
        .iter()
        .enumerate()
        .filter(|(i, p)| reach[*i] == 0 && !ignored_boxes.iter().any(|t| absorbs(t, p, coverage)))
        .count();

    Scored {
        outcomes,
        spurious,
        ignored: ignored_boxes.len(),
    }
}

use std::collections::BTreeMap;

/// Found against drawn, for one kind.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KindTally {
    /// Boxes that came back `OneToOne`. A merge, a split and a fragment are
    /// failures, not partial credit: counting them here would let a detector
    /// returning whole panels score as working.
    pub found: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub one_to_one: usize,
    pub merged: usize,
    pub split: usize,
    pub fragment: usize,
    pub missed: usize,
    pub spurious: usize,
    pub ignored: usize,
    pub pages: usize,
    pub by_kind: BTreeMap<Kind, KindTally>,
}

impl Report {
    /// Fold one page's result in.
    ///
    /// Takes the page's truth alongside its `Scored` because `outcomes` is
    /// parallel to the *scored* boxes, so recovering each outcome's kind means
    /// walking the same filter — done here once rather than at every call site.
    pub fn add_page(&mut self, truth: &[TruthRegion], scored: &Scored) {
        self.pages += 1;
        self.spurious += scored.spurious;
        self.ignored += scored.ignored;
        for (region, outcome) in truth.iter().filter(|t| !t.ignore).zip(&scored.outcomes) {
            let tally = self.by_kind.entry(region.kind).or_default();
            tally.total += 1;
            match outcome {
                Outcome::OneToOne => {
                    self.one_to_one += 1;
                    tally.found += 1;
                }
                Outcome::Merged => self.merged += 1,
                Outcome::Split => self.split += 1,
                Outcome::Fragment => self.fragment += 1,
                Outcome::Missed => self.missed += 1,
            }
        }
    }

    /// A rate rather than a count, because a candidate may legitimately return
    /// more predictions in total — which is why the promotion rule states this
    /// term per page and the 1:1 term as a raw count over a fixed dev set.
    #[must_use]
    pub fn spurious_per_page(&self) -> f64 {
        if self.pages == 0 {
            return 0.0;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "both are counts of boxes and pages in one scoring run, far below \
                      f64's 2^53 exact-integer limit; the dev set is 45 pages."
        )]
        {
            self.spurious as f64 / self.pages as f64
        }
    }

    /// The whole result, as one block a person reads.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "pages={} 1:1={} merged={} split={} fragment={} missed={} spurious={} ({:.2}/page) \
             ignored={}\n",
            self.pages,
            self.one_to_one,
            self.merged,
            self.split,
            self.fragment,
            self.missed,
            self.spurious,
            self.spurious_per_page(),
            self.ignored,
        ));
        for (kind, tally) in &self.by_kind {
            let recall = if tally.total == 0 {
                0.0
            } else {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "counts of boxes in one scoring run; see spurious_per_page above."
                )]
                {
                    tally.found as f64 / tally.total as f64
                }
            };
            out.push_str(&format!(
                "  {kind:?} recall={recall:.2} ({}/{})\n",
                tally.found, tally.total
            ));
        }
        out
    }
}

const USAGE: &str = "\
usage: taiyaku score <member>... --sidecar <taiyaku.json> [--coverage F]
       taiyaku score --sample <corpus-root>

Scores what `detect_page` finds against what a person drew in
<stem>.truth.json. Prints five outcomes per truth box, recall per kind, and
how many boxes were flagged `ignore`.

--sample scores nothing: it prints the labelling worklist — which pages of
which editions to draw — which is where a run of this command starts.
";

/// One edition's labelled pages, resolved and ready to decode.
///
/// Extracted from `command` so `truth_population.rs` measures **the same pages
/// this command scores**. A measurement taken on a different page set than the
/// score is a defect nothing would catch: both would print plausible numbers.
///
/// `truth` and `pages` are parallel and ascending. `pages` is undecoded bytes
/// rather than `GrayImage`, because `decode_all` bounds memory by decoding
/// large pages one at a time.
pub(crate) struct Labelled {
    pub truth: Vec<Vec<taiyaku_core::TruthRegion>>,
    pub pages: Vec<taiyaku_read::Page>,
}

pub(crate) fn resolve_labelled(
    sidecar: &str,
    members: &[std::path::PathBuf],
) -> Result<Labelled, String> {
    // Read and parse before a single page is decoded.
    let text = std::fs::read_to_string(sidecar).map_err(|e| format!("{sidecar}: {e}"))?;
    let doc = taiyaku_core::Sidecar::from_json(&text)?;

    // A missing truth document is an error naming the path, not an empty
    // report. Scoring against no labels would print perfect precision and zero
    // recall, which reads like a broken detector rather than a missing file.
    let truth_path = taiyaku_core::TruthDoc::path_beside(std::path::Path::new(sidecar));
    let truth_text = std::fs::read_to_string(&truth_path).map_err(|e| {
        format!(
            "{}: {e} — nothing has been labelled here",
            truth_path.display()
        )
    })?;
    let truth = taiyaku_core::TruthDoc::from_json(&truth_text)?;

    // Content, not order: the sorted sha256 set identifies the edition, so the
    // order these were typed in does not matter for identification.
    let mut digests = Vec::new();
    for member in members {
        digests.push(taiyaku_read::member_digest(member)?);
    }
    let edition = crate::edition_of_digests(&doc, sidecar, &digests)?;

    let Some(labelled) = truth.editions.get(&edition) else {
        return Err(format!(
            "{} holds no labels for edition `{edition}`",
            truth_path.display()
        ));
    };

    // `docs/sidecar.md` rule 1 is normative: the matched edition's own
    // `members` array *is* the page order and must not be re-sorted. Paths are
    // relative to the sidecar and may point into unrelated trees, so their
    // names need not sort into reading order at all. This is the block
    // in `regions::command` that builds `ordered_members`; it is restated
    // rather than shared because that one is inline in a longer function.
    let target = doc
        .editions
        .get(&edition)
        .expect("edition_of_digests matched an edition of this document");
    let by_digest: std::collections::BTreeMap<&str, &std::path::PathBuf> = digests
        .iter()
        .zip(members)
        .map(|(sha256, path)| (sha256.as_str(), path))
        .collect();
    let mut ordered_members: Vec<std::path::PathBuf> = Vec::new();
    let mut ordered_digests: Vec<String> = Vec::new();
    for member in &target.members {
        ordered_members.push(
            (*by_digest
                .get(member.sha256.as_str())
                .expect("edition_of_digests matched this exact set of digests"))
            .clone(),
        );
        ordered_digests.push(member.sha256.clone());
    }

    // Labels drawn on a replaced archive put a box on the wrong art on the
    // *right* page, which is harder to notice than a wrong page.
    //
    // Against `ordered_digests` — the edition's own member order — and not
    // against `digests`, which is argv order. `edition_of_digests` above
    // matched by *sorted set*, deliberately, so that the order these were
    // typed in does not decide which edition they are; `is_stale_for` compares
    // element-wise, deliberately, because content-in-order is what says the
    // labels describe these pages. Feeding argv order to an element-wise
    // comparison makes the two rules contradict each other, and a shell glob
    // is exactly where they disagree: `en-fan/*.cbz` expands to
    // `…c004, c005.5, c005` (`5` sorts before `c`) while the sidecar
    // lists `…c004, c005, c005.5`.
    if labelled.is_stale_for(&ordered_digests) {
        return Err(format!(
            "{} was drawn against different files; re-label or restore the archive",
            truth_path.display()
        ));
    }

    // **Only the labelled pages are decoded.** `decode_all` decodes everything
    // it is handed, so handing it the whole edition would pay for a
    // full-resolution decode of every page to score only the labelled ones.
    // Moved rather than cloned, so `Page` needs no `Clone`.
    let pages = taiyaku_read::read_members_in_order(&ordered_members)?;
    let available = pages.len();

    // A labelled page this edition cannot reach is named and refused, never
    // dropped. The filter below keeps only pages the truth document mentions,
    // so a truth key past the end simply never appears — and the report then
    // prints a smaller `pages=` with nothing anywhere saying which page went
    // unmeasured. Somebody's labelling silently not being measured is the
    // exact failure this whole sub-project exists to catch, and
    // `score_corpus.rs` already fails on it by name.
    let unreachable: Vec<String> = labelled
        .pages
        .keys()
        .filter(|number| usize::try_from(**number).is_ok_and(|n| n >= available))
        .map(u32::to_string)
        .collect();
    if !unreachable.is_empty() {
        return Err(format!(
            "{} holds labels for page(s) {} of edition `{edition}`, which has {available} \
             page(s) — the labels and the archives disagree about how long this edition is",
            truth_path.display(),
            unreachable.join(", ")
        ));
    }

    let mut truth: Vec<Vec<taiyaku_core::TruthRegion>> = Vec::new();
    let mut subset: Vec<taiyaku_read::Page> = Vec::new();
    for (index, page) in pages.into_iter().enumerate() {
        let number = u32::try_from(index).expect("a page index fits in u32");
        // Pages absent from the truth document are skipped: nobody labelled
        // them, and scoring against no labels is not zero recall, it is no
        // measurement.
        let Some(drawn) = labelled.pages.get(&number) else {
            continue;
        };
        truth.push(drawn.clone());
        subset.push(page);
    }

    Ok(Labelled {
        truth,
        pages: subset,
    })
}

/// Score `detect_page` against a truth document, or print the labelling
/// worklist under `--sample`.
///
/// Follows `regions::command` step for step — it is the closest sibling and
/// already carries the reasoning for each guard.
pub fn command(args: &[String]) -> Result<(), String> {
    // No `--cache`, for `regions::command`'s reason: the fingerprint cache
    // holds hashes and dimensions, not pixels, and a flag that cannot take
    // effect is refused rather than accepted (O19, O20, O22).
    const KNOWN: &[&str] = &["--sidecar", "--coverage", "--sample"];
    crate::reject_unknown_flags("score", args, KNOWN)?;

    // `--sample` is a different command wearing the same flag, checked and
    // dispatched before a single one of the scoring flags below is read: it
    // does not score anything, so `--sidecar` and `--coverage` cannot take
    // effect under it and are refused rather than silently ignored — the same
    // O19/O20/O22 rule `--cache`'s absence above already follows.
    if let Some(root) = crate::option_value(args, "--sample", "a corpus root")? {
        if args.iter().any(|a| a == "--sidecar") {
            return Err(
                "--sample cannot be used with --sidecar: --sample prints which pages to \
                 label, and does not score anything against a sidecar"
                    .into(),
            );
        }
        if args.iter().any(|a| a == "--coverage") {
            return Err(
                "--sample cannot be used with --coverage: --sample does not score \
                 anything, so there is no outcome for a coverage threshold to apply to"
                    .into(),
            );
        }
        return sample_worklist(std::path::Path::new(root));
    }

    // Every valued flag resolves before a member is opened, so a typo does not
    // cost the seconds a decode pass takes.
    let sidecar = crate::option_value(args, "--sidecar", "a path to a taiyaku.json")?;
    let coverage = match crate::option_value(args, "--coverage", "a fraction from 0 to 1")? {
        None => COVERAGE,
        Some(text) => {
            let value: f64 = text
                .parse()
                .map_err(|_| format!("--coverage wants a number from 0 to 1, not `{text}`"))?;
            if !(0.0..=1.0).contains(&value) {
                return Err(format!(
                    "--coverage must be a fraction from 0 to 1, not {value}"
                ));
            }
            value
        }
    };

    let members: Vec<std::path::PathBuf> = args
        .iter()
        .filter(|a| !a.starts_with("--"))
        .filter(|a| Some(a.as_str()) != sidecar)
        .filter(|a| Some(a.as_str()) != crate::option_value(args, "--coverage", "").ok().flatten())
        .map(std::path::PathBuf::from)
        .collect();
    if members.is_empty() {
        return Err(USAGE.into());
    }

    // Refused rather than ignored: truth is recorded against an edition, and
    // without the document there is nothing to identify against.
    let Some(sidecar) = sidecar else {
        return Err(
            "score needs --sidecar: labels are recorded against an edition, and the \
             sidecar is what says which edition these members are"
                .into(),
        );
    };

    let resolved = resolve_labelled(sidecar, &members)?;
    let detected = crate::fingerprint::decode_all(&resolved.pages, |_, luma| {
        crate::detect::detect_page(luma)
    })?;

    let mut report = Report::default();
    for (drawn, found) in resolved.truth.iter().zip(detected) {
        report.add_page(drawn, &score_page(drawn, &found, coverage));
    }

    print!("{}", report.render());
    Ok(())
}

/// The `.cbz` members of one edition directory, or the directory itself when
/// it holds loose page images.
///
/// The same rule `corpus::members` applies for `score_corpus.rs`'s and
/// `region_experiment.rs`'s own edition reads, restated here rather than
/// called: that function lives in `corpus.rs`, a module gated on
/// `#[cfg(test)]` because the corpus itself is never committed (`corpus.rs`'s
/// own header), while `--sample` is a command a person runs directly against
/// the plain `bin "taiyaku"` build. Unlike `corpus::members`, a directory that
/// will not list comes back a `Result` rather than a panic: `--sample` resolves
/// every entry in one run, and one bad directory must not take the other
/// entries' lines down with it — see `sample_worklist`.
fn sample_members(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut archives: Vec<std::path::PathBuf> = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cbz"))
        {
            archives.push(path);
        }
    }
    Ok(if archives.is_empty() {
        vec![dir.to_path_buf()]
    } else {
        archives
    })
}

/// Resolve one `sample.rs` entry to its page count and the indices to label.
///
/// Reaches the edition the way `region_experiment.rs`'s `measure_edition` does
/// — list the members, then `taiyaku_read::read_edition` for the page count —
/// because `dev_pages`'s subset-of-the-survey's-ten property is stated against
/// exactly that count. A second way of counting an edition's pages here would
/// be a second, possibly disagreeing, answer to the question the survey already
/// answered once.
fn resolve_sample_edition(
    root: &std::path::Path,
    work: &str,
    edition: &str,
) -> Result<(usize, Vec<usize>), String> {
    let dir = root.join(work).join(edition);
    if !dir.is_dir() {
        return Err(format!("{}: not a directory", dir.display()));
    }
    let members = sample_members(&dir)?;
    let pages = taiyaku_read::read_edition(&members)?;
    let available = pages.len();
    Ok((available, crate::sample::dev_pages(available)))
}

/// One group of `sample.rs` entries, printed under its own heading: the dev
/// set or the sealed held-out set.
struct SampleGroup {
    heading: &'static str,
    sealed: bool,
    editions: &'static [(&'static str, &'static str)],
}

const SAMPLE_GROUPS: &[SampleGroup] = &[
    SampleGroup {
        heading: "dev set — labelled now; the only material used to choose between candidates",
        sealed: false,
        editions: crate::sample::DEV_EDITIONS,
    },
    SampleGroup {
        heading: "SEALED held-out set — labelled once, opened once at the very end, and NEVER \
                  used to choose between candidates",
        sealed: true,
        editions: crate::sample::HELD_OUT_EDITIONS,
    },
];

/// Print the labelling worklist `sample.rs` fixes, and fail after printing it
/// if any entry could not be read.
///
/// **Every entry gets a line, resolved or not.** A silently short worklist —
/// an edition dropped from the list rather than named as broken — is the
/// exact failure this whole sub-project exists to fight, so a directory that
/// is missing or an archive that will not open is printed inline, marked
/// `CANNOT BE READ`, rather than skipped. The `Err` that follows a failure
/// is returned only after every line has printed, so a human reading stdout
/// sees the whole worklist before the exit code says the run did not come
/// back clean.
fn sample_worklist(root: &std::path::Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!(
            "{}: not a directory; --sample wants a corpus root — the directory holding \
             one subdirectory per work, same as `TAIYAKU_CORPUS`",
            root.display()
        ));
    }

    let mut grand_total_pages = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for group in SAMPLE_GROUPS {
        println!("{} ({} editions):", group.heading, group.editions.len());
        let mut group_pages = 0usize;
        for &(work, edition) in group.editions {
            match resolve_sample_edition(root, work, edition) {
                Ok((available, pages)) => {
                    group_pages += pages.len();
                    let page_list: Vec<String> = pages.iter().map(usize::to_string).collect();
                    println!(
                        "  {work}/{edition}: {available} pages -> label {} page(s): {}",
                        pages.len(),
                        page_list.join(", ")
                    );
                }
                Err(reason) => {
                    failed.push(format!("{work}/{edition}"));
                    println!("  {work}/{edition}: CANNOT BE READ — {reason}");
                }
            }
        }
        println!(
            "  {} subtotal: {group_pages} page(s)\n",
            if group.sealed { "sealed" } else { "dev" }
        );
        grand_total_pages += group_pages;
    }

    let total_editions = SAMPLE_GROUPS
        .iter()
        .map(|group| group.editions.len())
        .sum::<usize>();
    println!(
        "TOTAL: {grand_total_pages} page(s) to label across {} of {total_editions} editions \
         resolved",
        total_editions - failed.len()
    );

    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} edition(s) could not be read — see CANNOT BE READ above: {}",
            failed.len(),
            failed.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests;
