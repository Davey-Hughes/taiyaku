use super::{Plan, PlannedEntry, clipped};

/// Three pages and a metadata document, with the payloads a test can identify.
fn fixture() -> (Plan, Vec<(String, Vec<u8>)>) {
    let payloads: Vec<(String, Vec<u8>)> = vec![
        ("ComicInfo.xml".to_string(), b"<ComicInfo/>".to_vec()),
        ("0001.jpg".to_string(), b"page one bytes".to_vec()),
        ("0002.jpg".to_string(), b"page two".to_vec()),
        ("0003.png".to_string(), vec![0u8; 5000]),
    ];
    let entries = payloads
        .iter()
        .map(|(name, bytes)| PlannedEntry {
            name: name.clone(),
            size: bytes.len() as u64,
            crc32: crc32_of(bytes),
        })
        .collect();
    (Plan::of(entries), payloads)
}

/// The test's own crc, so a bug in the module's cannot hide behind it.
fn crc32_of(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Emit the whole file, the way `download.rs` will.
fn emit(plan: &Plan, payloads: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for (index, (_, bytes)) in payloads.iter().enumerate() {
        out.extend_from_slice(&plan.local_header(index));
        out.extend_from_slice(bytes);
    }
    out.extend_from_slice(&plan.trailer());
    out
}

#[test]
fn the_promised_length_is_the_length_that_comes_out() {
    let (plan, payloads) = fixture();
    let bytes = emit(&plan, &payloads);
    assert_eq!(
        plan.len(),
        bytes.len() as u64,
        "Content-Length would have been a lie"
    );
}

#[test]
fn a_real_zip_reader_gets_the_names_order_sizes_and_crcs_back() {
    let (plan, payloads) = fixture();
    let bytes = emit(&plan, &payloads);

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("parses as a zip");
    assert_eq!(archive.len(), payloads.len());
    for (index, (name, expected)) in payloads.iter().enumerate() {
        let mut entry = archive.by_index(index).expect("entry by position");
        assert_eq!(entry.name(), name, "entry {index} is out of order");
        assert_eq!(entry.compression(), zip::CompressionMethod::Stored);
        assert_eq!(entry.size(), expected.len() as u64);
        assert_eq!(entry.crc32(), crc32_of(expected));
        let mut got = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut got).expect("reads back");
        assert_eq!(&got, expected);
    }
}

#[test]
fn a_payload_span_is_where_that_entrys_bytes_actually_land() {
    let (plan, payloads) = fixture();
    let bytes = emit(&plan, &payloads);
    for (index, (_, expected)) in payloads.iter().enumerate() {
        let (start, end) = plan.payload_span(index);
        assert_eq!(
            &bytes[start as usize..end as usize],
            expected.as_slice(),
            "entry {index}'s span does not point at its bytes"
        );
    }
}

#[test]
fn an_empty_edition_is_still_a_readable_zip() {
    // Not reachable from a real edition — `derive` does not create one and
    // `Edition::total_pages` would be 0 — but a plan of nothing must not
    // produce something a reader chokes on, because the arithmetic for the
    // trailer is the same arithmetic in both cases.
    let plan = Plan::of(Vec::new());
    let bytes = plan.trailer();
    assert_eq!(plan.len(), bytes.len() as u64);
    let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("parses as a zip");
    assert_eq!(archive.len(), 0);
}

/// A plan whose entries are declared enormous, so the zip64 branch is taken.
/// No bytes exist for them and none are produced.
fn oversized() -> Plan {
    Plan::of(vec![
        PlannedEntry {
            name: "0001.jpg".into(),
            size: 3_000_000_000,
            crc32: 1,
        },
        PlannedEntry {
            name: "0002.jpg".into(),
            size: 3_000_000_000,
            crc32: 2,
        },
    ])
}

