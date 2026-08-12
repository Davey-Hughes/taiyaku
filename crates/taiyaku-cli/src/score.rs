//! What a detector found, against what a person drew.
//!
//! **Five outcomes per truth box, not an IoU pass or fail** — one-to-one,
//! merged, split, fragment, missed — and the reason is recorded rather than
//! assumed. The detector's two known failure modes both
//! score as a flat miss under IoU@0.5 and it cannot tell them apart:
//! `docs/notes.md` records *both* on one edition at one gap — panels
//! over-merged, and seven "blocks" that are vertical slices of one word. A
//! single overlap number reports "recall 0.0" for each and says nothing about
//! which, which is why finding out cost ten hand-cropped blocks.
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
    /// whole-panel-around-one-balloon case, `docs/notes.md:7046-7057`.
    Merged,
    /// Two or more predictions cover it.
    Split,
    /// Its prediction sits inside it and covers little of it — "a **letter
    /// fragment**, a vertical slice of one word", `docs/notes.md:7053`.
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
/// **A starting value, not a measurement.** It cannot be one yet: settling it
/// needs labels and there are none. It gets settled the way
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
    // than an obvious error. At zero this now reads "any overlap at all".
    inter > 0.0 && inter / smaller >= coverage
}

/// The shape of a single association: which way round the coverage is short.
///
/// `reach` is how many *scored* truth boxes the prediction covers. Deciding
/// on that count alone is the defect this instrument exists to catch: a
/// prediction can be a whole panel and still touch exactly one labelled
/// balloon, and `docs/notes.md:7046-7057` lists three such panels among ten
/// hand-cropped blocks. Counting says 1:1 and credits the detector for
/// returning panels. Geometry says the prediction is 99% something else.
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
    /// returning whole panels score as working, which is the mistake
    /// `blocks_per_page` made.
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

/// Score `detect_page` against a truth document, or print the labelling
/// worklist under `--sample`.
///
/// Follows `regions::command` step for step — it is the closest sibling and
/// already carries the reasoning for each guard.
/// One edition's labelled pages, resolved and ready to decode.
///
/// Extracted from `command` so `truth_population.rs` measures **the same pages
/// this command scores**. A measurement taken on a different page set than the
/// score is a defect nothing would catch: both would print plausible numbers.
///
/// `truth` and `pages` are parallel and ascending. `pages` is undecoded bytes
/// rather than `GrayImage`, because `decode_all` bounds memory by decoding
/// large pages one at a time and the corpus holds 430 MB TIFFs.
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
    // `regions::command` carries at `regions.rs:201-218`; it is restated
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
    // is exactly where they disagree: `en-fan/*.cbz` expands to `…c004,
    // c005.5, c005` (`5` sorts before `c`) while the sidecar lists `…c004,
    // c005, c005.5`. That refused a perfectly good truth document on this
    // corpus today, telling a person to redo hours of labelling.
    if labelled.is_stale_for(&ordered_digests) {
        return Err(format!(
            "{} was drawn against different files; re-label or restore the archive",
            truth_path.display()
        ));
    }

    // **Only the labelled pages are decoded.** `decode_all` decodes
    // everything it is handed, and five pages of 195 are labelled — handing it
    // the whole edition would pay for 190 full-resolution decodes to score
    // five. Moved rather than cloned, so `Page` needs no `Clone`.
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
/// `#[cfg(test)]` because the corpus itself is 24 GB and never committed
/// (`corpus.rs`'s own header), while `--sample` is a command a person runs
/// directly against the plain `bin "taiyaku"` build. Unlike `corpus::members`,
/// a directory that will not list comes back a `Result` rather than a panic:
/// `--sample` resolves fifteen entries in one run, and one bad directory must
/// not take the other fourteen lines down with it — see `sample_worklist`.
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
/// Reaches the edition the way `score_corpus.rs`'s `score_corpus_page` and
/// `region_experiment.rs`'s `measure_edition` both do — list the members,
/// then `taiyaku_read::read_edition` for the page count — because
/// `dev_pages`'s subset-of-the-survey's-ten property is stated against
/// exactly that count. A second way of counting an edition's pages here
/// would be a second, possibly disagreeing, answer to the question the
/// survey already answered once.
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
/// **Every one of the fifteen entries gets a line, resolved or not.** A
/// silently short worklist — an edition dropped from the list rather than
/// named as broken — is the exact failure this whole sub-project exists to
/// fight, so a directory that is missing or an archive that will not open is
/// printed inline, marked `CANNOT BE READ`, rather than skipped. The `Err`
/// that follows a failure is returned only after every line has printed, so
/// a human reading stdout sees the whole worklist before the exit code says
/// the run did not come back clean.
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
mod tests {
    use super::*;
    use taiyaku_core::{Axis, Kind, Region, TruthRegion};

