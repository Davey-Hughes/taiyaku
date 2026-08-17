use super::*;

/// A bare flag is on when it is there and off when it is not, and that is
/// the whole of it — but it is the whole of it at every call site, so the
/// comparison being right once is the comparison being right everywhere.
#[test]
fn a_bare_flag_is_on_only_when_it_was_given() {
    let args = |all: &[&str]| all.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();

    assert!(flag_present(&args(&["a.cbz", "--stats"]), "--stats"));
    assert!(!flag_present(&args(&["a.cbz", "--stats"]), "--ltr"));
    assert!(!flag_present(&args(&["a.cbz"]), "--stats"));
    assert!(!flag_present(&[], "--stats"));
    assert!(
        !flag_present(&args(&["--stats-please"]), "--stats"),
        "a longer flag that starts the same is a different flag"
    );
}

/// Right to left unless asked otherwise. Everything the corpus holds reads
/// that way, so the flag is the exception and the default is not a guess.
#[test]
fn the_reading_direction_is_right_to_left_unless_the_flag_says_otherwise() {
    let args = |all: &[&str]| all.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();

    assert_eq!(
        reading_direction(&args(&["a.cbz", "b.cbz"])),
        Direction::RightToLeft
    );
    assert_eq!(
        reading_direction(&args(&["a.cbz", "--ltr", "b.cbz"])),
        Direction::LeftToRight
    );
    assert_eq!(reading_direction(&[]), Direction::RightToLeft);
}

/// Both directions of every distinct pair, and no self-pairs.
///
/// A star is directional: `i -> j` composes `i -> hub` with `hub -> j`,
/// and `j -> i` uses the other two legs, so they are different
/// measurements. The count is also what the header prints as the price of
/// all-pairs, so it has to be the count of what the table actually holds.
#[test]
fn the_star_measures_both_directions_of_every_pair_and_no_self_pair() {
    assert_eq!(
        ordered_pairs(3),
        vec![(0, 1), (0, 2), (1, 0), (1, 2), (2, 0), (2, 1)]
    );
    assert_eq!(ordered_pairs(3).len(), 3 * (3 - 1), "n(n-1), as printed");
    assert_eq!(ordered_pairs(1), vec![], "one spoke has nothing to pair");
    assert_eq!(ordered_pairs(0), vec![]);
}

fn run_between(a: (u32, u32), b: (u32, u32)) -> Run {
    Run::paired(
        taiyaku_core::PageRange::new(a.0, a.1),
        taiyaku_core::PageRange::new(b.0, b.1),
    )
}

/// The four ways a direct solve and a composed one can differ, all four in
/// one comparison so that none of them can borrow another's count.
///
/// Pages 0 and 1 pair the same way in both. Page 2 the direct solve pairs
/// and composition loses. Page 3 composition pairs and the direct solve
/// misses. Page 4 both pair, to different counterparts. So: four pages
/// paired directly, one loss, one inverse, one disagreement — four
/// different numbers, which is what stops any of them being read off
/// another.
#[test]
fn a_divergence_counts_loss_inverse_and_disagreement_apart() {
    let direct = vec![
        run_between((0, 2), (0, 2)),
        Run::gap(taiyaku_core::PageRange::new(3, 3)),
        run_between((4, 4), (4, 4)),
    ];
    let composed = vec![
        run_between((0, 1), (0, 1)),
        Run::gap(taiyaku_core::PageRange::new(2, 2)),
        run_between((3, 4), (9, 10)),
    ];

    let found = Divergence::between(&direct, &composed, 5);

    assert_eq!(
        found,
        Divergence {
            direct_paired: 4,
            loss: 1,
            inverse: 1,
            disagree: 1,
        }
    );

    // Totals across the star are the four sums, each on its own.
    assert_eq!(
        found.plus(Divergence {
            direct_paired: 10,
            loss: 2,
            inverse: 3,
            disagree: 4,
        }),
        Divergence {
            direct_paired: 14,
            loss: 3,
            inverse: 4,
            disagree: 5,
        }
    );
    assert_eq!(found.plus(Divergence::default()), found);
}

/// The verdict column is where composition and `trust`'s worse-leg rule
/// are put side by side, and the disagreement is the measurement the
/// command exists to take — so agreement prints one word and disagreement
/// prints both, in that order.
#[test]
fn the_verdict_column_names_both_rules_only_when_they_disagree() {
    let report = |composed, worse_leg| PairReport {
        pages: 200,
        divergence: Divergence::default(),
        composed,
        worse_leg,
    };

    let agreed = report(Verdict::Sound, Verdict::Sound);
    assert!(!agreed.verdict_split());
    assert_eq!(agreed.verdict_column(), stats::verdict_word(Verdict::Sound));

    let split = report(Verdict::Sound, Verdict::Fragmented);
    assert!(split.verdict_split());
    assert_eq!(
        split.verdict_column(),
        format!(
            "{} != {}",
            stats::verdict_word(Verdict::Sound),
            stats::verdict_word(Verdict::Fragmented)
        ),
        "composed first, worse leg second"
    );
}

/// A composed route is judged by its weaker leg and a stored one answers
/// for itself, and asking the wrong one of the two is silent in both
/// directions: `quality_of` on a composed pair finds no edge and says the
/// document does not say, about a pairing the document says plenty about.
///
/// `en-fan` and `en-mono` are the two spokes of this fixture's star, so
/// nothing joins them directly and the difference between the two rules is
/// the difference between an answer and no answer at all.
#[test]
fn a_composed_route_is_judged_by_its_weaker_leg_and_a_stored_one_by_itself() {
    let doc = three_editions_one_hand_corrected();

    let composed = route_verdict(&doc, &Via::Hub("raw".into()), "en-fan", "en-mono");
    assert_eq!(
        composed,
        doc.trust("en-fan", "en-mono"),
        "a composed pair is only as good as its worse leg"
    );
    assert!(
        composed.is_some(),
        "the fixture must actually compose, or nothing above can fail"
    );
    assert_eq!(
        doc.quality_of("en-fan", "en-mono"),
        None,
        "and asking for a stored edge between two spokes finds none, \
             which is what makes the arm above load-bearing"
    );

    let stored = route_verdict(&doc, &Via::Direct, "raw", "en-fan");
    assert_eq!(
        stored,
        doc.quality_of("raw", "en-fan").map(|q| q.verdict),
        "a stored edge answers for itself"
    );
    assert!(stored.is_some(), "and this fixture stores one");
    assert_eq!(
        route_verdict(&doc, &Via::Inverted, "en-fan", "raw"),
        doc.quality_of("en-fan", "raw").map(|q| q.verdict),
        "so does the same edge read backwards"
    );
}

/// How the route was reached, which is the difference between a number a
/// solver checked and one nothing ever did.
#[test]
fn the_route_note_says_only_what_a_direct_edge_would_not() {
    assert_eq!(route_note(&Via::Direct, "raw", "fan"), "");
    assert_eq!(route_note(&Via::Identity, "raw", "fan"), "");
    assert_eq!(
        route_note(&Via::Inverted, "raw", "fan"),
        ", from the stored fan -> raw edge",
        "inverted names the edge as it is stored, not as it was asked for"
    );
    assert_eq!(
        route_note(&Via::Hub("raw".into()), "fan", "colour"),
        ", composed through raw"
    );
}

/// Both ends of a slot-indexed pairing move when it is put back onto
/// pages, and each moves by a different amount.
///
/// A slots: page 0, then page 1 as two halves, then page 2 — so slot 3 is
/// page 2. B slots: page 0 as two halves, then pages 1 and 2 — so slot 3
/// is page 2 there too, but slot 1 is page 0 rather than page 1. Mapping
/// only one end, or neither, still produces a score for every page and
/// puts it against the wrong picture.
#[test]
fn slot_scores_are_put_back_onto_the_pages_at_both_ends() {
    let a_slots = vec![
        Slot {
            page: 0,
            half: None,
        },
        Slot {
            page: 1,
            half: Some(Half::Right),
        },
        Slot {
            page: 1,
            half: Some(Half::Left),
        },
        Slot {
            page: 2,
            half: None,
        },
    ];
    let b_slots = vec![
        Slot {
            page: 0,
            half: Some(Half::Right),
        },
        Slot {
            page: 0,
            half: Some(Half::Left),
        },
        Slot {
            page: 1,
            half: None,
        },
        Slot {
            page: 2,
            half: None,
        },
    ];
    let score = |page, counterpart| taiyaku_core::PageConfidence {
        page,
        counterpart,
        similarity: 0.9,
        margin: 0.3,
    };

    let on = on_pages(
        vec![score(3, Some(1)), score(0, None), score(9, Some(0))],
        &a_slots,
        &b_slots,
    );

    assert_eq!(
        on.iter()
            .map(|s| (s.page, s.counterpart))
            .collect::<Vec<_>>(),
        vec![(2, Some(0)), (0, None)],
        "slot 3 is page 2 in A and slot 1 is page 0 in B; slot 9 is neither"
    );
    assert_eq!(on[0].margin, 0.3, "the score itself is carried through");
}

/// The count printed as "to review", which is a count of pairings rather
/// than of pages: an unpaired page had no second-best to be close to.
#[test]
fn only_a_pairing_with_a_narrow_margin_is_ambiguous() {
    let score = |counterpart, margin| taiyaku_core::PageConfidence {
        page: 0,
        counterpart,
        similarity: 0.9,
        margin,
    };

    // Two below the threshold rather than one: with a single narrow
    // pairing, "below 0.05" and "exactly 0.05" both count one thing, and
    // the test cannot tell a threshold from an equality.
    assert_eq!(
        ambiguous_pairings(&[
            score(Some(1), 0.01),
            score(Some(2), 0.02),
            score(Some(3), 0.9),
            score(None, 0.0),
            score(Some(4), AMBIGUOUS_MARGIN),
        ]),
        2,
        "two narrow pairings; a wide one, a gap and one exactly at the \
             threshold are not ambiguous"
    );
    assert_eq!(ambiguous_pairings(&[]), 0);
}

/// The counts under the alignment table. They partition the slots, so a
/// fixture holding all three kinds is the only one that can tell them
/// apart — count each kind as "everything that is not this kind" and the
/// three numbers still sum to the right total.
#[test]
fn the_pairing_tally_splits_the_slots_three_ways() {
    let kinds = [
        Pairing::Whole,
        Pairing::Half,
        Pairing::Whole,
        Pairing::Gap,
        Pairing::Whole,
    ];

    assert_eq!(tally(&kinds), (3, 1, 1));
    assert_eq!(
        tally(&kinds).0 + tally(&kinds).1 + tally(&kinds).2,
        kinds.len(),
        "every slot is counted exactly once"
    );
    assert_eq!(tally(&[]), (0, 0, 0));
}

/// A joined spread becomes two slots in reading order, and a page that is
/// not one keeps its own hash. The flags say which happened, because the
/// slot table is built from them.
#[test]
fn a_joined_spread_becomes_two_slot_hashes_in_reading_order() {
    let hash = |n: u64| taiyaku_core::PageHash([n, 0, 0, 0]);
    // Two portraits around one page twice as wide, which is what makes the
    // middle one read as a spread against the edition it sits in.
    let described = vec![
        cache::CachedPage {
            width: 100,
            height: 150,
            hash: hash(1),
            halves: None,
        },
        cache::CachedPage {
            width: 200,
            height: 150,
            hash: hash(2),
            halves: Some((hash(20), hash(21))),
        },
        cache::CachedPage {
            width: 100,
            height: 150,
            hash: hash(3),
            halves: None,
        },
    ];

    let (hashes, wide) = slot_hashes(&described, Direction::RightToLeft);
    assert_eq!(
        hashes,
        vec![hash(1), hash(21), hash(20), hash(3)],
        "manga reads the right half first"
    );
    assert_eq!(wide, vec![false, true, false]);

    let (hashes, _) = slot_hashes(&described, Direction::LeftToRight);
    assert_eq!(
        hashes,
        vec![hash(1), hash(20), hash(21), hash(3)],
        "and a western comic reads the left half first"
    );

    // A page wide enough to read as a spread but with no halves cached —
    // an entry written before halves were stored — keeps its whole hash
    // rather than contributing nothing.
    let mut without = described.clone();
    without[1].halves = None;
    let (hashes, wide) = slot_hashes(&without, Direction::RightToLeft);
    assert_eq!(hashes, vec![hash(1), hash(2), hash(3)]);
    assert_eq!(wide, vec![false, false, false]);
}

/// Spreads counted from the slot table, where each one appears twice.
#[test]
fn a_spread_is_two_slots_and_is_counted_once() {
    let whole = |page| Slot { page, half: None };
    let half = |page, side| Slot {
        page,
        half: Some(side),
    };

    assert_eq!(
        spreads_in(&[
            whole(0),
            half(1, Half::Right),
            half(1, Half::Left),
            whole(2),
            half(3, Half::Right),
            half(3, Half::Left),
        ]),
        2
    );
    assert_eq!(spreads_in(&[whole(0), whole(1)]), 0);
    assert_eq!(spreads_in(&[]), 0);
}

