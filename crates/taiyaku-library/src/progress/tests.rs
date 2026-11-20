use super::*;

/// The words the scan feed sends a step by, which the web client reads, so a
/// changed word leaves the client without its name for the step.
#[test]
fn each_step_is_sent_by_its_own_word() {
    let words: Vec<&str> = [
        Step::Waiting,
        Step::Listing,
        Step::Resolving,
        Step::Hashing,
        Step::Writing,
        Step::Finishing,
        Step::Fingerprinting,
        Step::Matching,
    ]
    .into_iter()
    .map(Step::as_str)
    .collect();
    assert_eq!(
        words,
        [
            "waiting",
            "listing",
            "resolving",
            "hashing",
            "writing",
            "finishing",
            "fingerprinting",
            "matching"
        ]
    );
}
