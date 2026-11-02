use super::*;
use taiyaku_core::PageRange;

/// Two copies of one scan: every slot paired on both sides.
fn whole(slots: u32) -> Vec<Run> {
    vec![Run::paired(
        PageRange::new(0, slots - 1),
        PageRange::new(0, slots - 1),
    )]
}

/// Identity is mutual full coverage. It is necessarily one run, because
/// paired runs are 1:1 and an offset that never changes cannot start a
/// second run — so the test states coverage and asserts the run count as a
/// consequence rather than as the definition.
#[test]
fn full_coverage_on_both_sides_is_identity() {
    let pair = Pair::judge(0, 1, &whole(234), 234, 234);

    assert_eq!(pair.relation, Relation::Identity);
    assert_eq!(pair.runs, 1);
    assert_eq!(pair.paired, 234);
}

/// The distinction base selection turns on. Chainsaw Man's `v01` against
/// `v01b` covers all 198 of A and leaves a 199th page on B that A never
/// reaches — so `a` is wholly within `b`, adding nothing `b` lacks, and
/// `v01b` goes on to win the rank on that extra page. Collapsing them
/// would discard the difference the choice is about.
#[test]
fn one_slot_left_over_on_the_far_side_is_not_identity() {
    let pair = Pair::judge(0, 1, &whole(198), 198, 199);

    assert_eq!(pair.relation, Relation::Within);
    assert_eq!(pair.paired, 198);
}

/// The mirror shape: Bocchi's hamaji copy against krc, 125 slots against
/// 124 with the 124 shared ones all paired. Here it is the `a` side that
/// carries the extra slot, so `b` is within `a`.
#[test]
fn extra_slot_on_the_near_side_is_containment_the_other_way() {
    let pair = Pair::judge(0, 1, &whole(124), 125, 124);

    assert_eq!(pair.relation, Relation::Contains);
    assert_eq!(pair.paired, 124);
}

/// One Piece v077's c0/c1 shape: 184 of 236 paired, neither side wholly
/// covered. The containment variants must not swallow the case they are
/// distinguished from.
#[test]
fn neither_side_wholly_covered_is_still_distinct() {
    let pair = Pair::judge(0, 1, &spanning(100, 184), 236, 234);

    assert_eq!(pair.relation, Relation::Distinct);
}

/// A refusal is a claim about the volume, not about the alignment.
#[test]
fn nothing_paired_is_a_refusal() {
    let runs = vec![Run::gap(PageRange::new(0, 201))];

    let pair = Pair::judge(0, 1, &runs, 202, 198);

    assert_eq!(pair.relation, Relation::Refused);
    assert_eq!(pair.verdict, Verdict::Refused);
}

/// One Piece volume 77, which is the case that argues for all-pairs: c1 and
/// c2 are the same edition at different paginations and c0 is a different
/// crop. Anchoring on c0 reports three unrelated files.
#[test]
fn identity_pairs_collapse_and_the_outlier_stays_alone() {
    let pairs = vec![
        Pair::judge(0, 1, &fragmented(), 236, 234),
        Pair::judge(0, 2, &fragmented(), 236, 234),
        Pair::judge(1, 2, &whole(234), 234, 234),
    ];

    let clusters = cluster(3, &pairs);

    assert_eq!(clusters, vec![vec![0], vec![1, 2]]);
    assert!(contradictions(&clusters, &pairs).is_empty());
}

/// Identity is transitive or one of the three solves is wrong. Every pair
/// is solved anyway, so the contradiction is already in hand: report it and
/// keep the merge, which is still the best available answer.
#[test]
fn identity_that_is_not_transitive_is_reported_and_still_merged() {
    let pairs = vec![
        Pair::judge(0, 1, &whole(200), 200, 200),
        Pair::judge(1, 2, &whole(200), 200, 200),
        Pair::judge(0, 2, &fragmented(), 200, 200),
    ];

    let clusters = cluster(3, &pairs);

    assert_eq!(clusters, vec![vec![0, 1, 2]]);
    assert_eq!(contradictions(&clusters, &pairs), vec![(0, 2)]);
}

/// A cluster's members are examined in ascending order, but the pair list
/// is not required to hold them that way round. A pair stored the other
/// way is the same pair.
#[test]
fn a_contradiction_is_found_however_the_pair_is_stored() {
    let pairs = vec![
        Pair::judge(0, 1, &whole(200), 200, 200),
        Pair::judge(1, 2, &whole(200), 200, 200),
        Pair::judge(2, 0, &fragmented(), 200, 200),
    ];

    let clusters = cluster(3, &pairs);

    assert_eq!(clusters, vec![vec![0, 1, 2]]);
    assert_eq!(contradictions(&clusters, &pairs), vec![(0, 2)]);
}

/// The Bocchi Gaiden file and the One Piece Starter Book: named for a
/// volume, refused by every copy of it.
#[test]
fn a_copy_that_refuses_against_everything_is_flagged() {
    let pairs = vec![
        Pair::judge(0, 1, &whole(124), 124, 124),
        Pair::judge(0, 2, &refusal(124), 124, 185),
        Pair::judge(1, 2, &refusal(124), 124, 185),
    ];

    assert_eq!(flagged(3, &pairs), vec![2]);
}