/// A pairing involves a spread if *either* side is a half. Which side is
/// halved depends on which way round the pair was given — raws split
/// spreads and English releases join them, and the colour editions join
/// them too — so a rule naming one side would count the same pairing
/// differently in each direction.
#[test]
fn a_pairing_involves_a_spread_whichever_side_is_the_half() {
    let whole = Slot {
        page: 4,
        half: None,
    };
    let half = Slot {
        page: 7,
        half: Some(Half::Left),
    };

    assert_eq!(pairing_of(whole, Some(whole)), Pairing::Whole);
    assert_eq!(pairing_of(half, Some(whole)), Pairing::Half);
    assert_eq!(pairing_of(whole, Some(half)), Pairing::Half);
    assert_eq!(pairing_of(half, Some(half)), Pairing::Half);
    assert_eq!(pairing_of(whole, None), Pairing::Gap);
    assert_eq!(pairing_of(half, None), Pairing::Gap);
}

/// `--write` merges, so it reads first — and a read that fails for any
/// reason other than the file not being there has to stop the write. It
/// used to be a bare `fs::write`, which silently discarded a document that
/// may hold four other editions, and treating every read error as "nothing
/// to merge" puts that back.
#[test]
fn a_sidecar_that_cannot_be_read_stops_the_write_rather_than_being_absent() {
    let root = std::env::temp_dir().join("taiyaku-test-existing-sidecar");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    assert!(
        existing_sidecar(&root.join("nothing-here.json").to_string_lossy())
            .expect("an absent file is a new document")
            .is_none()
    );

    // A directory is readable metadata and unreadable content: the error
    // is not `NotFound`, so it must be reported rather than overwritten.
    let error = existing_sidecar(&root.to_string_lossy())
        .expect_err("a path that is not a file is not an absent file");

    let _ = std::fs::remove_dir_all(&root);
    assert!(
        error.contains(&root.to_string_lossy().to_string()),
        "the refusal must name the path: {error}"
    );
}

/// The same rule for a document this build cannot fully read.
///
/// `--write` merges into whatever is already at `--out`, so a document from a
/// newer taiyaku would be parsed with every unknown field dropped, merged, and
/// written back stamped version 1 — over the reader's only copy, which is the
/// exact loss `existing_sidecar` was introduced to prevent for an unreadable
/// file. `taiyaku_library::import` refused this at the library's entry points
/// and `align` never did; `Sidecar::from_json` refuses it now, for every
/// command at once, and this pins that `align --write` is one of them.
#[test]
fn a_sidecar_from_a_newer_taiyaku_stops_the_write_rather_than_being_downgraded() {
    let root = std::env::temp_dir().join(format!(
        "taiyaku-test-existing-sidecar-newer-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("taiyaku.json");
    std::fs::write(
        &path,
        format!(
            r#"{{"taiyaku": {}, "work": {{"title": "T", "reading": "rtl"}},
                "editions": {{"a": {{"label": "A", "members": [
                  {{"path": "a.cbz", "sha256": "00", "pages": 2}}]}}}},
                "hub": "a", "alignments": []}}"#,
            taiyaku_core::VERSION + 1
        ),
    )
    .unwrap();

    let error = existing_sidecar(&path.to_string_lossy())
        .expect_err("a document this build cannot read is not a document to merge into");
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        error.contains(&format!("version {}", taiyaku_core::VERSION + 1)),
        "the refusal must name the version it was handed: {error}"
    );
}

/// How many members came from cache, which is the difference between a
/// cold run and a warm one and is reported to say which just happened.
///
/// Counted rather than inferred: a count that never rises reads as an
/// honest cold run every time, and the only thing wrong is that the
/// three-orders-of-magnitude saving the cache exists for is invisible.
#[test]
fn a_warm_edition_reports_every_member_as_a_cache_hit() {
    let root = std::env::temp_dir().join("taiyaku-test-cached-edition");
    let _ = std::fs::remove_dir_all(&root);
    // The two members hold different pages. Identical ones would share a
    // digest, and the second would be answered from the entry the first
    // just wrote — one hit on a cold run, which is correct behaviour and
    // not what this is measuring.
    let members: Vec<PathBuf> = ["c1", "c2"]
        .iter()
        .enumerate()
        .map(|(chapter, name)| {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            for page in 0..2u32 {
                let chapter = u32::try_from(chapter)
                    .expect("chapter is 0 or 1, from enumerate() over the fixed 2-element array [\"c1\", \"c2\"] above");
                let seed = page * 53 + chapter * 11;
                let image = image::GrayImage::from_fn(24, 32, |x, y| {
                    image::Luma([((x * 7 + y * 31 + seed) % 200) as u8])
                });
                image.save(dir.join(format!("{page:03}.png"))).unwrap();
            }
            dir
        })
        .collect();
    let cache_root = root.join("cache");

    let cold = cached_edition(&cache_root, &members, Direction::RightToLeft);
    let warm = cached_edition(&cache_root, &members, Direction::RightToLeft);

    let _ = std::fs::remove_dir_all(&root);
    let (cold_hashes, _, cold_hits) = cold.expect("decodes");
    let (warm_hashes, _, warm_hits) = warm.expect("reads the cache");
    assert_eq!(cold_hits, 0, "nothing is cached on the first run");
    assert_eq!(warm_hits, members.len(), "both members on the second");
    assert_eq!(cold_hashes.len(), 4, "two members of two pages each");
    assert_eq!(cold_hashes, warm_hashes);
}

/// The summary under a report: a line per run, then the figures.
///
/// Both are returned rather than printed because they go to different
/// streams and a survey script reads one without the other — and because
/// neither the paired total nor the runs-per-hundred rate is checkable at
/// all while they only ever reach a terminal.
#[test]
fn a_report_summary_counts_paired_pages_and_the_run_rate() {
    let runs = vec![
        run_between((0, 9), (0, 9)),
        Run::gap(taiyaku_core::PageRange::new(10, 14)),
        run_between((15, 19), (10, 14)),
    ];

    let (table, figures) = report_summary(&runs, 20);

    assert_eq!(table.lines().count(), 3, "one line per run, gaps included");
    assert!(
        table.contains("offset +0") && table.contains("offset -5"),
        "each paired run reports the offset it sits at: {table}"
    );
    assert!(table.contains("gap"), "and a gap says so: {table}");
    assert!(
        figures.starts_with("3 runs | 15/20 pages paired (75.0%) | 15.0 runs per 100 pages"),
        "three runs over twenty pages, fifteen of them paired: {figures}"
    );

    // A hundred pages makes the rate and the run count different numbers,
    // so neither can stand in for the other.
    let (_, figures) = report_summary(&runs, 100);
    assert!(
        figures.contains("3.0 runs per 100 pages"),
        "the rate is per hundred pages, not per alignment: {figures}"
    );

    let (table, figures) = report_summary(&[], 0);
    assert_eq!(table, "", "no runs, no table");
    assert!(
        figures.starts_with("0 runs | 0/0 pages paired (0.0%)"),
        "an empty alignment divides by the floor rather than by zero: {figures}"
    );
}

/// A shell glob is not in reading order, and the sidecar used to record it
/// verbatim while the solver quietly worked in natural order — so a
/// document's member list disagreed with the page sequence its own runs
/// addressed. Nothing in this repository reads member order, which is why
/// it went unnoticed; a reader that concatenates `members` to rebuild the
/// sequence would show the wrong page for every lookup.
///
/// Kaguya's fan translation is the reproduction: `*.cbz` yields
/// `c10.5, c10, c1, c2, …` and the solve used `c1, c2, … c10, c10.5`.
/// Ordering here rather than in each consumer is what makes them agree —
/// `slug` and `label` name an edition after its first member, and they
/// were reading the glob's first member too.
#[test]
fn both_editions_come_back_in_natural_order() {
    let args: Vec<String> = [
        "v02",
        "v01",
        "--",
        "c10.5.cbz",
        "c10.cbz",
        "c1.cbz",
        "c2.cbz",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();

    let (a, b) = split_editions(&args).unwrap();

    assert_eq!(a, ["v01", "v02"].map(PathBuf::from));
    assert_eq!(
        b,
        ["c1.cbz", "c2.cbz", "c10.cbz", "c10.5.cbz"].map(PathBuf::from)
    );
}

/// `docs/sidecar.md` says a member's `path` is relative to the sidecar, and
/// `LocalLibrary` resolves it that way. `describe_edition` wrote the path as
/// typed on the command line, which is relative to the working directory —
/// so a sidecar written from one directory and stored in another named
/// members that were not there, and every page 404'd.
///
/// The same class as the member-order bug: the writer disagreeing with a
/// rule the format states, invisible until something resolved it.
#[test]
fn a_member_path_is_recorded_relative_to_the_sidecar() {
    let root = std::env::temp_dir().join("taiyaku-test-relative");
    let _ = std::fs::remove_dir_all(&root);
    let library = root.join("library");
    let beside = root.join("sidecars");
    std::fs::create_dir_all(&library).unwrap();
    std::fs::create_dir_all(&beside).unwrap();
    let member = library.join("v01.cbz");
    std::fs::write(&member, b"placeholder").unwrap();

    let recorded = relative_to(&member, &beside).unwrap();

    let resolved = beside.join(&recorded);
    let ok = resolved.canonicalize().ok() == member.canonicalize().ok();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(recorded, PathBuf::from("../library/v01.cbz"));
    assert!(ok, "{} should resolve to the member", recorded.display());
}

/// The sibling case above is not the common one. The normal layout is a
/// sidecar sitting beside or inside the corpus it describes — `taiyaku.json`
/// at the root, members underneath it — which the sibling test does not
/// exercise at all. A regression that broke only this case would pass the
/// existing suite.
#[test]
fn a_member_inside_the_base_directory_is_recorded_relative_to_it() {
    let root = std::env::temp_dir().join("taiyaku-test-relative-nested");
    let _ = std::fs::remove_dir_all(&root);
    let raw_mono = root.join("raw-mono");
    std::fs::create_dir_all(&raw_mono).unwrap();
    let member = raw_mono.join("v01.cbz");
    std::fs::write(&member, b"placeholder").unwrap();
    // Sidecar at <root>/taiyaku.json, so its directory is <root> itself.
    let beside = root.clone();

    let recorded = relative_to(&member, &beside).unwrap();

    let resolved = beside.join(&recorded);
    let ok = resolved.canonicalize().ok() == member.canonicalize().ok();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(recorded, PathBuf::from("raw-mono/v01.cbz"));
    assert!(ok, "{} should resolve to the member", recorded.display());
}

/// A `let-else` pattern's bindings are not in scope inside its `else`
/// block, so `let (Ok(target), Ok(base)) = (...) else { return
/// target... }` returns the *parameter*, not a canonicalised path. In this
/// call chain that parameter is exactly the working-directory-relative
/// path this function exists to stop writing into the sidecar — the
/// fallback was reintroducing the bug it exists to prevent.
///
/// Constructed the way the review asked: a base that cannot be resolved (a
/// sidecar directory that does not exist) and a relative target, the shape
/// a caller's argument actually has. Before the fix this returned
/// `target` completely unchanged, `relative/thing.cbz` — not absolute, not
/// even looked at. After the fix it returns an absolute path.
///
/// Pinned against `current_dir().join(target)` rather than only asserting
/// `is_absolute()`: the weaker assertion passes for any absolute path at
/// all, including one built from `base`, so it would not have noticed the
/// two being swapped. What has to hold is that the recorded path still
/// names the *member*.
#[test]
fn relative_to_returns_an_absolute_path_when_the_base_cannot_be_resolved() {
    let target = Path::new("relative/thing.cbz");
    let missing_base = std::env::temp_dir().join("taiyaku-test-relative-missing-base");
    let _ = std::fs::remove_dir_all(&missing_base);

    let recorded = relative_to(target, &missing_base).unwrap();

    assert!(
        recorded.is_absolute(),
        "expected an absolute path when the base cannot be resolved, got {}",
        recorded.display()
    );
    assert_eq!(
        recorded,
        std::env::current_dir().unwrap().join(target),
        "must be the member made absolute, not the base and not the raw argument"
    );
    assert!(
        recorded.ends_with("relative/thing.cbz"),
        "must still name the member: {}",
        recorded.display()
    );
}

/// `Path::new("taiyaku.json").parent()` returns `Some("")`, not `None`,
/// for a bare filename — the guard at the `--write` call site exists
/// because computing member paths against an empty base would be silently
/// wrong. Pinned directly rather than only through the inline expression
/// it used to be.
#[test]
fn sidecar_dir_falls_back_to_the_current_directory_for_a_bare_filename() {
    assert_eq!(sidecar_dir(Path::new("taiyaku.json")), Path::new("."));
}

#[test]
fn sidecar_dir_is_the_parent_of_a_pathed_filename() {
    assert_eq!(sidecar_dir(Path::new("out/taiyaku.json")), Path::new("out"));
}

/// A star with a hand-corrected run in it: what `align --write` rewrites
/// wholesale in order to add or replace one spoke.
fn three_editions_one_hand_corrected() -> Sidecar {
    let text = r#"{
          "taiyaku": 1,
          "work": { "title": "Reversed Order Co.", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 8 }],
              "spreads": []
            },
            "en-fan": {
              "label": "en-fan",
              "members": [{ "path": "f.cbz", "sha256": "f0", "pages": 8 }],
              "spreads": []
            },
            "en-mono": {
              "label": "en-mono",
              "members": [{ "path": "m.cbz", "sha256": "m0", "pages": 8 }],
              "spreads": []
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "en-fan", "by": "solver",
              "runs": [{"a":"0-3","b":"0-3"},{"a":"4-7","b":"4-7","by":"hand"}] },
            { "from": "raw", "to": "en-mono", "by": "solver",
              "runs": [{"a":"0-7","b":"0-7"}] }
          ]
        }"#;
    Sidecar::from_json(text).expect("fixture parses")
}