#[test]
fn a_total_past_four_gibibytes_takes_the_zip64_branch() {
    let plan = oversized();
    assert!(
        plan.zip64(),
        "6 GB of payload cannot be stated in a classic record"
    );
    // Every record grows: the two local extras, the two central extras, the
    // zip64 end record and its locator.
    let classic_would_be = 2 * (30 + 8) + 6_000_000_000 + 2 * (46 + 8) + 22;
    assert_eq!(plan.len(), classic_would_be + 2 * 20 + 2 * 28 + 56 + 20);
}

#[test]
fn one_entry_past_four_gibibytes_takes_it_too() {
    // zip64 is forced here by the *total*, not by a rule about one entry:
    // `directory_at` is `lay_out`'s running sum of
    // `LOCAL_HEADER_FIXED + name.len() + size` over every entry, so a single
    // entry whose size alone exceeds `U32_MAX` already pushes that sum past
    // `U32_MAX` — no dedicated per-entry check is needed, or present, in
    // `Plan::of`.
    let plan = Plan::of(vec![PlannedEntry {
        name: "0001.jpg".into(),
        size: 5_000_000_000,
        crc32: 7,
    }]);
    assert!(plan.zip64());
}

#[test]
fn more_than_sixty_five_thousand_entries_takes_the_zip64_branch_on_count_alone() {
    // 65,536 entries, each with a tiny name and a zero-byte payload: the
    // classic total stays a few megabytes, nowhere near `U32_MAX`, and no
    // entry's size is anywhere near it either. If this test passes, the
    // *count* clause is the only thing that could have forced zip64 — which
    // is exactly the distinction `one_entry_past_four_gibibytes_takes_it_too`
    // cannot make.
    let count: usize = usize::from(u16::MAX) + 1;
    let entries: Vec<PlannedEntry> = (0..count)
        .map(|i| PlannedEntry {
            name: format!("{i:05}.jpg"),
            size: 0,
            crc32: 0,
        })
        .collect();
    let name_bytes: u64 = entries.iter().map(|e| e.name.len() as u64).sum();
    let plan = Plan::of(entries);
    assert!(
        plan.zip64(),
        "65,536 zero-byte entries must still force zip64, on count alone"
    );

    // Pin the layout too, not just the boolean: every local and central
    // record carries its zip64 extra field, and the trailer gains the zip64
    // end record and locator alongside the classic end record.
    let count = count as u64;
    let local = count * (30 + 20) + name_bytes; // LOCAL_HEADER_FIXED + ZIP64_LOCAL_EXTRA, per entry
    let central = count * (46 + 28) + name_bytes; // CENTRAL_HEADER_FIXED + ZIP64_CENTRAL_EXTRA, per entry
    let tail = 56 + 20 + 22; // ZIP64_END_RECORD + ZIP64_LOCATOR + END_RECORD
    assert_eq!(plan.len(), local + central + tail);
}

#[test]
fn a_zip64_payload_span_accounts_for_the_extra_field() {
    let plan = oversized();
    let (start, end) = plan.payload_span(0);
    // 30 fixed + 8 name + 20 zip64 extra.
    assert_eq!(start, 58);
    assert_eq!(end - start, 3_000_000_000);
    let (second, _) = plan.payload_span(1);
    assert_eq!(second, 58 + 3_000_000_000 + 58);
}