/// With two copies and a refusal between them, nothing in the set says
/// which one is the impostor, so both are flagged and nothing is ranked.
/// `--against` is what resolves it.
#[test]
fn two_copies_that_refuse_each_other_flag_both() {
    let pairs = vec![Pair::judge(0, 1, &refusal(202), 202, 198)];

    assert_eq!(flagged(2, &pairs), vec![0, 1]);
}

/// A copy nothing was compared against has not refused anything, and must
/// not be read as though it had. An empty `all()` is vacuously true, so
/// without the guard every uncompared copy comes back an impostor.
#[test]
fn a_copy_with_no_pairs_at_all_is_not_flagged() {
    let pairs = vec![Pair::judge(0, 1, &whole(124), 124, 124)];

    assert_eq!(flagged(3, &pairs), Vec::<usize>::new());
}

/// Content first. `notes.md`: "a missing page cannot be recovered from a
/// cleaner alignment of the pages that remain".
#[test]
fn ranking_puts_the_copy_with_more_slots_first() {
    let ranked = rank(vec![
        Ranked {
            cluster: vec![1],
            slots: 234,
            against: None,
        },
        Ranked {
            cluster: vec![0],
            slots: 236,
            against: None,
        },
    ]);

    assert_eq!(ranked[0].cluster, vec![0]);
    assert!(decided(&ranked));
}

/// Without a reference there is no honest tiebreak, so a tie is reported as
/// a tie rather than broken by a statistic that decides nothing.
#[test]
fn a_tie_on_slots_with_no_reference_is_undecided() {
    let ranked = rank(vec![
        Ranked {
            cluster: vec![0],
            slots: 198,
            against: None,
        },
        Ranked {
            cluster: vec![1],
            slots: 198,
            against: None,
        },
    ]);

    assert!(!decided(&ranked));
}

/// The Chainsaw Man table: four scans against one English release, ranked
/// on slots then pairings then runs. `v01b` wins on every axis at once.
///
/// **Cluster 2 is listed before cluster 0 on purpose.** The two are tied on
/// slots and on pairings and separate only on runs, so they are the pair
/// the reference tiebreak exists for — and `sort_by` is stable, which means
/// a tiebreak that did nothing at all would still leave them in whatever
/// order they arrived in. Given in the order the answer is not, so the
/// assertion below can only pass if something actually reordered them.
#[test]
fn a_reference_edition_breaks_a_tie_on_pairings_then_runs() {
    let ranked = rank(vec![
        Ranked {
            cluster: vec![2],
            slots: 198,
            against: Some(Against {
                runs: 7,
                paired: 182,
            }),
        },
        Ranked {
            cluster: vec![1],
            slots: 199,
            against: Some(Against {
                runs: 3,
                paired: 183,
            }),
        },
        Ranked {
            cluster: vec![0],
            slots: 198,
            against: Some(Against {
                runs: 5,
                paired: 182,
            }),
        },
    ]);

    let order: Vec<usize> = ranked.iter().map(|r| r.cluster[0]).collect();
    assert_eq!(
        order,
        vec![1, 0, 2],
        "199 slots first, then 5 runs before 7"
    );
    assert!(decided(&ranked));
}

/// A tie on slots that the reference breaks is decided; one it does not is
/// not.
///
/// Every other test of `decided` either has a winner on slots or has no
/// reference at all, so the arm that reads the reference never runs in any
/// of them — and without it `decided` answers from the slot count alone and
/// calls every reference-broken tie a tie.
#[test]
fn a_tie_on_slots_is_decided_by_the_reference_and_only_by_a_real_difference() {
    let tied = |paired, runs| Ranked {
        cluster: vec![0],
        slots: 198,
        against: Some(Against { runs, paired }),
    };

    assert!(
        decided(&[tied(183, 3), tied(182, 3)]),
        "more pages paired against the reference"
    );
    assert!(
        decided(&[tied(182, 3), tied(182, 7)]),
        "the same pages, in fewer runs"
    );
    assert!(
        !decided(&[tied(182, 3), tied(182, 3)]),
        "the same on both axes is a tie, and is reported as one"
    );
}

/// Two byte-identical copies whose fingerprint comes back with no slots.
///
/// The identity short-circuit skips the solve and states the answer
/// directly, as a run spanning `0..slots - 1` — one subtraction away from
/// underflowing when there are no slots, and `slots > 0` is the guard
/// against it.
///
/// Reached through the closure `resolve_with` exists to take rather than
/// through empty archives, which cannot get this far: `Signature::of`
/// refuses a member with no page images in step 1, several steps before
/// anything is fingerprinted. So this is a statement about the seam, not
/// about a set of files — which is exactly what the guard is protecting,
/// since through `resolve` the count can never be zero.
#[test]
fn an_identity_pair_with_no_slots_resolves_rather_than_underflowing() {
    let root = taiyaku_read::test_support::TempDir::new("duplicates-noslots");
    let copies: Vec<Vec<PathBuf>> = ["a", "b"]
        .iter()
        .map(|name| {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("000.jpg"), b"identical bytes").unwrap();
            vec![dir]
        })
        .collect();

    let mut fingerprint = |_: &[PathBuf]| Ok(Vec::new());
    let resolution = resolve_with(&copies, None, &mut fingerprint);

    let resolution = resolution.expect("a slotless identity pair resolves");
    assert!(
        resolution.copies.iter().all(|copy| copy.slots == 0),
        "the closure's answer is the slot count, whatever the files hold"
    );
    // The guard sends the pair to the solver rather than declaring the
    // identity, and a solve over two empty sequences pairs nothing — so the
    // two are reported as refusing each other. Stated because it is the
    // consequence of not underflowing, not because it is a good answer.
    assert_eq!(resolution.clusters, vec![vec![0], vec![1]]);
    assert_eq!(resolution.flagged, vec![0, 1]);
}