/// O26: `align --write` must not be able to leave a short sidecar behind.
///
/// The property O2 established for progress and `atomic.rs` now owns for
/// everything that writes beside a sidecar: the destination is *replaced*
/// by a rename, never opened for truncation. A crash cannot be staged in a
/// test, so this pins the mechanism that rules it out — the same shape
/// `credits::tests::writing_a_credits_list_replaces_the_sidecar_rather_than_truncating_it`
/// set for the command next door.
///
/// Observed by **inode identity** rather than by taking write permission
/// off the existing file. `atomic.rs`'s own test records why: on a
/// filesystem that ignores the read-only bit, or under root, the
/// permission-denied write that approach distinguishes "replaced" from
/// "truncated" by simply succeeds, and the skip line it prints is captured
/// by libtest — a green that measured nothing. A rename gives the path a
/// different inode; an in-place write keeps the same one, whoever is
/// running and whatever the filesystem enforces.
///
/// The fixture is a **three-edition star carrying a hand correction**,
/// because that is what is actually at stake here and it is more than
/// `credits` risks: `align --write` rewrites the whole document to add or
/// replace *one* spoke, so a half-written file takes the other editions,
/// the edges joining them, and every run a reader corrected through the
/// reading UI with it.
#[test]
fn writing_an_alignment_replaces_the_sidecar_rather_than_truncating_it() {
    let dir = std::env::temp_dir().join("taiyaku-test-align-write");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("taiyaku.json");
    let out = path.to_str().expect("utf-8 temp path").to_string();

    let doc = three_editions_one_hand_corrected();
    std::fs::write(&path, doc.to_json()).expect("the fixture is written");

    #[cfg(unix)]
    let before = {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(&path).expect("stat").ino()
    };

    let wrote = write_sidecar(&out, &doc);

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

    // Whole, not half: everything the write never touched came through.
    let written = Sidecar::from_json(&read_back).expect("the destination holds a whole document");
    assert_eq!(
        written.editions.len(),
        3,
        "every edition must survive the rewrite"
    );
    assert_eq!(
        written.alignments.len(),
        2,
        "every edge must survive the rewrite"
    );
    assert!(
        written
            .alignments
            .iter()
            .flat_map(|edge| &edge.runs)
            .any(|run| run.by == Some(Source::Hand)),
        "the hand-corrected run must survive the rewrite"
    );
}

/// Important 1a: an edition key reaches two URLs —
/// `/api/works/{work}/{edition}/pages/{n}` and the OPDS stream route —
/// verbatim. `--name-a`/`--name-b` used to hand `slug` a caller's string
/// unexamined, so `taiyaku align --name-a "raw color"` wrote a key
/// containing a space, breaking both `href`s it went into. Pinned to the
/// exact wording the review asked for: the key, the offending character,
/// its position, and a suggestion.
#[test]
fn validate_key_names_the_character_position_and_a_suggestion() {
    let err = validate_key("raw color").unwrap_err();
    assert_eq!(
        err,
        "edition key `raw color` is not URL-safe (space at position 3); try `raw-color`"
    );
}

/// A `/` is worse than a space: it inserts a path segment the registered
/// route cannot parse back at all, rather than merely mangling one.
#[test]
fn validate_key_names_a_non_space_offender_in_backticks() {
    let err = validate_key("raw/mono").unwrap_err();
    assert_eq!(
        err,
        "edition key `raw/mono` is not URL-safe (`/` at position 3); try `raw-mono`"
    );
}

/// The alphabet the auto-derived slug already produces — ASCII
/// alphanumerics and `-` — has to be accepted, or a caller cannot ever
/// pass `--name-a`/`--name-b` at all.
#[test]
fn validate_key_accepts_ascii_alphanumerics_and_hyphens() {
    assert!(validate_key("raw-mono-2").is_ok());
}

/// The one bad key the character scan cannot see, because it scans no
/// characters and so finds nothing to object to. An empty key is not a
/// usable path segment, which is the property this whole function is
/// about, so it has to be refused by its own branch rather than falling
/// through the loop as safe.
#[test]
fn validate_key_refuses_an_empty_key() {
    let err = validate_key("").unwrap_err();
    assert!(
        err.contains("empty"),
        "must say what is wrong with it: {err}"
    );
    assert!(
        !err.contains("position"),
        "there is no offending position to name: {err}"
    );
}

/// The end-to-end path: `align` itself refuses the bad key, and does so
/// before either edition is read — these member paths do not exist on
/// disk, and a version of this check that ran after fingerprinting would
/// fail on a file-not-found instead of ever reaching the validation.
#[test]
fn align_refuses_a_name_a_key_that_is_not_url_safe_before_touching_any_archive() {
    let args: Vec<String> = [
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--name-a",
        "raw color",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();

    let err = align(&args).unwrap_err();

    assert_eq!(
        err,
        "edition key `raw color` is not URL-safe (space at position 3); try `raw-color`"
    );
}

/// The same check on the other flag, so a bad `--name-b` cannot slip
/// through because only `--name-a` was checked.
#[test]
fn align_refuses_a_name_b_key_that_is_not_url_safe_before_touching_any_archive() {
    let args: Vec<String> = [
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--name-b",
        "en/fan",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();

    let err = align(&args).unwrap_err();

    assert_eq!(
        err,
        "edition key `en/fan` is not URL-safe (`/` at position 2); try `en-fan`"
    );
}

fn argv(args: &[&str]) -> Vec<String> {
    args.iter().map(ToString::to_string).collect()
}

/// Splitting joined spreads is what the corpus README has said to do
/// "essentially always" since before the flag had a second user, so it is
/// the default. The observable consequence at this level: `--write` alone
/// no longer trips O19's refusal, because the run it names now detects
/// spreads.
#[test]
fn writing_a_sidecar_no_longer_needs_a_flag_to_enable_it() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--write",
        "taiyaku.json",
    ]);

    let err = align(&args).unwrap_err();

    assert!(
        !err.contains("needs"),
        "should have got past argument handling, but refused: {err}"
    );
}

/// O19's rule survives the inversion — it now keys off the flag that asks
/// for the page-level path rather than off the absence of the one that
/// asked for the slot-level path. The reason is unchanged: that path
/// cannot describe an edition's spreads, so it cannot write a sidecar.
#[test]
fn align_refuses_write_with_no_split_spreads() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--no-split-spreads",
        "--write",
        "taiyaku.json",
    ]);

    assert_eq!(
        align(&args).unwrap_err(),
        "--write cannot be used with --no-split-spreads: a sidecar records \
             which pages are joined spreads, and that path does not detect them"
    );
}

/// The retired flag explains itself rather than being accepted as a no-op
/// or reported as a typo. Accepting it would break the rule O19 and O20
/// established — a flag that cannot change what a run does is refused —
/// and `reject_unknown_flags` alone would call yesterday's documented flag
/// a misspelling.
#[test]
fn the_retired_split_spreads_flag_says_what_replaced_it() {
    let args = argv(&["a.cbz", "--", "b.cbz", "--split-spreads"]);

    assert_eq!(
        align(&args).unwrap_err(),
        "--split-spreads is retired: splitting joined spreads is now what \
             align does by default, so the flag had nothing left to turn on; \
             pass --no-split-spreads for the page-level diagnostic"
    );
}

/// O20's two split-path flags follow the same inversion.
#[test]
fn align_refuses_ltr_with_no_split_spreads() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--no-split-spreads",
        "--ltr",
    ]);

    assert_eq!(
        align(&args).unwrap_err(),
        "--ltr cannot be used with --no-split-spreads: that path pairs pages \
             rather than slots, and a page pairing has no direction to read"
    );
}

/// The plain diagnostic is still reachable, and still takes `--stats`.
#[test]
fn the_page_level_diagnostic_is_still_reachable() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--no-split-spreads",
        "--stats",
    ]);

    let err = align(&args).unwrap_err();

    assert!(
        !err.contains("cannot be used") && !err.contains("no flag"),
        "the diagnostic path was refused: {err}"
    );
}

/// O21. O15, O19 and O20 all key off a flag's *name*, so none of them can
/// see a name that was typed wrong — and a misspelling reproduces exactly
/// the failure O19 closed: `--writ` is not `--write`, so the run reports a
/// plausible alignment and writes nothing, at exit 0.
#[test]
fn a_misspelled_flag_is_refused_rather_than_ignored() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--writ",
        "taiyaku.json",
    ]);

    assert_eq!(
        align(&args).unwrap_err(),
        "align has no flag `--writ`; its flags are --cache, --direct, --ltr, \
             --name-a, --name-b, --no-split-spreads, --stats, --title, --write"
    );
}

/// The near-miss that motivates naming the alphabet in the message: this
/// one differs from a real flag by a single character and produces a
/// plausible-looking answer rather than an obvious failure.
#[test]
fn a_flag_missing_one_letter_is_refused() {
    let args = argv(&["a.cbz", "--", "b.cbz", "--split-spread"]);

    assert!(
        align(&args)
            .unwrap_err()
            .starts_with("align has no flag `--split-spread`;"),
        "expected an unknown-flag refusal naming the typo"
    );
}

/// Each command validates against its own set rather than a global one,
/// so a flag that is real elsewhere is still refused here.
#[test]
fn a_flag_belonging_to_another_command_is_refused() {
    let args = argv(&["some-root", "--split-spreads"]);

    assert_eq!(
        inventory::command(&args).unwrap_err(),
        "inventory has no flag `--split-spreads`; its flags are --min-pages, --pattern"
    );
}

/// O22, O20's rule in the command O20 did not touch. Measured before the
/// fix: `verify a -- b --cache DIR` never created DIR, because
/// `cached_edition` is reached only from the `--sidecar` arm — without one
/// `verify` solves the pair directly and the cache is never consulted.
#[test]
fn verify_refuses_a_cache_it_would_never_read() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--cache",
        "somewhere",
    ]);

    assert_eq!(
        verify(&args).unwrap_err(),
        "--cache needs --sidecar: without one verify solves the pair \
             directly and reads nothing through the cache"
    );
}

/// The same arm carries `direction`, so `--ltr` is dead in `verify` for
/// the same reason it is dead on `align`'s plain path — and with a sidecar
/// it is the document's own reading order that indexes the runs anyway.
#[test]
fn verify_refuses_ltr_without_a_sidecar_to_apply_it_to() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--ltr",
    ]);

    assert_eq!(
        verify(&args).unwrap_err(),
        "--ltr needs --sidecar: without one verify pairs pages rather than \
             slots, and a page pairing has no direction to read"
    );
}

/// The over-refusal guard: with a sidecar named, both get past argument
/// handling and fail on the archives instead.
#[test]
fn verify_allows_cache_and_ltr_when_a_sidecar_is_named() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--sidecar",
        "taiyaku.json",
        "--cache",
        "somewhere",
        "--ltr",
    ]);

    let err = verify(&args).unwrap_err();

    assert!(
        !err.contains("needs"),
        "should have got past argument handling, but refused: {err}"
    );
}

/// Two commands take no flags at all, where "its flags are " would trail
/// off into nothing.
#[test]
fn a_command_with_no_flags_says_so_rather_than_listing_an_empty_set() {
    let args = argv(&["taiyaku.json", "raw", "en", "--stats"]);

    assert_eq!(
        lookup(&args).unwrap_err(),
        "lookup takes no flags, and `--stats` was given as one"
    );
}

/// `--` is the edition separator, not a flag, and must survive a scan that
/// refuses everything beginning with `--`.
#[test]
fn the_edition_separator_is_not_treated_as_an_unknown_flag() {
    let args = argv(&["does-not-exist-a.cbz", "--", "does-not-exist-b.cbz"]);

    let err = align(&args).unwrap_err();

    assert!(
        !err.contains("no flag"),
        "the separator was refused as a flag: {err}"
    );
}

