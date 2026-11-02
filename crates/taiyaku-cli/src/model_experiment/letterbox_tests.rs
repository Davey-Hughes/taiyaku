use super::*;

/// The other padding axis, and the one every real page actually takes.
///
/// The test below pads the **bottom** (`dw == 0`), so it cannot see a
/// regression that centres the *width* only — every value it checks would
/// be identical. Corpus pages are portrait: 685×1024 gives
/// `r = min(1024/1024, 1024/685) = 1`, so `dw = 339` and `dh = 0`, and the
/// padding lands on the **right**. That is the case this pins.
#[test]
fn a_portrait_page_pads_on_the_right_not_around_the_middle() {
    // The real shape of `gotoubun-v01/raw-mono`, uniformly white.
    let page = GrayImage::from_pixel(685, 1024, image::Luma([255]));

    let (tensor, dw, dh) = letterbox(&page);

    assert_eq!(dw, 339, "1024 - 685: the width leaves 339 columns to pad");
    assert_eq!(dh, 0, "the height fills the square exactly");

    let view = tensor
        .to_plain_array_view::<f32>()
        .expect("the tensor is f32");

    // Indices are [batch, channel, y, x]. A centred width would put the
    // page at x = 169..854 and leave this black.
    assert!(
        (view[[0, 0, 0, 0]] - 1.0).abs() < 1e-6,
        "the page starts at the left edge"
    );
    assert!(
        view[[0, 0, 0, 900]].abs() < 1e-6,
        "the padding is on the right, not split left and right"
    );
}

#[test]
fn the_page_goes_top_left_and_the_padding_is_black() {
    // 512 wide, 256 tall, uniformly white. r = min(1024/256, 1024/512) = 2,
    // so it resizes to 1024x512 and pads 512 rows at the bottom.
    let page = GrayImage::from_pixel(512, 256, image::Luma([255]));

    let (tensor, dw, dh) = letterbox(&page);

    assert_eq!(dw, 0, "the width fills the square exactly");
    assert_eq!(dh, 512, "the height leaves half the square to pad");

    // `to_plain_array_view`, not `to_array_view` — the latter is not on
    // `Tensor` (it lives on the plain-view types), and the compiler's
    // suggestion when you reach for it is exactly this method.
    let view = tensor
        .to_plain_array_view::<f32>()
        .expect("the tensor is f32");

    // Top-left is page, and page is white.
    assert!(
        (view[[0, 0, 0, 0]] - 1.0).abs() < 1e-6,
        "the page starts at the top-left corner"
    );
    // A row inside the padding is black — this is what a centred letterbox
    // would get wrong, and it would displace every box by dh/2.
    assert!(
        view[[0, 0, 600, 0]].abs() < 1e-6,
        "the padding is on the bottom, not split top and bottom"
    );
    // All three channels carry the same luma.
    assert!(
        (view[[0, 1, 0, 0]] - view[[0, 0, 0, 0]]).abs() < 1e-6
            && (view[[0, 2, 0, 0]] - view[[0, 0, 0, 0]]).abs() < 1e-6,
        "a mono page fills all three channels alike"
    );
}