/// A hundred short runs over most of the pages: the shape a needle and a
/// differently-cropped scan both produce.
fn fragmented() -> Vec<Run> {
    (0..100)
        .map(|i| Run::paired(PageRange::new(i * 2, i * 2), PageRange::new(i * 2, i * 2)))
        .collect()
}

fn refusal(a_slots: u32) -> Vec<Run> {
    vec![Run::gap(PageRange::new(0, a_slots - 1))]
}

/// One Piece volume 77: c0 is a different crop at 236 slots, c1 and c2 are
/// one edition at two paginations. The line a survey script collects,
/// asserted verbatim — `scripts/margin-survey.sh` reads `stats:` lines
/// exactly this way, and a format nobody can test drifts.
#[test]
fn the_set_line_reports_every_figure_that_decided_the_answer() {
    let line = set_line(&v077());

    assert_eq!(
        line,
        "set: copies=3 signature=0 clusters=2 flagged=0 refused=0 fragmented=2 \
             slots=236 decided=yes best=op/v077-c0"
    );
}

/// Nothing ranked still produces a line a survey loop can parse, rather
/// than vanishing.
#[test]
fn a_set_with_nothing_ranked_still_produces_a_line() {
    let resolution = Resolution {
        copies: vec![
            Copy {
                label: "op/v002-c0".into(),
                slots: 202,
            },
            Copy {
                label: "op/v002-c1".into(),
                slots: 129,
            },
        ],
        signature_settled: 0,
        pairs: vec![Pair::judge(0, 1, &refusal(202), 202, 129)],
        clusters: vec![vec![0], vec![1]],
        contradictions: Vec::new(),
        flagged: vec![0, 1],
        reference_given: false,
        ranked: Vec::new(),
    };

    assert_eq!(
        set_line(&resolution),
        "set: copies=2 signature=0 clusters=2 flagged=2 refused=1 fragmented=0 \
             slots=- decided=no best=-"
    );
}

#[test]
fn the_tables_show_the_pairs_the_clusters_and_the_rank() {
    let printed = tables(&v077());

    assert_eq!(
        printed,
        "\
copies
  0  236 slots  op/v077-c0
  1  234 slots  op/v077-c1
  2  234 slots  op/v077-c2

pairs
  0-1  100 runs  184 of 236  FRAGMENTED
  0-2  100 runs  186 of 236  FRAGMENTED
  1-2  1 run     234 of 234  sound  identity

clusters
  {0}
  {1, 2}  identity

rank
  1.  236 slots  copy 0
  2.  234 slots  copy 1
"
    );
}

/// Containment gets its own line, naming the contained copy first:
/// "0 within 1", not a bare "identity" — and the container's own slot count,
/// 199, so the figure that decides whether the containment matters is on
/// the line rather than only inferable from the `copies` table above.
/// Pinned separately from the identity table above so the "X within Y"
/// rendering is not left only inferred from the enum's doc comment.
#[test]
fn the_tables_name_a_containment_pair() {
    let printed = tables(&cm_v01());

    assert_eq!(
        printed,
        "\
copies
  0  198 slots  cm/v01
  1  199 slots  cm/v01b

pairs
  0-1  1 run     198 of 198  sound  0 within 1 (199 slots)

clusters
  {0}
  {1}

rank
  1.  199 slots  copy 1
  2.  198 slots  copy 0
"
    );
}

/// The Chainsaw Man `v01`/`v01b` resolution, built from the shapes rather
/// than from files: `v01` wholly within `v01b`, which carries one more
/// slot and wins the rank on it.
fn cm_v01() -> Resolution {
    let pairs = vec![Pair::judge(0, 1, &whole(198), 198, 199)];
    let clusters = cluster(2, &pairs);
    Resolution {
        copies: vec![
            Copy {
                label: "cm/v01".into(),
                slots: 198,
            },
            Copy {
                label: "cm/v01b".into(),
                slots: 199,
            },
        ],
        signature_settled: 0,
        contradictions: contradictions(&clusters, &pairs),
        flagged: flagged(2, &pairs),
        reference_given: false,
        ranked: rank(vec![
            Ranked {
                cluster: vec![0],
                slots: 198,
                against: None,
            },
            Ranked {
                cluster: vec![1],
                slots: 199,
                against: None,
            },
        ]),
        clusters,
        pairs,
    }
}