/// #3, folded in here because the fix is the same scan: a flag typed on
/// the a-side lands in the member list, and used to be reported as a file
/// that does not exist — which names the wrong problem entirely.
#[test]
fn a_flag_before_the_separator_says_so_rather_than_reporting_a_missing_file() {
    let args = argv(&["a.cbz", "--stats", "--", "b.cbz"]);

    assert_eq!(
        align(&args).unwrap_err(),
        "`--stats` is a flag, but it is before the `--` that separates the \
             two editions, so it was read as a member; flags go after both"
    );
}

/// The flag was not typed, so there is nothing to complain about and the
/// caller's default stands. This is the path every `unwrap_or` below
/// depends on, and the one a refusal must not break.
#[test]
fn an_absent_flag_is_absent_rather_than_an_error() {
    let args = argv(&["a.cbz", "--", "b.cbz"]);

    assert_eq!(option_value(&args, "--write", "a path"), Ok(None));
}

#[test]
fn a_flag_with_a_value_still_returns_it() {
    let args = argv(&["a.cbz", "--", "b.cbz", "--write", "taiyaku.json"]);

    assert_eq!(
        option_value(&args, "--write", "a path"),
        Ok(Some("taiyaku.json"))
    );
}

/// The first of O15's two silences: a flag last on the line has nothing
/// after it, and `args.get(at + 1)` answering `None` is indistinguishable
/// from the flag never being typed. `taiyaku inventory releases
/// --min-pages` walked with the default threshold of 60 and said nothing.
#[test]
fn a_flag_with_nothing_after_it_is_refused_rather_than_read_as_absent() {
    let args = argv(&["a.cbz", "--", "b.cbz", "--write"]);

    assert_eq!(
        option_value(&args, "--write", "a path"),
        Err("--write needs a path, and nothing followed it".to_string())
    );
}

/// The second: the argument after the flag is taken verbatim however it
/// reads, so `align a -- b --write --ltr` wrote the sidecar to a file
/// named `--ltr` — and applied the direction anyway, since `--ltr` is
/// matched separately, so nothing reported a problem.
#[test]
fn a_flag_followed_by_another_flag_is_refused_rather_than_taking_it_as_the_value() {
    let args = argv(&["a.cbz", "--", "b.cbz", "--write", "--ltr"]);

    assert_eq!(
        option_value(&args, "--write", "a path"),
        Err("--write needs a path, and `--ltr` is another flag".to_string())
    );
}

/// The refusal is worth nothing if it arrives after fingerprinting, which
/// is the argument the `--name-a` check above already makes — these member
/// paths do not exist, so a version of this that resolved options late
/// would fail on a file-not-found instead.
#[test]
fn align_refuses_a_valueless_write_before_touching_any_archive() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--write",
    ]);

    assert_eq!(
        align(&args).unwrap_err(),
        "--write needs a path, and nothing followed it"
    );
}

/// O20, the four flags that describe a sidecar. Dead on *both* paths when
/// nothing is written, not merely on the page-level one — the default
/// path without `--write` skips the block that reads them just as
/// `--no-split-spreads` does.
#[test]
fn align_refuses_a_title_when_no_sidecar_is_written() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--title",
        "Some Work",
    ]);

    assert_eq!(
        align(&args).unwrap_err(),
        "--title needs --write: it describes a sidecar, and this run writes none"
    );
}

/// The same rule reached through a flag that carries no value, so the
/// check cannot be spelled "a resolved option was Some".
#[test]
fn align_refuses_direct_when_no_sidecar_is_written() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--direct",
    ]);

    assert_eq!(
        align(&args).unwrap_err(),
        "--direct needs --write: it describes a sidecar, and this run writes none"
    );
}

/// O20's other half: two flags that are live on the default path and inert
/// on the page-level one. `--cache` writes the fingerprint cache there and
/// nothing at all here, measured — the directory is never created.
#[test]
fn align_refuses_a_cache_the_page_level_path_would_never_write() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--no-split-spreads",
        "--cache",
        "somewhere",
    ]);

    assert_eq!(
        align(&args).unwrap_err(),
        "--cache cannot be used with --no-split-spreads: that path \
             fingerprints directly and reads nothing through the cache"
    );
}

/// The guard against over-refusing: with a sidecar actually being written,
/// every one of these gets past argument handling and fails on the
/// archives instead.
#[test]
fn align_allows_the_sidecar_flags_when_one_is_written() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--write",
        "taiyaku.json",
        "--title",
        "Some Work",
        "--name-a",
        "raw",
        "--ltr",
        "--cache",
        "somewhere",
    ]);

    let err = align(&args).unwrap_err();

    assert!(
        !err.contains("needs"),
        "should have got past argument handling, but refused: {err}"
    );
}

/// `validate_key` does not catch this one and cannot: `-` is in its safe
/// alphabet, so `--name-b` is a well-formed edition key. Before this,
/// `align … --name-a --name-b` wrote an edition keyed `--name-b` into the
/// sidecar, where it becomes a path segment in two URLs.
#[test]
fn align_refuses_a_name_a_that_swallowed_the_next_flag() {
    let args = argv(&[
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--name-a",
        "--name-b",
    ]);

    assert_eq!(
        align(&args).unwrap_err(),
        "--name-a needs an edition key, and `--name-b` is another flag"
    );
}

// -- slug, label, url_safe_suggestion: pure formatting, no archives needed --

/// A caller-given name is trusted verbatim; the auto-derivation below is
/// never consulted.
#[test]
fn slug_returns_the_given_name_unchanged() {
    assert_eq!(slug(&[], Some("raw-mono")), "raw-mono");
}

/// The common case: a stem with spaces becomes hyphens, lowercased.
#[test]
fn slug_derives_from_the_first_members_stem() {
    let members = [PathBuf::from("Raw Volume 01.cbz"), PathBuf::from("z.cbz")];
    assert_eq!(slug(&members, None), "raw-volume-01");
}

/// The fallback this function shares with `url_safe_suggestion`: a stem
/// that is nothing but unsafe characters trims to empty, and an empty slug
/// is not a usable edition key.
#[test]
fn slug_falls_back_to_edition_when_the_stem_has_no_safe_characters() {
    assert_eq!(slug(&[], None), "edition");
    assert_eq!(slug(&[PathBuf::from("!!!.cbz")], None), "edition");
}

/// `validate_key`'s suggestion goes through this same fallback when the
/// key survives on nothing: every character is unsafe, so the slug it
/// proposes must still be a usable key rather than an empty string.
#[test]
fn validate_key_suggests_edition_when_nothing_survives_the_slug() {
    let err = validate_key("!!!").unwrap_err();
    assert_eq!(
        err,
        "edition key `!!!` is not URL-safe (`!` at position 0); try `edition`"
    );
}

#[test]
fn label_of_no_members_is_empty() {
    assert_eq!(label(&[]), "");
}

#[test]
fn label_of_one_member_is_its_file_name() {
    assert_eq!(label(&[PathBuf::from("a.cbz")]), "a.cbz");
}

#[test]
fn label_of_several_members_counts_the_rest() {
    let members = [
        PathBuf::from("a.cbz"),
        PathBuf::from("b.cbz"),
        PathBuf::from("c.cbz"),
    ];
    assert_eq!(label(&members), "a.cbz + 2 more");
}

/// `split_editions`' other refusal: the `--` was found, but one side of it
/// is empty. A member list with nothing in it cannot be aligned against
/// anything.
#[test]
fn split_editions_refuses_an_empty_side() {
    let err = split_editions(&argv(&["--", "b.cbz"])).unwrap_err();
    assert_eq!(err, "both sides need at least one member");
}

// -- run(): the dispatcher itself, not just the commands it calls --
//
// Every test above drives a command function directly, which never
// exercises `run`'s own `match` — the line that turns `"align"` into a
// call to `align` could be deleted, or point at the wrong function
// entirely, and nothing above would notice. These call `run` and lean on
// each command's own distinctive refusal to prove which function actually
// ran, the same way `a_flag_belonging_to_another_command_is_refused`
// proves `reject_unknown_flags` is not shared.

#[test]
fn run_dispatches_to_align() {
    let args = argv(&["align", "a.cbz", "--", "b.cbz", "--split-spreads"]);
    assert_eq!(
        run(&args).unwrap_err(),
        "--split-spreads is retired: splitting joined spreads is now what \
             align does by default, so the flag had nothing left to turn on; \
             pass --no-split-spreads for the page-level diagnostic"
    );
}

#[test]
fn run_dispatches_to_verify() {
    let args = argv(&[
        "verify",
        "does-not-exist-a.cbz",
        "--",
        "does-not-exist-b.cbz",
        "--cache",
        "somewhere",
    ]);
    assert_eq!(
        run(&args).unwrap_err(),
        "--cache needs --sidecar: without one verify solves the pair \
             directly and reads nothing through the cache"
    );
}

#[test]
fn run_dispatches_to_topology() {
    assert_eq!(
        run(&argv(&["topology"])).unwrap_err(),
        "needs a hub and at least two spokes, separated by `--`"
    );
}

#[test]
fn run_dispatches_to_lookup() {
    assert_eq!(
        run(&argv(&["lookup"])).unwrap_err(),
        "usage: taiyaku lookup <taiyaku.json> <from> <to> <page>..."
    );
}

#[test]
fn run_dispatches_to_serve() {
    assert_eq!(
        run(&argv(&["serve"])).unwrap_err(),
        "needs at least one taiyaku.json to serve"
    );
}

#[test]
fn run_dispatches_to_duplicates() {
    assert_eq!(
        run(&argv(&["duplicates"])).unwrap_err(),
        "needs at least two copies, separated by `--`; one copy is not a duplicate set"
    );
}

#[test]
fn run_dispatches_to_inventory() {
    assert_eq!(
        run(&argv(&["inventory"])).unwrap_err(),
        "needs a directory to walk"
    );
}

#[test]
fn run_dispatches_to_credits() {
    let err = run(&argv(&["credits"])).unwrap_err();
    assert!(err.starts_with("taiyaku credits <member>..."), "{err}");
}

#[test]
fn run_dispatches_to_regions() {
    let err = run(&argv(&["regions"])).unwrap_err();
    assert!(err.starts_with("taiyaku regions <member>..."), "{err}");
}

#[test]
fn run_refuses_an_unknown_command() {
    assert_eq!(
        run(&argv(&["frobnicate"])).unwrap_err(),
        "unknown command `frobnicate`"
    );
}

/// No command at all prints usage rather than refusing — the one arm none
/// of the dispatch tests above reach, since every one of them names a
/// command.
#[test]
fn run_with_no_command_prints_usage_and_succeeds() {
    assert_eq!(run(&[]), Ok(()));
}

// -- lookup(): resolved entirely from a sidecar, so no archive is needed --

/// Write a sidecar fixture to its own temp directory and return its path,
/// as a `String` since every command here takes paths as `&str`.
fn write_fixture(name: &str, doc: &Sidecar) -> String {
    let dir = std::env::temp_dir().join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("taiyaku.json");
    std::fs::write(&path, doc.to_json()).expect("fixture written");
    path.to_str().expect("utf-8 temp path").to_string()
}

/// A star built to walk `lookup` through every `Via` it can report, and
/// both ways `route` comes back `None`: `broken` names an edge that is
/// there but cannot be read (a run pointing at a slot `broken` does not
/// have), and `island` is an edition with no edge to anything at all.
fn lookup_fixture() -> Sidecar {
    let text = r#"{
          "taiyaku": 1,
          "work": { "title": "Lookup Fixture", "reading": "rtl" },
          "editions": {
            "raw": {
              "label": "raw",
              "members": [{ "path": "r.cbz", "sha256": "r0", "pages": 8 }],
              "spreads": []
            },
            "en-fan": {
              "label": "en-fan",
              "members": [{ "path": "f.cbz", "sha256": "f0", "pages": 8 }],
              "spreads": []
            },
            "en-mono": {
              "label": "en-mono",
              "members": [{ "path": "m.cbz", "sha256": "m0", "pages": 8 }],
              "spreads": []
            },
            "broken": {
              "label": "broken",
              "members": [{ "path": "k.cbz", "sha256": "k0", "pages": 3 }],
              "spreads": []
            },
            "island": {
              "label": "island",
              "members": [{ "path": "i.cbz", "sha256": "i0", "pages": 3 }],
              "spreads": []
            }
          },
          "hub": "raw",
          "alignments": [
            { "from": "raw", "to": "en-fan", "by": "solver",
              "runs": [{"a":"0-7","b":"0-7"}] },
            { "from": "raw", "to": "en-mono", "by": "solver",
              "runs": [{"a":"0-7","b":"0-7"}] },
            { "from": "raw", "to": "broken", "by": "solver",
              "runs": [{"a":"0-7","b":"0-99"}] }
          ]
        }"#;
    Sidecar::from_json(text).expect("fixture parses")
}

