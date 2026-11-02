use super::*;

/// The parameterisation both candidates are expressed in. Identity has to
/// be a genuine no-op, and the offset has to leave the page centre alone —
/// which is the property that keeps the one number the candidates disagree
/// about out of the one they agree on.
#[test]
fn an_axis_is_centred_so_scale_alone_fixes_the_middle() {
    assert!((Axis::IDENTITY.at(0.3) - 0.3).abs() < 1e-12);
    let scaled = Axis {
        scale: 1.2,
        offset: 0.0,
    };
    assert!((scaled.at(0.5) - 0.5).abs() < 1e-12, "the centre is fixed");
    assert!(
        (scaled.at(1.0) - 1.1).abs() < 1e-12,
        "a 20% stretch pushes the bottom edge a tenth of a page off the page"
    );
    assert!(
        (scaled.apart(Axis::IDENTITY, (0.0, 1.0)) - 0.1).abs() < 1e-12,
        "a 20% stretch moves a page edge by a tenth of a page"
    );
}

/// Nearest-rank, so every figure printed is a residual some slot actually
/// had. An interpolating percentile would report one none did.
#[test]
fn the_distribution_reports_values_the_sample_contains() {
    let sample: Vec<f64> = (1..=10).map(f64::from).collect();
    assert_eq!(distribution(&sample), (1.0, 5.0, 9.0, 10.0));
    assert_eq!(distribution(&[3.0]), (3.0, 3.0, 3.0, 3.0));
}

/// Reachable because the tables are taken over the slots whose search
/// settled: a volume with none has nothing to report. NaN in one cell,
/// rather than an index panic that loses the other eleven volumes' run.
#[test]
fn an_empty_sample_reports_nothing_rather_than_panicking() {
    let (lo, med, p90, hi) = distribution(&[]);
    assert!(lo.is_nan() && med.is_nan() && p90.is_nan() && hi.is_nan());
}

/// The sample must not be the front of the volume, which is colour inserts
/// and front matter, and must never run off the end.
#[test]
fn the_sample_is_spread_through_the_volume_rather_than_taken_from_its_front() {
    let items: Vec<usize> = (0..100).collect();
    let picked = spread_sample(&items, 8);
    assert_eq!(picked.len(), 8);
    assert!(picked[0] > 0, "not the very first slot: {picked:?}");
    assert!(picked[7] < 100);
    assert!(picked.windows(2).all(|w| w[0] < w[1]), "{picked:?}");
    // Fewer than asked for is everything, unchanged.
    assert_eq!(spread_sample(&items[..5], 8), items[..5].to_vec());
}

/// A window that would run off the page is refused rather than clamped: a
/// clamped crop is a different window scored as if it were the one asked
/// for, which teaches a grid search to like the edge of its own range.
#[test]
fn a_window_that_leaves_the_page_is_refused_rather_than_clamped() {
    let page = GrayImage::from_fn(200, 300, |x, y| {
        image::Luma([((x * 7 + y * 13) % 251) as u8])
    });
    assert!(window_hash(&page, Axis::IDENTITY, Axis::IDENTITY).is_some());
    let far = Axis {
        scale: 1.0,
        offset: 0.5,
    };
    assert!(window_hash(&page, far, Axis::IDENTITY).is_none());
    assert!(window_hash(&page, Axis::IDENTITY, far).is_none());
}