/// The mirror containment shape: Bocchi's hamaji copy carries the extra
/// slot over krc, so it is the `a` side rather than the `b` side that
/// contains. `Contains` must render the same "X within Y" wording as
/// `Within`, naming the contained copy — here `b` — first, but *without*
/// a parenthetical slot count: the container here is `a`, at 125 slots,
/// and 125 is already printed as the denominator in "124 of 125" earlier
/// on the same line, so the suffix does not say it again.
#[test]
fn the_tables_name_a_containment_pair_the_other_way() {
    let printed = tables(&bocchi_hamaji());

    assert_eq!(
        printed,
        "\
copies
  0  125 slots  bocchi/hamaji
  1  124 slots  bocchi/krc

pairs
  0-1  1 run     124 of 125  sound  1 within 0

clusters
  {0}
  {1}

rank
  1.  125 slots  copy 0
  2.  124 slots  copy 1
"
    );
}

/// Bocchi's hamaji copy against krc, built from the shapes rather than
/// from files: 125 slots against 124, all 124 shared ones paired, `a`
/// carrying the extra one.
fn bocchi_hamaji() -> Resolution {
    let pairs = vec![Pair::judge(0, 1, &whole(124), 125, 124)];
    let clusters = cluster(2, &pairs);
    Resolution {
        copies: vec![
            Copy {
                label: "bocchi/hamaji".into(),
                slots: 125,
            },
            Copy {
                label: "bocchi/krc".into(),
                slots: 124,
            },
        ],
        signature_settled: 0,
        contradictions: contradictions(&clusters, &pairs),
        flagged: flagged(2, &pairs),
        reference_given: false,
        ranked: rank(vec![
            Ranked {
                cluster: vec![0],
                slots: 125,
                against: None,
            },
            Ranked {
                cluster: vec![1],
                slots: 124,
                against: None,
            },
        ]),
        clusters,
        pairs,
    }
}

/// The flagged section, where the `--against` rule's output lives. Without a
/// reference both copies are named, and the hint says what would settle it
/// — a person reading an empty rank section otherwise has to work out for
/// themselves why it is empty.
#[test]
fn the_tables_flag_both_copies_and_say_what_would_settle_it() {
    let printed = tables(&starter_book(false));

    assert_eq!(
        printed,
        "\
copies
  0  202 slots  op/v002-c0
  1  198 slots  op/v002-c1

pairs
  0-1  1 run     0 of 202  REFUSED

clusters
  {0}
  {1}

not this volume
  0  refused against every other copy  op/v002-c0
  1  refused against every other copy  op/v002-c1
  nothing in this set says which of these is the impostor; pass --against a reference edition
"
    );
}

/// The same set with a reference edition. The reference paired with copy 1
/// and nothing with copy 0, so only copy 0 is named — and with a different
/// reason, because "refused against every other copy" is not what decided
/// it and the pairs table would contradict that sentence where a flagged
/// copy had paired inside the set. The hint is gone: the question it asks
/// has been answered.
#[test]
fn the_tables_name_only_what_the_reference_refused_and_drop_the_hint() {
    let printed = tables(&starter_book(true));

    assert_eq!(
        printed,
        "\
copies
  0  202 slots  op/v002-c0
  1  198 slots  op/v002-c1

pairs
  0-1  1 run     0 of 202  REFUSED

clusters
  {0}
  {1}

not this volume
  0  paired nothing with the reference edition  op/v002-c0

rank
  1.  198 slots  copy 1  198 paired  1 run
"
    );
}

/// The One Piece Starter Book pair: two copies filed as one volume with a
/// refusal between them, at 202 slots and 198. Built twice over from the
/// same shapes — as the set alone resolves it, and as a reference edition
/// that pairs with the smaller copy resolves it.
fn starter_book(with_reference: bool) -> Resolution {
    let pairs = vec![Pair::judge(0, 1, &refusal(202), 202, 198)];
    let clusters = cluster(2, &pairs);
    Resolution {
        copies: vec![
            Copy {
                label: "op/v002-c0".into(),
                slots: 202,
            },
            Copy {
                label: "op/v002-c1".into(),
                slots: 198,
            },
        ],
        signature_settled: 0,
        contradictions: contradictions(&clusters, &pairs),
        flagged: if with_reference {
            vec![0]
        } else {
            flagged(2, &pairs)
        },
        reference_given: with_reference,
        ranked: if with_reference {
            vec![Ranked {
                cluster: vec![1],
                slots: 198,
                against: Some(Against {
                    runs: 1,
                    paired: 198,
                }),
            }]
        } else {
            Vec::new()
        },
        clusters,
        pairs,
    }
}

/// The contradictions branch of `tables()`. Built on
/// `identity_that_is_not_transitive_is_reported_and_still_merged`'s shape
/// — 0≡1 and 1≡2 both hold, but 0≢2 — so the cluster still merges all
/// three and the sentence names the pair whose solve disagreed with the
/// merge it is part of.
#[test]
fn the_tables_report_a_contradiction_in_a_merged_cluster() {
    let printed = tables(&contradiction_shape());

    assert_eq!(
        printed,
        "\
copies
  0  200 slots  shape/c0
  1  200 slots  shape/c1
  2  200 slots  shape/c2

pairs
  0-1  1 run     200 of 200  sound  identity
  1-2  1 run     200 of 200  sound  identity
  0-2  100 runs  100 of 200  FRAGMENTED

clusters
  {0, 1, 2}  identity

copies 0 and 2 were merged but did not align as identity; one of the three solves in that cluster is wrong

rank
  1.  200 slots  copy 0
"
    );
}

