use super::*;

#[test]
fn the_dev_set_is_nine_editions_and_five_pages_each() {
    assert_eq!(DEV_EDITIONS.len(), 9);
    assert_eq!(dev_pages(200).len(), 5, "45 pages over nine editions");
}

#[test]
fn the_picks_are_a_strict_subset_of_the_ten_the_survey_uses() {
    let ten: Vec<usize> = (0..10).map(|i| i * 195 / 10 + 195 / 20).collect();
    for page in dev_pages(195) {
        assert!(
            ten.contains(&page),
            "page {page} is not one of the survey's ten"
        );
    }
}

#[test]
fn the_picks_are_spread_rather_than_taken_from_the_front() {
    let picked = dev_pages(200);
    assert!(picked[0] < 40, "starts near the front");
    assert!(*picked.last().unwrap() > 150, "and reaches the back");
}

#[test]
fn an_edition_shorter_than_the_sample_yields_only_pages_it_has() {
    assert!(dev_pages(3).iter().all(|page| *page < 3));
    assert!(dev_pages(0).is_empty());
}