#[test]
fn a_zip64_local_header_and_trailer_carry_the_real_sizes_and_offsets() {
    // `a_total_past_four_gibibytes_takes_the_zip64_branch` and
    // `a_zip64_payload_span_accounts_for_the_extra_field` pin the *lengths* of
    // the zip64 records — every other test above asserts `plan.zip64()` or
    // `plan.len()`, never `local_header` or `trailer`. So the `if self.zip64`
    // branches inside those two functions — the classic fields stated as
    // `U32_MAX`/`u16::MAX`, and the extra field's own tag, size and the real
    // sizes and offsets it carries — have never been decoded and checked
    // against APPNOTE 6.3.x. This does that field by field, reading the
    // produced bytes rather than re-deriving their layout a second time.
    let plan = oversized();
    assert!(plan.zip64());

    let header0 = plan.local_header(0);
    assert_eq!(header0.len(), 58, "30 fixed + 8 name + 20 zip64 extra");
    assert_eq!(&header0[0..4], &0x0403_4b50u32.to_le_bytes());
    assert_eq!(
        u16::from_le_bytes(header0[4..6].try_into().unwrap()),
        45,
        "version needed: zip64"
    );
    assert_eq!(u32::from_le_bytes(header0[14..18].try_into().unwrap()), 1);
    assert_eq!(
        u32::from_le_bytes(header0[18..22].try_into().unwrap()),
        u32::MAX,
        "compressed size, classic field: escape value"
    );
    assert_eq!(
        u32::from_le_bytes(header0[22..26].try_into().unwrap()),
        u32::MAX,
        "uncompressed size, classic field: escape value"
    );
    assert_eq!(u16::from_le_bytes(header0[26..28].try_into().unwrap()), 8);
    assert_eq!(
        u16::from_le_bytes(header0[28..30].try_into().unwrap()),
        20,
        "zip64 extra length"
    );
    assert_eq!(&header0[30..38], b"0001.jpg");
    assert_eq!(
        u16::from_le_bytes(header0[38..40].try_into().unwrap()),
        0x0001,
        "zip64 extra tag"
    );
    assert_eq!(
        u16::from_le_bytes(header0[40..42].try_into().unwrap()),
        16,
        "zip64 extra size: two u64s"
    );
    assert_eq!(
        u64::from_le_bytes(header0[42..50].try_into().unwrap()),
        3_000_000_000,
        "the real uncompressed size, in the extra field"
    );
    assert_eq!(
        u64::from_le_bytes(header0[50..58].try_into().unwrap()),
        3_000_000_000,
        "the real compressed size, in the extra field"
    );

    let trailer = plan.trailer();
    // Central-directory record 0: 46 fixed + 8 name + 28 zip64 extra = 82 bytes.
    assert_eq!(&trailer[0..4], &0x0201_4b50u32.to_le_bytes());
    assert_eq!(
        u16::from_le_bytes(trailer[6..8].try_into().unwrap()),
        45,
        "version needed: zip64"
    );
    assert_eq!(
        u32::from_le_bytes(trailer[20..24].try_into().unwrap()),
        u32::MAX,
        "compressed size, classic field: escape value"
    );
    assert_eq!(
        u32::from_le_bytes(trailer[24..28].try_into().unwrap()),
        u32::MAX,
        "uncompressed size, classic field: escape value"
    );
    assert_eq!(
        u16::from_le_bytes(trailer[30..32].try_into().unwrap()),
        28,
        "zip64 central extra length"
    );
    assert_eq!(
        u32::from_le_bytes(trailer[42..46].try_into().unwrap()),
        u32::MAX,
        "local header offset, classic field: escape value"
    );
    assert_eq!(&trailer[46..54], b"0001.jpg");
    assert_eq!(
        u16::from_le_bytes(trailer[54..56].try_into().unwrap()),
        0x0001,
        "zip64 extra tag"
    );
    assert_eq!(
        u16::from_le_bytes(trailer[56..58].try_into().unwrap()),
        24,
        "zip64 extra size: three u64s"
    );
    assert_eq!(
        u64::from_le_bytes(trailer[58..66].try_into().unwrap()),
        3_000_000_000,
        "the real uncompressed size"
    );
    assert_eq!(
        u64::from_le_bytes(trailer[66..74].try_into().unwrap()),
        3_000_000_000,
        "the real compressed size"
    );
    assert_eq!(
        u64::from_le_bytes(trailer[74..82].try_into().unwrap()),
        0,
        "the real local header offset, entry 0"
    );

    // Record 1 starts right after record 0's 82 bytes.
    let record1 = &trailer[82..];
    assert_eq!(&record1[46..54], b"0002.jpg");
    assert_eq!(
        u32::from_le_bytes(record1[42..46].try_into().unwrap()),
        u32::MAX,
        "local header offset, classic field: escape value"
    );
    assert_eq!(
        u64::from_le_bytes(record1[74..82].try_into().unwrap()),
        58 + 3_000_000_000,
        "the real local header offset, entry 1 — right after entry 0's own \
         58-byte local header and 3,000,000,000-byte payload"
    );

    // The zip64 end record and locator, then the classic end record — every
    // one of its fields the escape value that says "see the zip64 records
    // instead", which is what a client actually reads to find them.
    let zip64_end_at = trailer.len() - (56 + 20 + 22);
    let tail = &trailer[zip64_end_at..];
    assert_eq!(
        &tail[0..4],
        &0x0606_4b50u32.to_le_bytes(),
        "zip64 end record signature"
    );
    assert_eq!(
        u64::from_le_bytes(tail[32..40].try_into().unwrap()),
        2,
        "entry count, total"
    );
    let locator = &tail[56..];
    assert_eq!(
        &locator[0..4],
        &0x0706_4b50u32.to_le_bytes(),
        "zip64 locator signature"
    );
    let end = &locator[20..];
    assert_eq!(
        &end[0..4],
        &0x0605_4b50u32.to_le_bytes(),
        "classic end record signature"
    );
    assert_eq!(
        u16::from_le_bytes(end[8..10].try_into().unwrap()),
        u16::MAX,
        "entry count, classic field: escape value"
    );
    assert_eq!(
        u32::from_le_bytes(end[12..16].try_into().unwrap()),
        u32::MAX,
        "directory size, classic field: escape value"
    );
    assert_eq!(
        u32::from_le_bytes(end[16..20].try_into().unwrap()),
        u32::MAX,
        "directory offset, classic field: escape value"
    );
}