/// The shape `identity_that_is_not_transitive_is_reported_and_still_merged`
/// establishes, turned into a full `Resolution` so `tables()` can render
/// it: 0≡1 and 1≡2 both hold, but 0≢2 — a contradiction the union-find
/// still merges into one cluster.
fn contradiction_shape() -> Resolution {
    let pairs = vec![
        Pair::judge(0, 1, &whole(200), 200, 200),
        Pair::judge(1, 2, &whole(200), 200, 200),
        Pair::judge(0, 2, &fragmented(), 200, 200),
    ];
    let clusters = cluster(3, &pairs);
    Resolution {
        copies: vec![
            Copy {
                label: "shape/c0".into(),
                slots: 200,
            },
            Copy {
                label: "shape/c1".into(),
                slots: 200,
            },
            Copy {
                label: "shape/c2".into(),
                slots: 200,
            },
        ],
        signature_settled: 0,
        contradictions: contradictions(&clusters, &pairs),
        flagged: flagged(3, &pairs),
        reference_given: false,
        ranked: rank(vec![Ranked {
            cluster: vec![0, 1, 2],
            slots: 200,
            against: None,
        }]),
        clusters,
        pairs,
    }
}

/// The v077 resolution, built from the shapes rather than from files.
fn v077() -> Resolution {
    let pairs = vec![
        Pair::judge(0, 1, &spanning(100, 184), 236, 234),
        Pair::judge(0, 2, &spanning(100, 186), 236, 234),
        Pair::judge(1, 2, &whole(234), 234, 234),
    ];
    let clusters = cluster(3, &pairs);
    Resolution {
        copies: vec![
            Copy {
                label: "op/v077-c0".into(),
                slots: 236,
            },
            Copy {
                label: "op/v077-c1".into(),
                slots: 234,
            },
            Copy {
                label: "op/v077-c2".into(),
                slots: 234,
            },
        ],
        signature_settled: 0,
        contradictions: contradictions(&clusters, &pairs),
        flagged: flagged(3, &pairs),
        reference_given: false,
        ranked: rank(vec![
            Ranked {
                cluster: vec![0],
                slots: 236,
                against: None,
            },
            Ranked {
                cluster: vec![1, 2],
                slots: 234,
                against: None,
            },
        ]),
        clusters,
        pairs,
    }
}

/// Exactly `runs` paired runs covering exactly `paired` slots between them,
/// each separated by a slot of gap so they stay separate runs.
///
/// The remainder is spread over the leading runs rather than dropped: a
/// helper that quietly covered 100 slots when the test said 184 would make
/// the assertion pass for the wrong reason. Assumes `paired >= runs`, which
/// every caller satisfies.
fn spanning(runs: u32, paired: u32) -> Vec<Run> {
    let (each, longer) = (paired / runs, paired % runs);
    let mut out = Vec::new();
    let mut at = 0;
    for run in 0..runs {
        let length = each + u32::from(run < longer);
        out.push(Run::paired(
            PageRange::new(at, at + length - 1),
            PageRange::new(at, at + length - 1),
        ));
        at += length + 1;
    }
    out
}

/// Copies are `--`-separated as in `topology`, but unlike `topology` this
/// command has flags — and `--against` itself begins with `--`. Group
/// collection therefore stops at the first argument that starts with `--`
/// and is not exactly `--`.
#[test]
fn groups_end_at_the_first_flag() {
    let args: Vec<String> = ["a", "--", "b1", "b2", "--against", "ref", "--ltr"]
        .iter()
        .map(ToString::to_string)
        .collect();

    let parsed = groups(&args).unwrap();

    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[1].len(), 2, "both b members, and not `ref`");
}

#[test]
fn one_copy_is_not_a_duplicate_set() {
    let args: Vec<String> = ["only"].iter().map(ToString::to_string).collect();

    let error = groups(&args).unwrap_err();

    assert!(error.contains("two copies"), "unhelpful: {error}");
}

/// The same argument array `groups_end_at_the_first_flag` parses: the
/// reference must not swallow `--ltr`, the flag that follows it.
#[test]
fn against_members_stops_at_the_next_flag() {
    let args: Vec<String> = ["a", "--", "b1", "b2", "--against", "ref", "--ltr"]
        .iter()
        .map(ToString::to_string)
        .collect();

    let members = against_members(&args).unwrap();

    assert_eq!(members, Some(vec![PathBuf::from("ref")]));
}

/// `--against` with nothing after it would otherwise collect an empty
/// list and proceed with no reference edition — silently, since an empty
/// list and "no `--against`" looked the same. Dropping the reference can
/// change `decided=` and the rank order, so this must be refused rather
/// than swallowed.
#[test]
fn against_with_no_member_is_an_error() {
    let args: Vec<String> = ["a", "--", "b", "--against", "--ltr"]
        .iter()
        .map(ToString::to_string)
        .collect();

    let error = against_members(&args).unwrap_err();

    assert!(error.contains("--against"), "unhelpful: {error}");
}