/// Pinned against `route` directly, not just `lookup`'s `Result`:
/// `lookup` reports `Ok(())` whether it found a direct edge, an inverted
/// one, a hub composition, or nothing at all, so the `Result` alone
/// cannot tell this apart from any of those.
#[test]
fn lookup_reports_a_direct_edge() {
    let doc = lookup_fixture();
    let route = taiyaku_core::route(&doc, "raw", "en-fan").expect("a direct edge is stored");
    assert_eq!(route.via, Via::Direct);
    assert_eq!(
        route.resolve(0),
        Some(Slot {
            page: 0,
            half: None
        })
    );
    assert_eq!(
        route.resolve(7),
        Some(Slot {
            page: 7,
            half: None
        })
    );

    let path = write_fixture("taiyaku-test-lookup-direct", &doc);
    assert_eq!(lookup(&argv(&[&path, "raw", "en-fan", "0", "7"])), Ok(()));
}

/// The reverse of a stored edge, walked without a second edge ever being
/// written: `sidecar.md` stores N-1 edges and reads the (N-1)th one both
/// ways. Pinned against `route` so a `Via::Direct` masquerading as the
/// reverse — or a resolve that silently fell through to the hub — would
/// fail here rather than only showing up as a printed line nothing reads.
#[test]
fn lookup_reports_an_inverted_edge() {
    let doc = lookup_fixture();
    let route =
        taiyaku_core::route(&doc, "en-fan", "raw").expect("the raw -> en-fan edge reverses");
    assert_eq!(route.via, Via::Inverted);
    assert_eq!(
        route.resolve(0),
        Some(Slot {
            page: 0,
            half: None
        })
    );

    let path = write_fixture("taiyaku-test-lookup-inverted", &doc);
    assert_eq!(lookup(&argv(&[&path, "en-fan", "raw", "0"])), Ok(()));
}

/// Neither `en-fan` nor `en-mono` has a direct edge to the other; both
/// have one to the hub, so the route composes through it. Pinned against
/// `route` so a composition that quietly failed and left the pages
/// unresolved — indistinguishable from success through `lookup`'s
/// `Result` alone — would fail here.
#[test]
fn lookup_reports_a_hub_composed_route() {
    let doc = lookup_fixture();
    let route = taiyaku_core::route(&doc, "en-fan", "en-mono").expect("both spokes reach the hub");
    assert_eq!(route.via, Via::Hub("raw".to_string()));
    assert_eq!(
        route.resolve(0),
        Some(Slot {
            page: 0,
            half: None
        })
    );

    let path = write_fixture("taiyaku-test-lookup-hub", &doc);
    assert_eq!(lookup(&argv(&[&path, "en-fan", "en-mono", "0"])), Ok(()));
}

/// Pinned against `route` so an identity route that quietly resolved
/// nothing — or one that was reached only by a same-name coincidence
/// through the hub rather than `Via::Identity` — would fail here.
#[test]
fn lookup_reports_identity_for_the_same_edition() {
    let doc = lookup_fixture();
    let route = taiyaku_core::route(&doc, "raw", "raw").expect("an edition aligns to itself");
    assert_eq!(route.via, Via::Identity);
    assert_eq!(
        route.resolve(0),
        Some(Slot {
            page: 0,
            half: None
        })
    );

    let path = write_fixture("taiyaku-test-lookup-identity", &doc);
    assert_eq!(lookup(&argv(&[&path, "raw", "raw", "0"])), Ok(()));
}

/// **A stored edge is the whole answer, including when it cannot be
/// read** — `route`'s own doc comment. `raw -> broken` is stored, so this
/// must not fall back to composing through the hub and must not error
/// lookup as a whole. Pinned against `route` and `runs_of` directly,
/// since `lookup`'s `Result` cannot distinguish this from a hub fallback,
/// an island with nothing stored, or a clean direct edge — all of them
/// also return `Ok(())`.
#[test]
fn lookup_reports_an_unreadable_edge_rather_than_falling_back_to_the_hub() {
    let doc = lookup_fixture();
    assert_eq!(
        taiyaku_core::route(&doc, "raw", "broken"),
        None,
        "an edge that cannot be read must not fall back to the hub"
    );
    let alignment = doc
        .alignments
        .iter()
        .find(|a| a.from == "raw" && a.to == "broken")
        .expect("the raw -> broken edge is stored");
    assert!(
        doc.runs_of(alignment).is_err(),
        "the stored edge names a page the 3-page `broken` edition does not have"
    );

    let path = write_fixture("taiyaku-test-lookup-broken", &doc);
    assert_eq!(lookup(&argv(&[&path, "raw", "broken", "0"])), Ok(()));
}

/// `island` has no edge to anything, including the hub, so there is
/// nothing to compose through it either. The other `None` case above is
/// an edge that exists and fails to read; this is no edge existing at
/// all — pinned against `route` and the alignment list so the two
/// `None` cases cannot be silently swapped for one another.
#[test]
fn lookup_reports_that_an_unrelated_pair_has_nothing_stored() {
    let doc = lookup_fixture();
    assert_eq!(taiyaku_core::route(&doc, "raw", "island"), None);
    assert!(
        doc.alignments
            .iter()
            .all(|a| a.from != "island" && a.to != "island"),
        "island must have no stored edge to anything, unlike `broken`"
    );

    let path = write_fixture("taiyaku-test-lookup-island", &doc);
    assert_eq!(lookup(&argv(&[&path, "raw", "island", "0"])), Ok(()));
}

#[test]
fn lookup_refuses_a_page_that_does_not_parse() {
    let path = write_fixture("taiyaku-test-lookup-badpage", &lookup_fixture());
    let err = lookup(&argv(&[&path, "raw", "en-fan", "not-a-number"])).unwrap_err();
    assert_eq!(err, "not a page: not-a-number");
}

/// A two-slot spread, page 1 of a 3-page edition, present on both sides so
/// the whole thing is one paired run. Reading order decides which half is
/// found first by a bare page number — `RightToLeft` puts the right half
/// first, `LeftToRight` the left — so both are exercised rather than
/// whichever one the default direction happens to reach.
fn spread_lookup_fixture(reading: &str) -> Sidecar {
    let text = format!(
        r#"{{
              "taiyaku": 1,
              "work": {{ "title": "Spread Fixture", "reading": "{reading}" }},
              "editions": {{
                "raw": {{
                  "label": "raw",
                  "members": [{{ "path": "r.cbz", "sha256": "r0", "pages": 3 }}],
                  "spreads": [1]
                }},
                "en": {{
                  "label": "en",
                  "members": [{{ "path": "e.cbz", "sha256": "e0", "pages": 3 }}],
                  "spreads": [1]
                }}
              }},
              "hub": "raw",
              "alignments": [
                {{ "from": "raw", "to": "en", "by": "solver",
                  "runs": [{{"a":"0-2","b":"0-2"}}] }}
              ]
            }}"#
    );
    Sidecar::from_json(&text).expect("fixture parses")
}

/// Pinned against `route`/`resolve` directly, not just `lookup`'s
/// `Result`: a span written in page notation is easy to get subtly wrong
/// — `"0-3"` reads as "page 3", which does not exist on a 3-page edition,
/// rather than "all four slots" — and `assert_eq!(lookup(...), Ok(()))`
/// alone cannot tell that mistake apart from success, since an
/// unreadable edge still returns `Ok(())` having printed a warning
/// instead of the half. This is what caught exactly that mistake in this
/// fixture during development.
#[test]
fn lookup_names_the_right_half_of_a_spread_in_a_right_to_left_document() {
    let doc = spread_lookup_fixture("rtl");
    let route = taiyaku_core::route(&doc, "raw", "en").expect("a direct edge is stored");
    assert_eq!(
        route.resolve(1),
        Some(Slot {
            page: 1,
            half: Some(Half::Right)
        })
    );

    let path = write_fixture("taiyaku-test-lookup-spread-rtl", &doc);
    assert_eq!(lookup(&argv(&[&path, "raw", "en", "1"])), Ok(()));
}

#[test]
fn lookup_names_the_left_half_of_a_spread_in_a_left_to_right_document() {
    let doc = spread_lookup_fixture("ltr");
    let route = taiyaku_core::route(&doc, "raw", "en").expect("a direct edge is stored");
    assert_eq!(
        route.resolve(1),
        Some(Slot {
            page: 1,
            half: Some(Half::Left)
        })
    );

    let path = write_fixture("taiyaku-test-lookup-spread-ltr", &doc);
    assert_eq!(lookup(&argv(&[&path, "raw", "en", "1"])), Ok(()));
}

// -- align, verify, topology: the happy paths, which need real pages --
//
// Everything above stops short of `taiyaku_read::read_edition` actually
// succeeding: every fixture is a nonexistent path, because the property
// under test is argument handling, and real archives would only slow that
// down. The full pipeline — decode, fingerprint, solve, print, and for
// `align --write`, write a sidecar back — has had no test at all up to
// this point, so everything below builds a real, decodable `.cbz`.

/// Build a cbz of `count` real, decodable, and mutually distinguishable
/// pages.
///
/// A solid fill will not do here the way it does for `regions.rs`'s
/// `cbz_with_page`: `fingerprint::fingerprint` is a *difference* hash,
/// comparing each pixel to its neighbour, and every neighbour comparison
/// in a flat image is a tie — so every solid-colour page hashes to the
/// same all-zero fingerprint regardless of its level, and every alignment
/// over such a set would look like a perfect match by accident rather
/// than by the solver actually distinguishing anything. The pattern below
/// varies along both axes and by page and by `seed`, so distinct calls
/// produce distinct fingerprints.
fn cbz_pages(name: &str, count: u32, seed: u32) -> PathBuf {
    use std::io::Write;
    let path = std::env::temp_dir().join(name);
    let _ = std::fs::remove_file(&path);
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).expect("create cbz"));
    for i in 0..count {
        let level = seed.wrapping_add(i).wrapping_mul(101);
        let image = image::GrayImage::from_fn(8, 8, |x, y| {
            let v = x.wrapping_mul(37).wrapping_add(y.wrapping_mul(59)) + level;
            let byte = u8::try_from(v % 256)
                .expect("v % 256 is in 0..256 by construction, which is exactly u8's range");
            image::Luma([byte])
        });
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .expect("an 8x8 gray image always encodes");
        zip.start_file(
            format!("{i:03}.png"),
            zip::write::SimpleFileOptions::default(),
        )
        .expect("start entry");
        zip.write_all(&bytes).expect("write page bytes");
    }
    zip.finish().expect("finish cbz");
    path
}

/// The page-level diagnostic (`--no-split-spreads`), with `--stats` so
/// both `stats::calibration_line` and `stats::shape_line` run too, not
/// just the plain `report_summary` beneath them.
///
/// Everything this path computes is printed, not returned, so pin
/// against the same `taiyaku_read::read_edition` +
/// `fingerprint::fingerprint_edition` + `solve` chain `align` runs on
/// this exact fixture: `a` and `b` share a seed, so every page is
/// byte-identical to its counterpart and the correspondence is exact,
/// not approximate. A solve that lost a page, or paired pages out of
/// order, would still leave `align` returning `Ok(())`.
#[test]
fn align_runs_the_page_level_diagnostic_against_real_pages() {
    let a = cbz_pages("taiyaku-test-main-diag-a.cbz", 3, 1);
    let b = cbz_pages("taiyaku-test-main-diag-b.cbz", 3, 1);

    let a_pages = taiyaku_read::read_edition(std::slice::from_ref(&a)).expect("a decodes");
    let b_pages = taiyaku_read::read_edition(std::slice::from_ref(&b)).expect("b decodes");
    let a_hashes = fingerprint::fingerprint_edition(&a_pages).expect("a fingerprints");
    let b_hashes = fingerprint::fingerprint_edition(&b_pages).expect("b fingerprints");
    let runs = solve(&a_hashes, &b_hashes);
    for page in 0..3u32 {
        assert_eq!(
            counterpart(&runs, page),
            Some(page),
            "page {page} is identical on both sides and should pair with itself"
        );
    }

    let args = argv(&[
        a.to_str().unwrap(),
        "--",
        b.to_str().unwrap(),
        "--no-split-spreads",
        "--stats",
    ]);

    assert_eq!(align(&args), Ok(()));
}