#[test]
fn an_ordinary_edition_does_not_take_it() {
    let (plan, _) = fixture();
    assert!(
        !plan.zip64(),
        "nothing in corpus/ reaches this branch — largest group 1.49 GB, 0 of 47 over 4 GiB"
    );
}

#[test]
fn clipped_keeps_only_the_part_inside_the_window() {
    let run = b"abcdefghij";
    // Wholly inside.
    assert_eq!(clipped(run, 100, 0, 1000), run);
    // Trimmed at the front.
    assert_eq!(clipped(run, 100, 103, 1000), b"defghij");
    // Trimmed at the back.
    assert_eq!(clipped(run, 100, 0, 104), b"abcd");
    // Both ends.
    assert_eq!(clipped(run, 100, 102, 105), b"cde");
    // Entirely before the window.
    assert_eq!(clipped(run, 100, 200, 300), b"");
    // Entirely after it.
    assert_eq!(clipped(run, 100, 0, 100), b"");
    // A window that is empty is empty everywhere.
    assert_eq!(clipped(run, 100, 105, 105), b"");
}

// `every_structural_boundary_is_clipped_correctly_on_both_sides` and
// `every_split_of_the_output_reassembles_into_the_whole` used to live here,
// driving `emit_range` — a hand-written second copy of the header/payload/
// trailer walk `download.rs::generated` performs, kept only in this test
// file. Whole-branch review, 2026-08-24 (Finding 3): that duplication is why
// a real off-by-one in `generated`'s own walk (the `plan.local_header(index)`
// placed before its `send` call, one byte off within a range window) passed
// the whole suite — `emit_range` was never mutated, so it kept agreeing with itself regardless
// of what the route actually did. Both tests are now
// `download::tests::a_route_download_matches_the_whole_at_every_local_header_boundary`
// and `download::tests::n_random_ranges_of_a_route_download_reassemble_into_the_whole`,
// driven against the real route so `generated` is the walk's only copy.
// `clipped_keeps_only_the_part_inside_the_window` below still tests the
// clipping primitive directly, and `emit`/the tests that use it still test
// `Plan::local_header`/`Plan::trailer` directly — neither of those is the
// range walk this note is about.