/// The saving the signature step exists for. Three copies of one scan
/// fingerprint once, not three times — skipping a solve saves microseconds
/// and skipping a decode of every page saves seconds.
#[test]
fn copies_confirmed_identical_all_stand_for_the_first() {
    let same = vec![signed("aaa"), signed("aaa"), signed("aaa")];
    let mut confirmed = 0;

    let stands_for = representatives(&same, |_, _| {
        confirmed += 1;
        Ok(true)
    })
    .unwrap();

    assert_eq!(stands_for, vec![0, 0, 0]);
    assert_eq!(
        confirmed, 2,
        "each later copy confirmed against the first only"
    );
}

/// The signature is a claim about sizes, not about bytes. When the content
/// hash refutes it the copies stay separate and get solved like any other
/// pair — the confirmation is what makes the short-circuit sound rather
/// than merely fast.
#[test]
fn a_signature_match_the_content_hash_refutes_does_not_collapse() {
    let same = vec![signed("aaa"), signed("aaa")];

    let stands_for = representatives(&same, |_, _| Ok(false)).unwrap();

    assert_eq!(stands_for, vec![0, 1]);
}

/// And the expensive confirmation is never reached where a listing already
/// settled it.
#[test]
fn copies_with_different_signatures_are_never_hashed() {
    let different = vec![signed("aaa"), signed("bbb")];

    let stands_for = representatives(&different, |_, _| {
        panic!("a differing signature must not cost a content hash")
    })
    .unwrap();

    assert_eq!(stands_for, vec![0, 1]);
}

/// The bad shape `memoized_confirm` exists to fix: a set whose signatures
/// all collide but whose bytes all differ, so `confirm` never returns
/// `true` and `representatives` tries every earlier copy against every
/// later one — five copies, ten comparisons. Without memoisation each
/// comparison re-hashes both sides, so copy 0 alone would be hashed four
/// times over. With it, every copy's digest is computed exactly once,
/// however many candidates it goes on to be compared against.
#[test]
fn a_copy_is_hashed_once_however_many_candidates_compare_it() {
    let same = vec![signed("aaa"); 5];
    let mut hashed = vec![0u32; 5];

    let confirm = memoized_confirm(5, |idx| {
        hashed[idx] += 1;
        // A digest distinct per copy, so every comparison refutes the
        // signature match and `representatives` keeps trying — the
        // all-refuse shape that makes it compare every earlier copy.
        Ok(idx.to_string())
    });
    let stands_for = representatives(&same, confirm).unwrap();

    assert_eq!(
        stands_for,
        vec![0, 1, 2, 3, 4],
        "all distinct; nothing collapses"
    );
    assert_eq!(
        hashed,
        vec![1, 1, 1, 1, 1],
        "each copy's digest computed exactly once, not once per comparison"
    );
}

fn signed(digest: &str) -> Signature {
    Signature {
        pages: 3,
        bytes: 600,
        digest: digest.to_string(),
    }
}

/// The saving the signature step exists for, held by the only means that
/// can hold it: counting decodes. Two byte-identical copies must
/// fingerprint once, not twice — and must still come out identity, since
/// the second copy's slot count is derived rather than measured.
#[test]
fn a_copy_confirmed_identical_is_never_fingerprinted() {
    let (first, second) = two_identical_copies("skip");
    let mut decoded = 0;
    let mut fingerprint = |_: &[PathBuf]| {
        decoded += 1;
        Ok(vec![PageHash([1, 2, 3, 4]); 6])
    };

    let resolved = resolve_with(
        &[vec![first.to_path_buf()], vec![second.to_path_buf()]],
        None,
        &mut fingerprint,
    )
    .unwrap();

    assert_eq!(decoded, 1, "the second copy is derived, not decoded");
    assert_eq!(resolved.signature_settled, 1);
    assert_eq!(
        resolved.copies[1].slots, 6,
        "slot count taken from the first"
    );
    assert_eq!(resolved.pairs[0].relation, Relation::Identity);
    assert_eq!(
        resolved.pairs[0].runs, 1,
        "no solve ran; the run is the whole of it"
    );
    assert_eq!(resolved.clusters, vec![vec![0, 1]]);
}