/// The default, split-spreads path without `--write`: fingerprints
/// through `cached_edition`, solves, and prints the per-slot table.
/// Unequal page counts so at least one slot goes unmatched, reaching the
/// `None => gaps += 1` arm alongside the paired ones.
///
/// Everything this path computes is printed, not returned, so pin
/// against the same `cached_edition` + `solve` chain `align` runs, on a
/// cache directory of its own so this does not depend on `align`
/// having populated its cache first. `a` and `b` do not share a seed —
/// this is not a trivial identity pairing — and at these seeds two of
/// `a`'s four pages land on `b`'s two, with the other two the gaps the
/// doc comment above is about. A swapped or reordered pairing here
/// would still leave `align` returning `Ok(())`.
#[test]
fn align_runs_the_default_diagnostic_against_real_pages() {
    let a = cbz_pages("taiyaku-test-main-split-a.cbz", 4, 1);
    let b = cbz_pages("taiyaku-test-main-split-b.cbz", 2, 50);

    let probe_cache = std::env::temp_dir().join("taiyaku-test-main-split-cache-expected");
    let _ = std::fs::remove_dir_all(&probe_cache);
    let (a_hashes, _, _) = cached_edition(
        &probe_cache,
        std::slice::from_ref(&a),
        Direction::LeftToRight,
    )
    .expect("a fingerprints");
    let (b_hashes, _, _) = cached_edition(
        &probe_cache,
        std::slice::from_ref(&b),
        Direction::LeftToRight,
    )
    .expect("b fingerprints");
    let runs = solve(&a_hashes, &b_hashes);
    assert_eq!(
        counterpart(&runs, 0),
        None,
        "page 0 of a has no counterpart in the 2-page b"
    );
    assert_eq!(counterpart(&runs, 1), Some(0));
    assert_eq!(counterpart(&runs, 2), Some(1));
    assert_eq!(
        counterpart(&runs, 3),
        None,
        "page 3 of a has no counterpart in the 2-page b"
    );

    let cache = std::env::temp_dir().join("taiyaku-test-main-split-cache");
    let _ = std::fs::remove_dir_all(&cache);
    let args = argv(&[
        a.to_str().unwrap(),
        "--",
        b.to_str().unwrap(),
        "--cache",
        cache.to_str().unwrap(),
        "--stats",
        "--ltr",
    ]);

    assert_eq!(align(&args), Ok(()));
}

/// `--write` with no existing file at `out`: the `None` arm of the
/// existing/absent match, building a fresh two-edition `Sidecar` from
/// scratch. Asserted against the file `align` actually wrote, not just
/// the `Result` — `write_sidecar`'s own tests already cover truncation
/// safety, so what is worth pinning here is that `align` handed it the
/// document `docs/sidecar.md` describes: both editions, one alignment,
/// the title and hub this run asked for.
///
/// `b` shares `a`'s seed rather than using one of its own, so every page
/// matches its counterpart exactly: a real solve with no signal to speak
/// of would leave `solver: None` and every run's `margin: None`, which is
/// correct but exercises neither. A clean match is what makes calibration
/// worth accepting and every run fully paired, reaching both.
#[test]
fn align_write_creates_a_new_sidecar_from_real_pages() {
    let a = cbz_pages("taiyaku-test-main-write-a.cbz", 3, 1);
    let b = cbz_pages("taiyaku-test-main-write-b.cbz", 3, 1);
    let cache = std::env::temp_dir().join("taiyaku-test-main-write-cache");
    let _ = std::fs::remove_dir_all(&cache);
    let out_dir = std::env::temp_dir().join("taiyaku-test-main-write-out");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("temp dir");
    let out = out_dir.join("taiyaku.json");
    let args = argv(&[
        a.to_str().unwrap(),
        "--",
        b.to_str().unwrap(),
        "--cache",
        cache.to_str().unwrap(),
        "--write",
        out.to_str().unwrap(),
        "--title",
        "Test Work",
        "--name-a",
        "raw",
        "--name-b",
        "en",
    ]);

    assert_eq!(align(&args), Ok(()));

    let text = std::fs::read_to_string(&out).expect("sidecar written");
    let doc = Sidecar::from_json(&text).expect("written sidecar parses");
    assert_eq!(doc.work.title, "Test Work");
    assert_eq!(doc.hub, "raw");
    assert_eq!(doc.editions.len(), 2);
    assert!(doc.editions.contains_key("raw"));
    assert!(doc.editions.contains_key("en"));
    assert_eq!(doc.alignments.len(), 1);
    let alignment = &doc.alignments[0];
    assert_eq!(alignment.from, "raw");
    assert_eq!(alignment.to, "en");
    assert_eq!(
        alignment.runs.len(),
        1,
        "one clean run over identical pages"
    );
    assert!(
        alignment.runs[0].b.is_some(),
        "the run is paired, not a gap"
    );
    assert!(
        alignment.runs[0].margin.is_some(),
        "a paired run's weakest margin is recorded"
    );
    assert!(
        alignment.solver.is_some(),
        "a real match calibrates as accepted"
    );
}

/// Merging a second and then a third spoke into the document the test
/// above could have written, covering what a fresh write cannot: the
/// `Some(existing)` arm end to end, `--name-a` both given and derived
/// from the document's own hub, and both `Merged::Added` (`fr` is new)
/// and `Merged::Replaced` (`en` already exists) — `merge_spoke`'s own
/// tests cover the rule; what is worth pinning here is that `align`
/// reaches it with the right arguments in both shapes.
#[test]
fn align_write_merges_further_spokes_into_an_existing_sidecar() {
    let a = cbz_pages("taiyaku-test-main-merge-a.cbz", 3, 1);
    let b = cbz_pages("taiyaku-test-main-merge-b.cbz", 3, 60);
    let c = cbz_pages("taiyaku-test-main-merge-c.cbz", 3, 90);
    let cache = std::env::temp_dir().join("taiyaku-test-main-merge-cache");
    let _ = std::fs::remove_dir_all(&cache);
    let out_dir = std::env::temp_dir().join("taiyaku-test-main-merge-out");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("temp dir");
    let out = out_dir.join("taiyaku.json");

    // First write: establishes the document, hub `raw`.
    assert_eq!(
        align(&argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--cache",
            cache.to_str().unwrap(),
            "--write",
            out.to_str().unwrap(),
            "--name-a",
            "raw",
            "--name-b",
            "en",
        ])),
        Ok(())
    );

    // Second write: no --name-a, so it is read off the document's own
    // hub. `fr` does not exist yet, so this is `Merged::Added`.
    assert_eq!(
        align(&argv(&[
            a.to_str().unwrap(),
            "--",
            c.to_str().unwrap(),
            "--cache",
            cache.to_str().unwrap(),
            "--write",
            out.to_str().unwrap(),
            "--name-b",
            "fr",
        ])),
        Ok(())
    );

    // Third write: --name-a given explicitly and it agrees with the hub,
    // re-merging `en`, which already exists — `Merged::Replaced`.
    assert_eq!(
        align(&argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--cache",
            cache.to_str().unwrap(),
            "--write",
            out.to_str().unwrap(),
            "--name-a",
            "raw",
            "--name-b",
            "en",
        ])),
        Ok(())
    );

    let text = std::fs::read_to_string(&out).expect("sidecar written");
    let doc = Sidecar::from_json(&text).expect("written sidecar parses");
    assert_eq!(doc.editions.len(), 3, "raw, en and fr all present");
    assert!(doc.editions.contains_key("fr"));
    assert_eq!(doc.alignments.len(), 2, "raw->en replaced, not duplicated");
}

/// `--direct` on a merge has no hub to fall back on, so it requires
/// `--name-a` — the one arm of that `match` the tests above never reach,
/// since both give an existing document but never `--direct` without a
/// name.
#[test]
fn align_write_refuses_direct_without_name_a_on_a_merge() {
    let a = cbz_pages("taiyaku-test-main-direct-a.cbz", 3, 1);
    let b = cbz_pages("taiyaku-test-main-direct-b.cbz", 3, 60);
    let cache = std::env::temp_dir().join("taiyaku-test-main-direct-cache");
    let _ = std::fs::remove_dir_all(&cache);
    let out_dir = std::env::temp_dir().join("taiyaku-test-main-direct-out");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("temp dir");
    let out = out_dir.join("taiyaku.json");
    assert_eq!(
        align(&argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--cache",
            cache.to_str().unwrap(),
            "--write",
            out.to_str().unwrap(),
            "--name-a",
            "raw",
            "--name-b",
            "en",
        ])),
        Ok(())
    );

    let err = align(&argv(&[
        a.to_str().unwrap(),
        "--",
        b.to_str().unwrap(),
        "--cache",
        cache.to_str().unwrap(),
        "--write",
        out.to_str().unwrap(),
        "--direct",
        "--name-b",
        "fr",
    ]))
    .unwrap_err();
    assert_eq!(
        err,
        "--direct needs --name-a to say which edition the edge starts from"
    );
}

#[test]
fn align_write_refuses_both_editions_named_the_same() {
    let a = cbz_pages("taiyaku-test-main-clash-a.cbz", 3, 1);
    let b = cbz_pages("taiyaku-test-main-clash-b.cbz", 3, 60);
    let out = std::env::temp_dir().join("taiyaku-test-main-clash.json");
    let _ = std::fs::remove_file(&out);

    let err = align(&argv(&[
        a.to_str().unwrap(),
        "--",
        b.to_str().unwrap(),
        "--write",
        out.to_str().unwrap(),
        "--name-a",
        "same",
        "--name-b",
        "same",
    ]))
    .unwrap_err();

    assert_eq!(
        err,
        "both editions would be named `same`; pass --name-a and --name-b"
    );
}

/// A merge cannot silently rename the work it is joining — `--title` on
/// an existing document is only honoured when it agrees.
#[test]
fn align_write_refuses_to_rename_a_work_on_a_merge() {
    let a = cbz_pages("taiyaku-test-main-retitle-a.cbz", 3, 1);
    let b = cbz_pages("taiyaku-test-main-retitle-b.cbz", 3, 60);
    let out_dir = std::env::temp_dir().join("taiyaku-test-main-retitle-out");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("temp dir");
    let out = out_dir.join("taiyaku.json");
    assert_eq!(
        align(&argv(&[
            a.to_str().unwrap(),
            "--",
            b.to_str().unwrap(),
            "--write",
            out.to_str().unwrap(),
            "--title",
            "Original Title",
            "--name-a",
            "raw",
            "--name-b",
            "en",
        ])),
        Ok(())
    );

    let err = align(&argv(&[
        a.to_str().unwrap(),
        "--",
        b.to_str().unwrap(),
        "--write",
        out.to_str().unwrap(),
        "--title",
        "Different Title",
        "--name-b",
        "en",
    ]))
    .unwrap_err();

    assert_eq!(
        err,
        format!(
            "{} is the sidecar for `Original Title`; --title cannot rename a work on a merge",
            out.to_str().unwrap()
        )
    );
}

/// `verify` with no `--sidecar`: solves the pair directly and writes the
/// HTML report. `report::render`'s own tests cover the markup; what is
/// worth pinning at this level is that `verify` actually reaches it and
/// leaves a non-empty file at `--out`.
#[test]
fn verify_writes_a_report_for_a_direct_solve() {
    let a = cbz_pages("taiyaku-test-main-verify-a.cbz", 3, 1);
    let b = cbz_pages("taiyaku-test-main-verify-b.cbz", 3, 60);
    let out_dir = std::env::temp_dir().join("taiyaku-test-main-verify-out");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).expect("temp dir");
    let out = out_dir.join("report.html");
    let args = argv(&[
        a.to_str().unwrap(),
        "--",
        b.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);

    assert_eq!(verify(&args), Ok(()));

    let html = std::fs::read_to_string(&out).expect("report written");
    assert!(!html.is_empty(), "a real report was written");
}

/// `verify --sidecar`: the branch that reads runs from a file rather than
/// solving, indexes by slot rather than page, and maps slot-confidence
/// back to page numbers for the report. The sidecar's `sha256` entries
/// are the real digests of the real archives below, computed the same way
/// `verify`'s own `named` closure computes them — a placeholder digest
/// would fail `edition_of` before any of the rest of this ran.
#[test]
fn verify_reports_confidence_through_a_stored_sidecar() {
    let a = cbz_pages("taiyaku-test-main-verify-sc-a.cbz", 3, 1);
    let b = cbz_pages("taiyaku-test-main-verify-sc-b.cbz", 3, 60);
    let a_digest = taiyaku_read::member_digest(&a).expect("digest a");
    let b_digest = taiyaku_read::member_digest(&b).expect("digest b");
    let doc_text = format!(
        r#"{{
              "taiyaku": 1,
              "work": {{ "title": "Verify Fixture", "reading": "rtl" }},
              "editions": {{
                "raw": {{ "label": "raw",
                  "members": [{{ "path": "a.cbz", "sha256": "{a_digest}", "pages": 3 }}],
                  "spreads": [] }},
                "en": {{ "label": "en",
                  "members": [{{ "path": "b.cbz", "sha256": "{b_digest}", "pages": 3 }}],
                  "spreads": [] }}
              }},
              "hub": "raw",
              "alignments": [
                {{ "from": "raw", "to": "en", "by": "solver",
                  "runs": [{{"a":"0-2","b":"0-2"}}] }}
              ]
            }}"#
    );
    let dir = std::env::temp_dir().join("taiyaku-test-main-verify-sidecar");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let sidecar_path = dir.join("taiyaku.json");
    std::fs::write(&sidecar_path, &doc_text).expect("fixture written");

    let out = dir.join("report.html");
    let cache = dir.join("cache");
    let args = argv(&[
        a.to_str().unwrap(),
        "--",
        b.to_str().unwrap(),
        "--sidecar",
        sidecar_path.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--cache",
        cache.to_str().unwrap(),
    ]);

    assert_eq!(verify(&args), Ok(()));
    assert!(out.exists(), "report written through the sidecar path");
}