    fn truth(x: f64, y: f64, w: f64, h: f64, kind: Kind) -> TruthRegion {
        TruthRegion {
            x,
            y,
            w,
            h,
            axis: Some(Axis::Vertical),
            kind,
            ignore: false,
            text: None,
        }
    }

    fn ignored(x: f64, y: f64, w: f64, h: f64) -> TruthRegion {
        TruthRegion {
            ignore: true,
            ..truth(x, y, w, h, Kind::Sfx)
        }
    }

    fn predicted(x: f64, y: f64, w: f64, h: f64) -> Region {
        Region {
            x,
            y,
            w,
            h,
            axis: Axis::Vertical,
        }
    }

    #[test]
    fn one_prediction_over_one_truth_box_is_one_to_one() {
        let scored = score_page(
            &[truth(0.1, 0.1, 0.2, 0.2, Kind::Dialogue)],
            &[predicted(0.1, 0.1, 0.2, 0.2)],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::OneToOne]);
        assert_eq!(scored.spurious, 0);
    }

    #[test]
    fn one_prediction_swallowing_three_truth_boxes_is_a_merge_not_three_misses() {
        // The failure `notes.md` found by cropping: the detector returns whole
        // panels. Under IoU@0.5 this is three misses and says nothing about
        // why. It must come back as three merges.
        let scored = score_page(
            &[
                truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue),
                truth(0.3, 0.1, 0.1, 0.1, Kind::Dialogue),
                truth(0.5, 0.1, 0.1, 0.1, Kind::Dialogue),
            ],
            &[predicted(0.05, 0.05, 0.6, 0.2)],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Merged; 3]);
        assert_eq!(
            scored.spurious, 0,
            "the prediction is accounted for, not spurious"
        );
    }

    #[test]
    fn several_predictions_over_one_truth_box_is_a_split() {
        // The opposite failure on the same page: seven "blocks" that are
        // vertical slices of one word.
        let scored = score_page(
            &[truth(0.1, 0.1, 0.3, 0.1, Kind::Dialogue)],
            &[
                predicted(0.10, 0.1, 0.09, 0.1),
                predicted(0.21, 0.1, 0.09, 0.1),
                predicted(0.31, 0.1, 0.09, 0.1),
            ],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Split]);
    }

    #[test]
    fn nothing_overlapping_a_truth_box_is_a_miss() {
        let scored = score_page(
            &[truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue)],
            &[predicted(0.7, 0.7, 0.1, 0.1)],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Missed]);
        assert_eq!(scored.spurious, 1);
    }

    #[test]
    fn a_prediction_over_nothing_is_spurious() {
        let scored = score_page(&[], &[predicted(0.7, 0.7, 0.1, 0.1)], COVERAGE);
        assert!(scored.outcomes.is_empty());
        assert_eq!(scored.spurious, 1);
    }

    #[test]
    fn a_prediction_landing_on_an_ignored_box_is_neither_hit_nor_false_positive() {
        let scored = score_page(
            &[ignored(0.1, 0.1, 0.2, 0.2)],
            &[predicted(0.1, 0.1, 0.2, 0.2)],
            COVERAGE,
        );
        assert!(scored.outcomes.is_empty(), "an ignored box is not scored");
        assert_eq!(scored.spurious, 0, "and its prediction is not punished");
        assert_eq!(scored.ignored, 1, "but it is counted out loud");
    }

    #[test]
    fn a_grazing_overlap_below_the_threshold_does_not_match() {
        let scored = score_page(
            &[truth(0.1, 0.1, 0.2, 0.2, Kind::Dialogue)],
            &[predicted(0.28, 0.28, 0.2, 0.2)],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Missed]);
        assert_eq!(scored.spurious, 1);
    }

    #[test]
    fn an_empty_page_scores_nothing_and_does_not_panic() {
        let scored = score_page(&[], &[], COVERAGE);
        assert!(scored.outcomes.is_empty());
        assert_eq!(scored.spurious, 0);
        assert_eq!(scored.ignored, 0);
    }

    #[test]
    fn a_report_tallies_outcomes_across_pages() {
        let mut report = Report::default();
        let page = [truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue)];
        report.add_page(
            &page,
            &score_page(&page, &[predicted(0.1, 0.1, 0.1, 0.1)], COVERAGE),
        );
        report.add_page(&page, &score_page(&page, &[], COVERAGE));
        assert_eq!(report.one_to_one, 1);
        assert_eq!(report.missed, 1);
        assert_eq!(report.pages, 2);
    }

    #[test]
    fn recall_is_reported_per_kind_so_dialogue_and_sfx_are_separable() {
        // The whole reason kinds were not collapsed into one class: a later
        // decision to stop caring about sound effects must cost no relabelling.
        let page = [
            truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue),
            truth(0.5, 0.5, 0.1, 0.1, Kind::Sfx),
        ];
        let mut report = Report::default();
        report.add_page(
            &page,
            &score_page(&page, &[predicted(0.1, 0.1, 0.1, 0.1)], COVERAGE),
        );
        assert_eq!(
            report.by_kind[&Kind::Dialogue],
            KindTally { found: 1, total: 1 }
        );
        assert_eq!(report.by_kind[&Kind::Sfx], KindTally { found: 0, total: 1 });
    }

    #[test]
    fn only_one_to_one_counts_as_found() {
        // A merge and a split are failures, not partial credit. Counting them as
        // found would let a detector returning whole panels score as working,
        // which is the exact mistake blocks_per_page made.
        let page = [
            truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue),
            truth(0.3, 0.1, 0.1, 0.1, Kind::Dialogue),
        ];
        let mut report = Report::default();
        report.add_page(
            &page,
            &score_page(&page, &[predicted(0.05, 0.05, 0.4, 0.2)], COVERAGE),
        );
        assert_eq!(report.merged, 2);
        assert_eq!(
            report.by_kind[&Kind::Dialogue],
            KindTally { found: 0, total: 2 }
        );
    }

    #[test]
    fn the_ignored_count_is_carried_so_the_hatch_is_never_silent() {
        let page = [ignored(0.1, 0.1, 0.1, 0.1)];
        let mut report = Report::default();
        report.add_page(&page, &score_page(&page, &[], COVERAGE));
        assert_eq!(report.ignored, 1);
        assert!(report.render().contains("ignored"), "and it is printed");
    }

    #[test]
    fn spurious_is_a_rate_because_a_candidate_may_predict_more_in_total() {
        let mut report = Report::default();
        let page = [truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue)];
        report.add_page(
            &page,
            &score_page(&page, &[predicted(0.8, 0.8, 0.1, 0.1)], COVERAGE),
        );
        report.add_page(&page, &score_page(&page, &[], COVERAGE));
        assert!((report.spurious_per_page() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_report_over_no_pages_renders_without_dividing_by_zero() {
        let report = Report::default();
        assert_eq!(report.spurious_per_page(), 0.0);
        assert!(!report.render().is_empty());
    }

    #[test]
    fn a_whole_panel_prediction_over_one_balloon_is_a_merge_not_one_to_one() {
        // The regression this instrument exists for. Of the ten blocks
        // hand-cropped in `docs/notes.md:7046-7057`, five were confirmed by eye
        // to be whole panels and **three of those contain exactly one balloon**
        // — `gotoubun p139 b1` ("a figure on a bench, tone, one balloon"),
        // `gotoubun p158 b2` ("one balloon plus a black silhouette") and
        // `oshinoko p78 b0` ("one balloon plus art"). Decided on a count of how
        // many labelled boxes a prediction reaches, every one of those three
        // scores as a clean 1:1 hit, `add_page` credits it as `found`, and the
        // detector that returns panels ranks *better*. The prediction here
        // covers 0.25% of its own area with balloon; it is a merge.
        let scored = score_page(
            &[truth(0.4, 0.4, 0.05, 0.05, Kind::Dialogue)],
            &[predicted(0.0, 0.0, 1.0, 1.0)],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Merged]);
        assert_eq!(
            scored.spurious, 0,
            "the panel is accounted for by the box it swallowed, not spurious"
        );
    }

    #[test]
    fn a_prediction_covering_a_sliver_of_a_truth_box_is_a_fragment() {
        // `gotoubun p102 b3`, 16×82: "a **letter fragment**, a vertical slice
        // of one word" (`docs/notes.md:7053`). The prediction lies wholly
        // inside the truth box and covers 0.25% of it. Association still
        // holds — it is plainly *about* this box — but it found a quarter of a
        // percent of it, which is a failure and not a hit.
        let scored = score_page(
            &[truth(0.1, 0.1, 0.4, 0.4, Kind::Dialogue)],
            &[predicted(0.2, 0.2, 0.02, 0.02)],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Fragment]);
        assert_eq!(scored.spurious, 0, "it is not a false positive either");
    }

    #[test]
    fn a_prediction_spanning_a_scored_and_an_ignored_box_is_a_merge() {
        // `reach` counts scored boxes only, so a prediction swallowing one
        // balloon together with a sound effect flagged `ignore` reaches
        // exactly one scored box and reads 1:1 on a count. It is the same
        // over-merge, and the ignore flag forgives the prediction — it does
        // not turn it into a hit.
        let page = [
            truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue),
            ignored(0.3, 0.1, 0.1, 0.1),
        ];
        let scored = score_page(&page, &[predicted(0.05, 0.05, 0.4, 0.2)], COVERAGE);
        assert_eq!(scored.outcomes, vec![Outcome::Merged]);
        assert_eq!(scored.ignored, 1);
        assert_eq!(scored.spurious, 0);
    }

    #[test]
    fn a_coverage_of_zero_does_not_match_disjoint_boxes() {
        // `COVERAGE` is provisional and meant to be swept. A ratio test alone
        // is vacuously true at zero, so the sweep's first step would report a
        // flawless page — every prediction matched to every truth box,
        // including boxes on opposite corners.
        let scored = score_page(
            &[truth(0.1, 0.1, 0.1, 0.1, Kind::Dialogue)],
            &[predicted(0.7, 0.7, 0.1, 0.1)],
            0.0,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Missed]);
        assert_eq!(scored.spurious, 1);
    }

    #[test]
    fn a_fragment_is_not_counted_as_found() {
        // Same rule as a merge and a split: a quarter of a percent of a box is
        // not partial credit, and recall must not move for it.
        let page = [truth(0.1, 0.1, 0.4, 0.4, Kind::Dialogue)];
        let mut report = Report::default();
        report.add_page(
            &page,
            &score_page(&page, &[predicted(0.2, 0.2, 0.02, 0.02)], COVERAGE),
        );
        assert_eq!(report.fragment, 1);
        assert_eq!(report.one_to_one, 0);
        assert_eq!(
            report.by_kind[&Kind::Dialogue],
            KindTally { found: 0, total: 1 }
        );
        let rendered = report.render();
        assert!(
            rendered.contains("split=0 fragment=1 missed=0"),
            "and it is printed beside merged/split/missed: {rendered}"
        );
        assert!(
            rendered.contains("/page) ignored=0"),
            "the wrapped format string keeps its space — a `\\` continuation \
             eats the newline *and* the indent that follows it: {rendered}"
        );
    }

    #[test]
    fn a_zero_area_ignored_box_still_absorbs_its_prediction() {
        // A click without a drag on the ignore hatch. Refusing it would make
        // the one labelling slip that *forgives* a prediction punish it
        // instead, which is backwards.
        let page = [ignored(0.15, 0.15, 0.0, 0.0)];
        let scored = score_page(&page, &[predicted(0.1, 0.1, 0.1, 0.1)], COVERAGE);
        assert_eq!(scored.spurious, 0, "the mis-click still forgives");
        assert_eq!(scored.ignored, 1, "and is still counted out loud");

        let elsewhere = score_page(&page, &[predicted(0.7, 0.7, 0.1, 0.1)], COVERAGE);
        assert_eq!(
            elsewhere.spurious, 1,
            "but it does not forgive the whole page"
        );
    }

    #[test]
    fn a_non_finite_coordinate_is_not_a_hit() {
        // `f64::max` and `f64::min` discard NaN, so the intersection of a NaN
        // box collapses onto the other box's own edges and the ratio comes out
        // 1.0 — corrupt input scoring as a perfect match.
        let scored = score_page(
            &[truth(f64::NAN, 0.1, 0.2, 0.2, Kind::Dialogue)],
            &[predicted(0.1, 0.1, 0.2, 0.2)],
            COVERAGE,
        );
        assert_eq!(scored.outcomes, vec![Outcome::Missed]);
        assert_eq!(scored.spurious, 1);
    }

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_string()).collect()
    }

    #[test]
    fn score_with_no_arguments_prints_its_usage() {
        let err = command(&argv(&[])).unwrap_err();
        assert!(err.contains("usage"), "{err}");
    }

    #[test]
    fn a_misspelled_flag_is_refused_rather_than_ignored() {
        // O21's rule: a flag typed with a typo must not be accepted in silence.
        let err = command(&argv(&["--sidecarr", "x", "page.jpg"])).unwrap_err();
        assert!(
            err.contains("--sidecarr"),
            "the message names the flag: {err}"
        );
    }

    #[test]
    fn score_needs_a_sidecar_because_truth_is_recorded_against_an_edition() {
        let err = command(&argv(&["page.jpg"])).unwrap_err();
        assert!(err.contains("--sidecar"), "{err}");
    }

    #[test]
    fn a_coverage_outside_zero_to_one_is_refused() {
        let err = command(&argv(&[
            "--sidecar",
            "t.json",
            "--coverage",
            "1.5",
            "p.jpg",
        ]))
        .unwrap_err();
        assert!(err.contains("coverage"), "{err}");
    }

    #[test]
    fn sample_with_no_corpus_root_is_refused() {
        let err = command(&argv(&["--sample"])).unwrap_err();
        assert!(err.contains("--sample"), "{err}");
    }

    #[test]
    fn sample_combined_with_sidecar_is_refused_and_names_the_conflict() {
        // O19, O20, O22's rule: a flag that cannot take effect is refused, not
        // silently ignored. --sample does not score anything, so --sidecar
        // has nothing to do under it.
        let err = command(&argv(&["--sample", "corpus", "--sidecar", "x"])).unwrap_err();
        assert!(err.contains("--sample"), "{err}");
        assert!(err.contains("--sidecar"), "{err}");
    }

    #[test]
    fn sample_combined_with_coverage_is_refused_and_names_the_conflict() {
        let err = command(&argv(&["--sample", "corpus", "--coverage", "0.5"])).unwrap_err();
        assert!(err.contains("--sample"), "{err}");
        assert!(err.contains("--coverage"), "{err}");
    }

    /// O21: a misspelling must not be accepted in silence, --sample included.
    #[test]
    fn sample_with_a_misspelled_flag_is_refused() {
        let err = command(&argv(&["--sample", "corpus", "--covrage", "0.5"])).unwrap_err();
        assert!(err.contains("--covrage"), "{err}");
    }

    #[test]
    fn sample_needs_the_corpus_root_itself_to_be_a_directory() {
        let err = sample_worklist(std::path::Path::new("/no/such/taiyaku-sample-corpus-root"))
            .unwrap_err();
        assert!(err.contains("not a directory"), "{err}");
    }

    /// The failure this whole sub-project exists to fight: an edition that
    /// cannot be read must be named on its own, not folded away into a bare
    /// count or dropped from the worklist. Every `sample.rs` entry is missing
    /// under this empty root, so every one of the fifteen must be named.
    #[test]
    fn a_missing_edition_directory_is_reported_by_name_not_skipped_silently() {
        let root = std::env::temp_dir().join("taiyaku-test-sample-missing-root");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create an empty corpus root");

        let err = sample_worklist(&root).expect_err("no sample.rs entry exists under this root");
        let _ = std::fs::remove_dir_all(&root);

        assert!(
            err.contains("15 edition(s) could not be read"),
            "all nine dev and six sealed entries are missing: {err}"
        );
        assert!(
            err.contains("gotoubun-v01/raw-mono"),
            "a specific missing dev edition must be named: {err}"
        );
        assert!(
            err.contains("incoming/berserk-cleaned/Berserk.T01"),
            "a specific missing sealed edition must be named too: {err}"
        );
    }

    /// A scratch directory nobody else is using, emptied first in case a
    /// previous run left one behind.
    fn scratch(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("taiyaku-test-score-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create a scratch directory");
        root
    }

    /// A one-page cbz whose page is a real, decodable image.
    ///
    /// `command` runs the whole decode path — `read_members_in_order` then
    /// `fingerprint::decode_all` — so the entry has to be bytes an image
    /// decoder accepts, not a stand-in. `regions.rs`'s `cbz_with_page` is the
    /// same fixture for the same reason; the entry name differs per member so
    /// the two archives hash differently, which is the whole basis of every
    /// order comparison below.
    fn cbz_with_page(path: &std::path::Path, page: &str) {
        use std::io::Write;
        let image = image::GrayImage::from_pixel(4, 4, image::Luma([255]));
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("a 4x4 gray image always encodes");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).expect("create cbz"));
        zip.start_file(page, zip::write::SimpleFileOptions::default())
            .expect("start entry");
        zip.write_all(&bytes).expect("write page bytes");
        zip.finish().expect("finish cbz");
    }

    /// A sidecar naming one edition made of `members`, **in the order given**.
    fn write_sidecar(root: &std::path::Path, members: &[(&str, &str)]) -> std::path::PathBuf {
        let entries: Vec<String> = members
            .iter()
            .map(|(path, sha256)| {
                format!(r#"{{ "path": "{path}", "sha256": "{sha256}", "pages": 1 }}"#)
            })
            .collect();
        let path = root.join("taiyaku.json");
        std::fs::write(
            &path,
            format!(
                r#"{{
  "taiyaku": 1,
  "work": {{ "title": "t", "reading": "rtl" }},
  "editions": {{
    "raw-mono": {{ "label": "raw-mono", "members": [{}] }}
  }},
  "hub": "raw-mono",
  "alignments": []
}}"#,
                entries.join(", ")
            ),
        )
        .expect("write sidecar");
        path
    }

    /// The truth document beside `sidecar`, with `members` in the order the
    /// browser writes them — the sidecar's own — and `pages` verbatim.
    fn write_truth(sidecar: &std::path::Path, members: &[&str], pages: &str) {
        let entries: Vec<String> = members
            .iter()
            .map(|sha256| format!(r#"{{ "sha256": "{sha256}" }}"#))
            .collect();
        std::fs::write(
            taiyaku_core::TruthDoc::path_beside(sidecar),
            format!(
                r#"{{ "taiyaku_truth": 1, "editions": {{ "raw-mono": {{
                   "members": [{}], "pages": {pages} }} }} }}"#,
                entries.join(", ")
            ),
        )
        .expect("write truth document");
    }

    const ONE_BOX_ON_PAGE_0: &str =
        r#"{"0":[{"x":0.1,"y":0.1,"w":0.2,"h":0.2,"axis":"v","kind":"dialogue"}]}"#;

    /// Two members whose sidecar order is not their sorted order, typed in
    /// sorted order — and the good truth document beside them must not be
    /// called stale.
    ///
    /// `edition_of_digests` matches by **sorted set**, deliberately, so the
    /// order the members were typed in does not decide *which* edition they
    /// are — `score.rs`'s own comment says exactly that. `is_stale_for`
    /// compares **element-wise**, deliberately, because content-in-order is
    /// what says these labels describe these pages. Handing the argv-ordered
    /// list to that element-wise check makes the two rules contradict each
    /// other, and it refused a perfectly good document telling a person to
    /// redo hours of labelling.
    ///
    /// **Live on this corpus.** `corpus/gotoubun-v01`'s sidecar lists en-fan
    /// as `…c004, c005, c005.5`, while `en-fan/*.cbz` expands to `…c004,
    /// c005.5, c005` — `5` sorts before `c`. The fixture reproduces that
    /// shape: sidecar order `b, a`, command line `a, b`. A fixture whose two
    /// orders agreed could not tell the two lists apart at all.
    #[test]
    fn a_glob_ordered_command_line_does_not_make_a_good_truth_document_stale() {
        let root = scratch("glob-order");
        let a = root.join("a.cbz");
        let b = root.join("b.cbz");
        cbz_with_page(&a, "a1.png");
        cbz_with_page(&b, "b1.png");
        let digest_a = taiyaku_read::member_digest(&a).expect("digest a");
        let digest_b = taiyaku_read::member_digest(&b).expect("digest b");
        assert_ne!(digest_a, digest_b, "or no order can be told from another");

        // Sidecar order b, a — the reverse of sorted order.
        let sidecar = write_sidecar(&root, &[("b.cbz", &digest_b), ("a.cbz", &digest_a)]);
        // The browser writes the truth document's members in sidecar order.
        write_truth(&sidecar, &[&digest_b, &digest_a], ONE_BOX_ON_PAGE_0);

        // Typed a, b — sorted order, which is what a shell glob hands over.
        let result = command(&argv(&[
            "--sidecar",
            sidecar.to_str().expect("sidecar path is utf8"),
            a.to_str().expect("a path is utf8"),
            b.to_str().expect("b path is utf8"),
        ]));
        let _ = std::fs::remove_dir_all(&root);
        result.expect("a truth document drawn against these very archives is not stale");
    }

    /// And the guard is not weakened to get there: a member whose bytes are
    /// not the ones the labels were drawn against is still refused.
    ///
    /// Without this, dropping the staleness check entirely would pass the test
    /// above. Stale ground truth "draws a box on the wrong art on the *right*
    /// page" (`truth.rs`), which is the sharpest form of the problem here.
    #[test]
    fn a_replaced_archive_is_still_refused_after_the_order_fix() {
        let root = scratch("replaced");
        let a = root.join("a.cbz");
        let b = root.join("b.cbz");
        cbz_with_page(&a, "a1.png");
        cbz_with_page(&b, "b1.png");
        let digest_a = taiyaku_read::member_digest(&a).expect("digest a");
        let digest_b = taiyaku_read::member_digest(&b).expect("digest b");

        let sidecar = write_sidecar(&root, &[("b.cbz", &digest_b), ("a.cbz", &digest_a)]);
        // Drawn against a different second archive: same count, same order,
        // one digest that is not on disk any more.
        write_truth(
            &sidecar,
            &[
                &digest_b,
                "0000000000000000000000000000000000000000000000000000000000000000",
            ],
            ONE_BOX_ON_PAGE_0,
        );

        let err = command(&argv(&[
            "--sidecar",
            sidecar.to_str().expect("sidecar path is utf8"),
            a.to_str().expect("a path is utf8"),
            b.to_str().expect("b path is utf8"),
        ]))
        .unwrap_err();
        let _ = std::fs::remove_dir_all(&root);
        assert!(err.contains("drawn against different files"), "{err}");
    }

    /// A labelled page this edition cannot reach is named and refused, not
    /// quietly left out of the count.
    ///
    /// The filter that keeps only labelled pages cannot see a truth key past
    /// the end — it simply never appears — so the report printed a smaller
    /// `pages=` with nothing saying which page went unmeasured.
    /// `score_corpus.rs` already fails by name on exactly this, and the
    /// shipped command must not be the softer of the two.
    #[test]
    fn a_labelled_page_past_the_end_of_the_edition_is_named_rather_than_dropped() {
        let root = scratch("past-the-end");
        let a = root.join("a.cbz");
        cbz_with_page(&a, "a1.png");
        let digest_a = taiyaku_read::member_digest(&a).expect("digest a");

        let sidecar = write_sidecar(&root, &[("a.cbz", &digest_a)]);
        // One page in the edition; labels on page 0 and on page 7.
        write_truth(
            &sidecar,
            &[&digest_a],
            r#"{"0":[{"x":0.1,"y":0.1,"w":0.2,"h":0.2,"axis":"v","kind":"dialogue"}],
               "7":[{"x":0.1,"y":0.1,"w":0.2,"h":0.2,"axis":"v","kind":"dialogue"}]}"#,
        );

        let err = command(&argv(&[
            "--sidecar",
            sidecar.to_str().expect("sidecar path is utf8"),
            a.to_str().expect("a path is utf8"),
        ]))
        .unwrap_err();
        let _ = std::fs::remove_dir_all(&root);
        assert!(err.contains('7'), "the message names the page: {err}");
        assert!(
            err.contains("1 page(s)"),
            "and how long the edition actually is: {err}"
        );
    }

    /// The success path: a real edition directory of loose page images
    /// resolves to its page count and the exact indices `dev_pages` picks —
    /// the same function `region_experiment.rs`'s survey already samples
    /// with, so the worklist stays a subset of it.
    #[test]
    fn a_resolved_edition_reports_its_page_count_and_the_pages_to_label() {
        let root = std::env::temp_dir().join("taiyaku-test-sample-resolved-root");
        let dir = root.join("gotoubun-v01").join("raw-mono");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&dir).expect("create an edition directory");

        let image = image::GrayImage::from_pixel(4, 4, image::Luma([255]));
        for i in 0..6 {
            image
                .save(dir.join(format!("p{i:03}.png")))
                .expect("a 4x4 gray image always encodes");
        }

        let result = resolve_sample_edition(&root, "gotoubun-v01", "raw-mono");
        let _ = std::fs::remove_dir_all(&root);
        let (available, pages) = result.expect("six loose page images resolve");

        assert_eq!(available, 6);
        assert_eq!(pages, crate::sample::dev_pages(6));
    }
}