/// The mirror case: a signature match the content hash refutes. Same page
/// sizes give matching signatures, but different bytes must still cost a
/// fingerprint for both copies — proving the content-hash confirmation is
/// load-bearing rather than decorative.
#[test]
fn a_signature_match_the_bytes_refute_fingerprints_both() {
    let (first, second) = two_copies_matching_sizes_different_bytes("differ");
    let mut decoded = 0;
    let mut fingerprint = |_: &[PathBuf]| {
        decoded += 1;
        Ok(vec![PageHash([1, 2, 3, 4]); 6])
    };

    // The precondition the test's name depends on. If `Signature::of` ever
    // stopped matching these two, `representatives` would never call
    // `confirm` at all, both copies would be fingerprinted for want of a
    // signature match rather than in spite of one, and both assertions
    // below would pass for the opposite reason.
    assert_eq!(
        Signature::of(std::slice::from_ref(&first.to_path_buf()))
            .unwrap()
            .digest,
        Signature::of(std::slice::from_ref(&second.to_path_buf()))
            .unwrap()
            .digest,
        "the fixture must produce a signature match for this test to mean anything"
    );

    let resolved = resolve_with(
        &[vec![first.to_path_buf()], vec![second.to_path_buf()]],
        None,
        &mut fingerprint,
    )
    .unwrap();

    assert_eq!(decoded, 2, "the content hash refuted the signature match");
    assert_eq!(resolved.signature_settled, 0);
}

/// Two temp directories holding the same handful of `.jpg` files with the
/// same bytes, so `Signature::of` and `content_digest` do genuine work on
/// real bytes while `fingerprint` is a stub that never opens them — the
/// files need not be decodable images at all, which is the whole point.
/// Each is a directory of its own, removed when the test ends.
fn two_identical_copies(
    tag: &str,
) -> (
    taiyaku_read::test_support::TempDir,
    taiyaku_read::test_support::TempDir,
) {
    let first = taiyaku_read::test_support::TempDir::new(&format!("duplicates-{tag}-a"));
    let second = taiyaku_read::test_support::TempDir::new(&format!("duplicates-{tag}-b"));
    for dir in [&first, &second] {
        for (name, bytes) in [
            ("p1.jpg", &b"one"[..]),
            ("p2.jpg", &b"two"[..]),
            ("p3.jpg", &b"three"[..]),
        ] {
            std::fs::write(dir.join(name), bytes).unwrap();
        }
    }
    (first, second)
}

/// Two copies with matching page sizes but differing content: same
/// signature, different content digest. Built from `two_identical_copies`
/// by overwriting one file with same-length, different bytes.
fn two_copies_matching_sizes_different_bytes(
    tag: &str,
) -> (
    taiyaku_read::test_support::TempDir,
    taiyaku_read::test_support::TempDir,
) {
    let (first, second) = two_identical_copies(tag);
    std::fs::write(second.join("p2.jpg"), b"TWO").unwrap();
    (first, second)
}

/// What the set alone can conclude, pinned so the reference rule below
/// cannot quietly change it: two copies, a refusal between them, no
/// reference. Both are flagged because the refusal is symmetric, nothing is
/// ranked, and the output says what would settle it.
#[test]
fn a_refusing_pair_with_no_reference_flags_both_and_ranks_nothing() {
    let (_impostor_dir, impostor) = copy_on_disk("within-set", "impostor", 8);
    let (_real_dir, real) = copy_on_disk("within-set", "real", 6);
    let mut fingerprint = |members: &[PathBuf]| Ok(synthetic_pages(members));

    let resolved = resolve_with(
        &[vec![impostor.to_path_buf()], vec![real.to_path_buf()]],
        None,
        &mut fingerprint,
    )
    .unwrap();

    assert_eq!(resolved.pairs[0].relation, Relation::Refused);
    assert_eq!(
        resolved.flagged,
        vec![0, 1],
        "nothing inside the set says which of the two is the impostor"
    );
    assert!(resolved.ranked.is_empty());
    assert!(
        tables(&resolved).contains("nothing in this set says which of these is the impostor"),
        "the hint belongs here, where the set could not decide:\n{}",
        tables(&resolved)
    );
}

/// The same two copies with a reference edition, which is the case
/// `--against` exists for. The reference decides: the copy that pairs with
/// it is this volume even though it refused the other copy, and the copy
/// that pairs nothing with it is not, even though it carries more slots.
///
/// The impostor holds 8 slots to the real copy's 6, so a ranking that
/// merely ordered on content would put it first. It is not ordered at all —
/// the reference removes it before the ranking sees it.
#[test]
fn a_reference_the_real_copy_pairs_with_flags_only_the_impostor() {
    let (_impostor_dir, impostor) = copy_on_disk("reference-decides", "impostor", 8);
    let (_real_dir, real) = copy_on_disk("reference-decides", "real", 6);
    let (_reference_dir, reference) = copy_on_disk("reference-decides", "reference", 6);
    let mut fingerprint = |members: &[PathBuf]| Ok(synthetic_pages(members));

    let resolved = resolve_with(
        &[vec![impostor.to_path_buf()], vec![real.to_path_buf()]],
        Some(std::slice::from_ref(&reference.to_path_buf())),
        &mut fingerprint,
    )
    .unwrap();

    assert_eq!(
        resolved.pairs[0].relation,
        Relation::Refused,
        "the two still refuse each other; it is the reference that decides"
    );
    assert_eq!(
        resolved.flagged,
        vec![0],
        "only the copy the reference refused"
    );
    assert_eq!(
        resolved.ranked.len(),
        1,
        "the real copy is ranked despite refusing the other: {:?}",
        resolved.ranked
    );
    assert_eq!(resolved.ranked[0].cluster, vec![1]);
    assert_eq!(
        resolved.ranked[0].against,
        Some(Against { runs: 1, paired: 6 }),
        "scored against the reference, which a flagged cluster never was"
    );
    assert!(decided(&resolved.ranked));
    assert!(
        !tables(&resolved).contains("nothing in this set says"),
        "the question the hint asks was answered:\n{}",
        tables(&resolved)
    );
}