/// The whole star: hub-to-spoke and spoke-to-spoke, direct against
/// composed, printed and totalled. `merge_spoke`, `compose` and `verdict`
/// each have their own unit tests in `taiyaku-core`; what only exists
/// here is `topology` actually wiring them together over real pages.
///
/// `topology` only prints — nothing it computes reaches the `Result` —
/// and capturing stdout would need a dependency this crate does not
/// have, so this pins against the same `fingerprint_edition`, `solve`
/// and `compose` chain `topology` runs, on this exact fixture, instead.
/// At these seeds `spoke2 -> hub` is a real calibration refusal (a total
/// gap) while the direct `spoke2 -> spoke1` solve is a clean match — so
/// `compose(to_hub[spoke2], from_hub[spoke1])`, the composed leg
/// `topology` builds for that pair, must come back strictly weaker than
/// the direct solve it is measured against. That gap is exactly what the
/// `loss` column `topology` prints counts. A composition that silently
/// fell back to the direct answer, or a hub/spoke argument swapped in
/// `topology`'s own `solve` calls, would still return `Ok(())` and would
/// still fail this.
#[test]
fn topology_reports_the_star_against_two_spokes() {
    let hub = cbz_pages("taiyaku-test-main-topology-hub.cbz", 3, 10);
    let spoke1 = cbz_pages("taiyaku-test-main-topology-s1.cbz", 3, 20);
    let spoke2 = cbz_pages("taiyaku-test-main-topology-s2.cbz", 3, 30);

    let hub_pages = taiyaku_read::read_edition(std::slice::from_ref(&hub)).expect("hub decodes");
    let s1_pages =
        taiyaku_read::read_edition(std::slice::from_ref(&spoke1)).expect("spoke1 decodes");
    let s2_pages =
        taiyaku_read::read_edition(std::slice::from_ref(&spoke2)).expect("spoke2 decodes");
    let hub_hashes = fingerprint::fingerprint_edition(&hub_pages).expect("hub fingerprints");
    let s1_hashes = fingerprint::fingerprint_edition(&s1_pages).expect("spoke1 fingerprints");
    let s2_hashes = fingerprint::fingerprint_edition(&s2_pages).expect("spoke2 fingerprints");

    let to_hub_2 = solve(&s2_hashes, &hub_hashes);
    let from_hub_1 = solve(&hub_hashes, &s1_hashes);
    let direct_2_1 = solve(&s2_hashes, &s1_hashes);
    let composed_2_1 = compose(&to_hub_2, &from_hub_1);

    for page in 0..3u32 {
        assert_eq!(
            counterpart(&direct_2_1, page),
            Some(page),
            "spoke2 and spoke1 solve directly as a clean, page-for-page match at these seeds"
        );
        assert_eq!(
            counterpart(&to_hub_2, page),
            None,
            "spoke2 -> hub is a calibration refusal at these seeds, not a match"
        );
        assert_eq!(
            counterpart(&composed_2_1, page),
            None,
            "composed through the hub must not recover a pairing the hub leg refused"
        );
    }

    let args = argv(&[
        hub.to_str().unwrap(),
        "--",
        spoke1.to_str().unwrap(),
        "--",
        spoke2.to_str().unwrap(),
    ]);

    assert_eq!(topology(&args), Ok(()));
}

/// The relative path is `.` exactly when `target` and `base` canonicalise
/// to the same directory — the empty-components case `PathBuf` cannot
/// represent any other way, since `PathBuf::from("")` is not a usable
/// path segment.
#[test]
fn relative_to_is_current_directory_when_target_and_base_are_the_same() {
    let dir = std::env::temp_dir().join("taiyaku-test-relative-same");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");

    let recorded = relative_to(&dir, &dir);

    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(recorded, Ok(PathBuf::from(".")));
}

// -- verify(): the sidecar arm's remaining branches --

/// A sidecar this specific pair is not an edition of: `--sidecar` names a
/// real document, but the members given on the command line hash to
/// nothing it defines. `named`'s closure is what catches this, and it has
/// to report which side.
#[test]
fn verify_refuses_members_that_match_no_edition_of_the_sidecar() {
    let a = cbz_pages("taiyaku-test-main-verify-mismatch-a.cbz", 3, 1);
    let b = cbz_pages("taiyaku-test-main-verify-mismatch-b.cbz", 3, 60);
    let unrelated = cbz_pages("taiyaku-test-main-verify-mismatch-x.cbz", 3, 200);
    let a_digest = taiyaku_read::member_digest(&a).expect("digest a");
    let b_digest = taiyaku_read::member_digest(&b).expect("digest b");
    let doc_text = format!(
        r#"{{
              "taiyaku": 1,
              "work": {{ "title": "Mismatch Fixture", "reading": "rtl" }},
              "editions": {{
                "raw": {{ "label": "raw",
                  "members": [{{ "path": "a.cbz", "sha256": "{a_digest}", "pages": 3 }}],
                  "spreads": [] }},
                "en": {{ "label": "en",
                  "members": [{{ "path": "b.cbz", "sha256": "{b_digest}", "pages": 3 }}],
                  "spreads": [] }}
              }},
              "hub": "raw",
              "alignments": [
                {{ "from": "raw", "to": "en", "by": "solver",
                  "runs": [{{"a":"0-2","b":"0-2"}}] }}
              ]
            }}"#
    );
    let dir = std::env::temp_dir().join("taiyaku-test-main-verify-mismatch");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let sidecar_path = dir.join("taiyaku.json");
    std::fs::write(&sidecar_path, &doc_text).expect("fixture written");

    let args = argv(&[
        unrelated.to_str().unwrap(),
        "--",
        b.to_str().unwrap(),
        "--sidecar",
        sidecar_path.to_str().unwrap(),
    ]);

    let err = verify(&args).unwrap_err();
    assert!(
        err.contains("the a-side members are not an edition of"),
        "{err}"
    );
}

/// The same fixture read the other way round: `raw -> en` is the stored
/// edge, so asking for `en`'s members as the a-side and `raw`'s as the
/// b-side reads it inverted rather than direct — the `--ltr` on this test
/// also covers `Direction::LeftToRight`, which nothing else here reaches
/// since every other sidecar test leaves it at the default.
#[test]
fn verify_reports_confidence_through_an_inverted_edge() {
    let a = cbz_pages("taiyaku-test-main-verify-inv-a.cbz", 3, 1);
    let b = cbz_pages("taiyaku-test-main-verify-inv-b.cbz", 3, 60);
    let a_digest = taiyaku_read::member_digest(&a).expect("digest a");
    let b_digest = taiyaku_read::member_digest(&b).expect("digest b");
    let doc_text = format!(
        r#"{{
              "taiyaku": 1,
              "work": {{ "title": "Inverted Fixture", "reading": "rtl" }},
              "editions": {{
                "raw": {{ "label": "raw",
                  "members": [{{ "path": "a.cbz", "sha256": "{a_digest}", "pages": 3 }}],
                  "spreads": [] }},
                "en": {{ "label": "en",
                  "members": [{{ "path": "b.cbz", "sha256": "{b_digest}", "pages": 3 }}],
                  "spreads": [] }}
              }},
              "hub": "raw",
              "alignments": [
                {{ "from": "raw", "to": "en", "by": "solver",
                  "runs": [{{"a":"0-2","b":"0-2"}}] }}
              ]
            }}"#
    );
    let dir = std::env::temp_dir().join("taiyaku-test-main-verify-inverted");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let sidecar_path = dir.join("taiyaku.json");
    std::fs::write(&sidecar_path, &doc_text).expect("fixture written");

    let out = dir.join("report.html");
    let cache = dir.join("cache");
    // b's real members on the a-side, a's on the b-side: the reverse of
    // the stored `raw -> en` edge.
    let args = argv(&[
        b.to_str().unwrap(),
        "--",
        a.to_str().unwrap(),
        "--sidecar",
        sidecar_path.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--cache",
        cache.to_str().unwrap(),
        "--ltr",
    ]);

    assert_eq!(verify(&args), Ok(()));
    assert!(out.exists(), "report written through the inverted edge");
}

// -- serve(): argument parsing only. Actually starting the server would
// block this test forever, so every one of these stops at the first
// thing that fails before a socket is ever bound: `library::canonical_sidecar_path`
// refusing a sidecar that does not exist, before the library is ever opened
// (Task 15) — the same failure point `load_works` used to be, moved one
// step earlier so a mistyped path still fails without touching a database.

#[test]
fn serve_refuses_an_unknown_flag() {
    let err = serve(&argv(&["--bogus"])).unwrap_err();
    assert_eq!(
        err,
        "serve has no flag `--bogus`; its flags are --addr, --cache, --db, --log-requests, \
             --refresh-stump, --stump-key, --stump-url"
    );
}

#[test]
fn serve_refuses_a_stump_url_with_no_key() {
    let err = serve(&argv(&["a.json", "--stump-url", "https://example.test"])).unwrap_err();
    assert_eq!(
        err,
        "--stump-url needs a key: --stump-key, or TAIYAKU_STUMP_KEY"
    );
}

#[test]
fn serve_refuses_a_stump_key_with_no_url() {
    let err = serve(&argv(&["a.json", "--stump-key", "secret"])).unwrap_err();
    assert_eq!(
        err,
        "a stump key was given with no --stump-url or TAIYAKU_STUMP_URL"
    );
}

/// Past every argument-parsing branch — both stump flags together,
/// `--log-requests`, `--refresh-stump` — and then failing on the sidecar,
/// which does not exist, rather than ever reaching `tokio::runtime` and
/// blocking this test on a server that never stops.
#[test]
fn serve_reaches_load_works_with_every_flag_resolved() {
    let log = std::env::temp_dir().join("taiyaku-test-serve-log.jsonl");
    let args = argv(&[
        "does-not-exist.json",
        "--addr",
        "127.0.0.1:0",
        "--stump-url",
        "https://example.test",
        "--stump-key",
        "secret",
        "--log-requests",
        log.to_str().unwrap(),
        "--refresh-stump",
    ]);

    let err = serve(&args).unwrap_err();
    assert!(err.contains("does-not-exist.json"), "{err}");
}

/// Neither `--stump-url` nor `--stump-key` given, and neither environment
/// variable set: the fourth arm of the `(base, key)` match, `stump`
/// staying `None` rather than being configured at all — the ordinary case
/// for a reader with no Stump instance, which every other serve test here
/// bypasses by giving both flags.
#[test]
fn serve_reaches_load_works_with_no_stump_configured() {
    let err = serve(&argv(&["does-not-exist.json"])).unwrap_err();
    assert!(err.contains("does-not-exist.json"), "{err}");
}

/// A fixture sidecar for `serve_works`'s own tests, under a scratch directory
/// named after `tag` — this file's own convention (see e.g. `cbz_pages`
/// above) for a temp path nothing else is using. `content_key` hashes the
/// member's *declared* `sha256`, not its actual bytes, so no archive needs to
/// exist on disk.
fn serve_works_fixture(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("taiyaku-test-serve-works-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture directory");
    let path = dir.join("taiyaku.json");
    std::fs::write(
        &path,
        r#"{"taiyaku":1,"work":{"title":"Serve Works Fixture","reading":"rtl"},
             "editions":{"raw":{"label":"R","members":[
               {"path":"raw/v01.cbz","sha256":"ef56","pages":10}]}},
             "hub":"raw","alignments":[]}"#,
    )
    .expect("writing the fixture");
    path
}

/// `serve_works` is `serve`'s own database-and-import half, split out
/// (see its doc comment) precisely so this — and the dedup test below —
/// can drive it directly rather than through `serve` itself, which would
/// bind a port and block the test forever the moment this much succeeds.
///
/// This is also where the placeholder-path hazard's fix actually runs in
/// production: every path `serve` is handed reaches
/// `library::canonical_sidecar_path` and `library::import_canonical` through
/// here, so a `Work` this function builds carries the library's *real* slug
/// (`unique_slug`, from the title) rather than the filename-derived slug
/// `taiyaku_serve::work_id` would have given it — which is what makes
/// `taiyaku-serve::works::row_id_for_write`'s `find_row` (keyed on
/// `work.id`) hit the row this same import just wrote, on the very first
/// write a reader makes, rather than minting a second, placeholder one.
#[test]
fn serve_works_imports_the_given_path_and_the_work_carries_the_librarys_slug() {
    let path = serve_works_fixture("single");
    let db = std::env::temp_dir().join("taiyaku-test-serve-works-single.db");
    let _ = std::fs::remove_file(&db);

    let (library, works) =
        serve_works(std::slice::from_ref(&path), Some(db.to_str().unwrap())).expect("builds works");

    assert_eq!(works.len(), 1, "one path, one work");
    // `slugify("Serve Works Fixture")`: lowercased, non-alphanumerics to
    // `-`, trimmed — not `works::work_id(&path)`, which would slugify the
    // *filename* `taiyaku` instead. The two are unrelated on purpose (see
    // `library.rs`'s doc comment on the hazard); this pins that `serve_works`
    // uses the library's title-derived one.
    assert_eq!(works[0].id, "serve-works-fixture");
    assert_eq!(works[0].sidecar().editions.len(), 1);

    // And the row really is in the database this call opened, not just in
    // the in-memory `Work` — the property `taiyaku library import` and a
    // later `serve` of the same file both rely on.
    let conn = library.conn();
    let slug: String = conn
        .query_row("SELECT slug FROM work", [], |row| row.get(0))
        .expect("one work row exists");
    assert_eq!(slug, "serve-works-fixture");
}

/// The same file, named twice on one command line — a repeated argument, or
/// two spellings of one path — must not become two independent `Work`s
/// sharing one library row: `Work` is deliberately not `Clone`, precisely
/// because two of them over one document could correct it twice and see two
/// different answers. This is the `if imported.contains(&work_id) { continue; }`
/// branch in `serve_works`.
#[test]
fn serve_works_serves_one_work_when_the_same_path_is_given_twice() {
    let path = serve_works_fixture("repeated");
    let db = std::env::temp_dir().join("taiyaku-test-serve-works-repeated.db");
    let _ = std::fs::remove_file(&db);

    let (_library, works) = serve_works(&[path.clone(), path.clone()], Some(db.to_str().unwrap()))
        .expect("builds works");

    assert_eq!(
        works.len(),
        1,
        "the same path twice must not mint two Works over one row"
    );
}

/// The skip above is announced, and the sentence names **both** paths. Two
/// *different* files can import to one work — that is what matching on
/// content means, and a copy of a library under a second path reaches it —
/// so "one of your arguments was dropped" without saying which leaves a
/// reader unable to tell which file the server is reading. Asserted on the
/// sentence rather than through captured stderr, the way
/// `taiyaku-serve`'s `skipping_notice` is.
#[test]
fn the_duplicate_work_notice_names_both_paths() {
    let notice = already_served_notice(
        Path::new("/corpus/copy/taiyaku.json"),
        Path::new("/corpus/original/taiyaku.json"),
    );
    assert!(
        notice.contains("/corpus/copy/taiyaku.json"),
        "the dropped path: {notice}"
    );
    assert!(
        notice.contains("/corpus/original/taiyaku.json"),
        "and the one it is already being served as: {notice}"
    );
}

/// A sidecar at `<tag>/taiyaku.json` that cannot be imported: valid JSON,
/// parses as a `Sidecar`, and names a `hub` no edition in it answers to — so
/// it fails in `identity::content_key`, the way a hand edit that renames an
/// edition and forgets the hub does. Written as a separate fixture rather
/// than by corrupting `serve_works_fixture`'s, since the point of the tests
/// below is that a bad document sits *beside* a good one.
fn unimportable_fixture(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("taiyaku-test-serve-works-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture directory");
    let path = dir.join("taiyaku.json");
    std::fs::write(
        &path,
        r#"{"taiyaku":1,"work":{"title":"Unimportable","reading":"rtl"},
             "editions":{"raw":{"label":"R","members":[
               {"path":"raw/v01.cbz","sha256":"bad1","pages":10}]}},
             "hub":"renamed-and-not-updated","alignments":[]}"#,
    )
    .expect("writing the fixture");
    path
}

/// **One work that cannot be imported must not stop the library from
/// serving.**
///
/// `serve_works` propagated its import failure with `?` from inside the loop,
/// so a single unreadable or structurally broken `taiyaku.json` aborted
/// `taiyaku serve` for every other sidecar on the command line — the reader
/// lost their whole library to one hand edit, with no way to start until the
/// file was found and fixed. Whole-branch review 2, I3.
///
/// The bad path is named on stderr and skipped; every other work is served.
/// Asserted on the payload — which works came back, by id — rather than on
/// the call merely returning `Ok`.
#[test]
fn serve_works_skips_a_work_it_cannot_import_and_serves_the_rest() {
    let good = serve_works_fixture("survivor");
    let bad = unimportable_fixture("survivor-bad");
    let db = std::env::temp_dir().join("taiyaku-test-serve-works-survivor.db");
    let _ = std::fs::remove_file(&db);

    let (_library, works) = serve_works(&[bad.clone(), good.clone()], Some(db.to_str().unwrap()))
        .expect("one unimportable work must not abort the start");

    let ids: Vec<&str> = works.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["serve-works-fixture"],
        "the good work is served and the bad one is skipped"
    );
}

/// **Two works whose sidecars share a filename are both served.**
/// `taiyaku.json` beside each work is this project's documented convention, so
/// `taiyaku_serve::work_id` — which slugifies the *filename* — gives every
/// work in a real library the same id, and `load_works` refuses the pair
/// outright rather than shadowing one. Off the database it is not a collision
/// at all: the id is `work.slug`, minted from the title by `unique_slug`, and
/// the two coexist.
///
/// The production form of `taiyaku-serve`'s
/// `a_library_serves_many_works_where_load_works_refused_two` — same claim,
/// through the paths a person actually types, and the reason
/// `works::from_library` could be deleted without the property going with it.
#[test]
fn serve_works_serves_two_sidecars_both_named_taiyaku_json() {
    let first = serve_works_fixture("shared-name-a");
    let second = std::env::temp_dir().join("taiyaku-test-serve-works-shared-name-b");
    let _ = std::fs::remove_dir_all(&second);
    std::fs::create_dir_all(&second).expect("fixture directory");
    let second = second.join("taiyaku.json");
    // A different title *and* a different hub member digest: the title is what
    // `unique_slug` derives the id from, and the digest is what `content_key`
    // hashes — share the digest and the second import lands on the first
    // work's row instead of a second one.
    std::fs::write(
        &second,
        r#"{"taiyaku":1,"work":{"title":"Second Fixture","reading":"rtl"},
             "editions":{"raw":{"label":"R","members":[
               {"path":"raw/v01.cbz","sha256":"ab12","pages":10}]}},
             "hub":"raw","alignments":[]}"#,
    )
    .expect("writing the second fixture");

    let db = std::env::temp_dir().join("taiyaku-test-serve-works-shared-name.db");
    let _ = std::fs::remove_file(&db);

    let (_library, works) =
        serve_works(&[first.clone(), second.clone()], Some(db.to_str().unwrap()))
            .expect("two sidecars sharing a filename must both be served");

    let ids: Vec<&str> = works.iter().map(|w| w.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["serve-works-fixture", "second-fixture"],
        "both works are served, each under the library's title-derived slug \
         rather than the filename they share"
    );
    assert_eq!(
        taiyaku_serve::work_id(&first),
        taiyaku_serve::work_id(&second),
        "and the filename-derived id really would have collided, which is what \
         load_works refuses"
    );
}

/// The other end of the same branch: if *nothing* could be imported there is
/// no library to serve, so this refuses rather than binding a port and
/// serving an empty shelf. The message names how many failed, and each
/// failure has already been named on stderr.
#[test]
fn serve_works_refuses_when_no_work_could_be_imported() {
    let bad = unimportable_fixture("all-bad");
    let db = std::env::temp_dir().join("taiyaku-test-serve-works-all-bad.db");
    let _ = std::fs::remove_file(&db);

    let err = serve_works(std::slice::from_ref(&bad), Some(db.to_str().unwrap()))
        .expect_err("nothing to serve must be an error");

    assert!(
        err.contains("no work could be served"),
        "the refusal says what happened: {err}"
    );
    assert!(err.contains('1'), "and how many sidecars failed: {err}");
}

/// **C1b, end to end through the real `serve` startup path.** Adding a
/// volume to the hub edition and re-solving is the ordinary case for an
/// ongoing series. Before the fix, `reimport`'s only write was the database's
/// own stale projection, so `merge_spoke`'s from-side guard refused every
/// spoke merge — the held hub had one member, the freshly solved one had two
/// — and that refusal propagated all the way out of `serve_works` as an
/// `Err`, which is what made the real `taiyaku serve` binary exit 1 telling a
/// reader to re-solve the work they had just re-solved. This drives
/// `serve_works` directly (see its own doc comment: the split exists
/// precisely so a test can reach the database-and-import half without
/// binding a port), so it is the same code path `serve` itself runs, minus
/// the `tokio::runtime` and the socket.
#[test]
fn serve_works_survives_a_hub_that_gained_a_member() {
    let dir = std::env::temp_dir().join("taiyaku-test-serve-works-hub-grows");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("fixture directory");
    let path = dir.join("taiyaku.json");
    std::fs::write(
        &path,
        r#"{"taiyaku":1,"work":{"title":"Hub Grows","reading":"rtl"},
             "editions":{
               "raw":{"label":"R","members":[
                 {"path":"raw/v01.cbz","sha256":"hh1","pages":10}]},
               "en":{"label":"En","members":[
                 {"path":"en/v01.cbz","sha256":"ee1","pages":10}]}
             },
             "hub":"raw",
             "alignments":[{"from":"raw","to":"en","by":"solver",
               "runs":[{"a":"0-9","b":"0-9"}]}]}"#,
    )
    .expect("writing the fixture");
    let db = std::env::temp_dir().join("taiyaku-test-serve-works-hub-grows.db");
    let _ = std::fs::remove_file(&db);

    let (library, works) =
        serve_works(std::slice::from_ref(&path), Some(db.to_str().unwrap())).expect("first start");
    assert_eq!(works.len(), 1);
    assert_eq!(works[0].sidecar().editions["raw"].members.len(), 1);
    // Explicitly dropped before the second `serve_works` call reopens the
    // same database file — `library/tests.rs`'s own idiom for reusing a db
    // path within one test.
    drop(library);
    drop(works);

    // The hub gains a volume and the work is re-solved — same file, same
    // path, a real content_key change underneath it.
    std::fs::write(
        &path,
        r#"{"taiyaku":1,"work":{"title":"Hub Grows","reading":"rtl"},
             "editions":{
               "raw":{"label":"R","members":[
                 {"path":"raw/v01.cbz","sha256":"hh1","pages":10},
                 {"path":"raw/v02.cbz","sha256":"hh2","pages":10}]},
               "en":{"label":"En","members":[
                 {"path":"en/v01.cbz","sha256":"ee1","pages":10}]}
             },
             "hub":"raw",
             "alignments":[{"from":"raw","to":"en","by":"solver",
               "runs":[{"a":"0-9","b":"0-9"}]}]}"#,
    )
    .expect("re-solving over the same fixture");

    let (library, works) = serve_works(std::slice::from_ref(&path), Some(db.to_str().unwrap()))
        .expect("a hub that gained a member must not fail serve's startup");
    assert_eq!(works.len(), 1, "the same work, not a second one");
    assert_eq!(
        works[0].sidecar().editions["raw"].members.len(),
        2,
        "the re-solved hub — the incoming document's, not the stale projection's — is what serve now holds"
    );

    let conn = library.conn();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM work", [], |row| row.get(0))
        .expect("count");
    assert_eq!(count, 1, "no second work was minted");
}

/// `--write` reading an existing `out` that fails for a reason other than
/// "not found" — here, `out` is itself a directory — reports that error
/// rather than reading it as an absent file and trying to create a fresh
/// document over it.
#[test]
fn align_write_reports_a_read_error_other_than_not_found() {
    let a = cbz_pages("taiyaku-test-main-readerr-a.cbz", 3, 1);
    let b = cbz_pages("taiyaku-test-main-readerr-b.cbz", 3, 60);
    let out = std::env::temp_dir().join("taiyaku-test-main-readerr-out");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).expect("out exists as a directory, not a file");

    let err = align(&argv(&[
        a.to_str().unwrap(),
        "--",
        b.to_str().unwrap(),
        "--write",
        out.to_str().unwrap(),
        "--name-a",
        "raw",
        "--name-b",
        "en",
    ]))
    .unwrap_err();

    let _ = std::fs::remove_dir_all(&out);
    assert!(
        err.starts_with(&format!("{}: ", out.to_str().unwrap())),
        "{err}"
    );
}

// `main` itself is not tested here, and cannot usefully be: it calls
// `std::process::exit`, which would tear down this whole test binary
// along with every other test running in it, so calling it in-process is
// not an option. The standard escape hatch — `env!("CARGO_BIN_EXE_...")`,
// running the real compiled binary as a subprocess — does not apply
// either: that variable is set by Cargo for a package's *other* test
// targets, not for a `[[bin]]`'s own unit tests, since building this test
// binary and building the `taiyaku` binary it would need to invoke is the
// same `cargo build` step. `main` is four lines that unpack `argv`, call
// `run`, and translate `Err` into a message on stderr and exit code 1;
// every one of those four lines is exercised in `run`'s own tests above,
// and `run` is the whole of what `main` delegates to.