/// A reference edition neither copy pairs with. Then neither is this
/// volume, both are flagged and nothing is ranked — the reference decides
/// against both as readily as it decides between them.
#[test]
fn a_reference_both_copies_refuse_flags_both() {
    let (_impostor_dir, impostor) = copy_on_disk("reference-refuses-both", "impostor", 8);
    let (_real_dir, real) = copy_on_disk("reference-refuses-both", "real", 6);
    let (_alien_dir, alien) = copy_on_disk("reference-refuses-both", "alien", 6);
    let mut fingerprint = |members: &[PathBuf]| Ok(synthetic_pages(members));

    let resolved = resolve_with(
        &[vec![impostor.to_path_buf()], vec![real.to_path_buf()]],
        Some(std::slice::from_ref(&alien.to_path_buf())),
        &mut fingerprint,
    )
    .unwrap();

    assert_eq!(resolved.flagged, vec![0, 1], "none of them is the volume");
    assert!(resolved.ranked.is_empty());
    assert_eq!(
        set_line(&resolved),
        "set: copies=2 signature=0 clusters=2 flagged=2 refused=1 fragmented=0 \
             slots=- decided=no best=-"
    );
    assert!(
        !tables(&resolved).contains("nothing in this set says"),
        "a reference was given, so the hint asking for one must not print:\n{}",
        tables(&resolved)
    );
}

/// One copy on disk: a directory of throwaway `.jpg` files, holding the
/// role in its name.
///
/// The files have to exist because `resolve_with` signs every copy from a
/// listing before anything is fingerprinted, and the page counts differ by
/// role so no two copies sign alike. What the *solver* sees is not in these
/// bytes at all — `synthetic_pages` decides that — so they need not be
/// images. The copy is `<role>` inside a directory of its own, so
/// `synthetic_pages` can read the role off its name; the directory goes when
/// the test ends.
fn copy_on_disk(
    tag: &str,
    role: &str,
    pages: usize,
) -> (taiyaku_read::test_support::TempDir, PathBuf) {
    let scratch = taiyaku_read::test_support::TempDir::new(&format!("duplicates-{tag}"));
    let dir = scratch.join(role);
    std::fs::create_dir_all(&dir).unwrap();
    for page in 0..pages {
        std::fs::write(dir.join(format!("p{page}.jpg")), format!("{role}-{page}")).unwrap();
    }
    (scratch, dir)
}

/// Six 128-bit patterns, each with exactly 64 bits set and no two of them
/// closer than 64 bits apart.
///
/// The equal weights are what let a page of one family sit exactly 128 bits
/// from every page of another; the mutual distance is what stops a page
/// pairing with the wrong page of its own family.
const PATTERNS: [(u64, u64); 6] = [
    (0xFFFF_FFFF_FFFF_FFFF, 0x0000_0000_0000_0000),
    (0x0000_0000_FFFF_FFFF, 0xFFFF_FFFF_0000_0000),
    (0xAAAA_AAAA_AAAA_AAAA, 0xAAAA_AAAA_AAAA_AAAA),
    (0x5555_5555_5555_5555, 0x5555_5555_5555_5555),
    (0xFFFF_0000_FFFF_0000, 0x0000_FFFF_0000_FFFF),
    (0x00FF_00FF_00FF_00FF, 0xFF00_FF00_FF00_FF00),
];

/// What the solver sees for a copy, chosen by the role in its directory
/// name rather than read from its bytes.
///
/// `resolve_with` takes a fingerprint closure precisely so the rules over
/// alignments can be tested on stated alignments rather than on fixtures,
/// and a refusal has to be a certainty here rather than a likely outcome of
/// pseudo-random bits — a test that measured the calibration threshold by
/// accident would fail the day it moved.
///
/// `real` and `reference` are the same six patterns in the low half of the
/// hash, so they align as one clean run over every slot. `impostor` is the
/// same patterns moved into the high half, so it differs from every `real`
/// page in exactly 128 of 256 bits: similarity 0.5 for **every** pair in
/// the matrix, which leaves calibration no separation to find and refuses.
/// `alien` is that high half filled solid, a constant similarity against
/// both families and so a refusal against both.
fn synthetic_pages(members: &[PathBuf]) -> Vec<PageHash> {
    let name = members[0]
        .file_name()
        .expect("a copy is named for its role")
        .to_string_lossy()
        .to_string();
    match name.as_str() {
        "real" | "reference" => PATTERNS
            .iter()
            .map(|&(low, high)| PageHash([low, high, 0, 0]))
            .collect(),
        "impostor" => (0..8)
            .map(|page| {
                let (low, high) = PATTERNS[page % PATTERNS.len()];
                PageHash([0, 0, low, high])
            })
            .collect(),
        "alien" => vec![PageHash([0, 0, u64::MAX, u64::MAX]); 6],
        other => panic!("no synthetic pages for the role `{other}`"),
    }
}
