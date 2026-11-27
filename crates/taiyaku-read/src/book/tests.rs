use super::*;
use std::{fmt::Write as _, path::PathBuf};

use crate::test_support::TempDir;

/// An EPUB whose package sits at `package_path`, listing `manifest` (id, href,
/// media type) and reading `spine` (ids) in that order, with `files` (archive
/// path, contents) written into the archive in the order given.
struct Epub<'a> {
    package_path: &'a str,
    manifest: &'a [(&'a str, &'a str, &'a str)],
    spine: &'a [&'a str],
    files: &'a [(&'a str, &'a str)],
}

const XHTML: &str = "application/xhtml+xml";

impl Epub<'_> {
    fn write(&self, dir: &Path) -> PathBuf {
        let mut items = String::new();
        for (id, href, media) in self.manifest {
            write!(
                items,
                r#"<item id="{id}" href="{href}" media-type="{media}"/>"#
            )
            .expect("write");
        }
        let mut itemrefs = String::new();
        for id in self.spine {
            write!(itemrefs, r#"<itemref idref="{id}"/>"#).expect("write");
        }
        let package = format!(
            r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title> Beowulf </dc:title><dc:language>en-US</dc:language></metadata>
  <manifest>{items}</manifest>
  <spine>{itemrefs}</spine>
</package>"#
        );
        let container = format!(
            r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="{}" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
            self.package_path
        );

        let path = dir.join("book.epub");
        let file = std::fs::File::create(&path).expect("create");
        let mut zip = zip::ZipWriter::new(file);
        let mut put = |name: &str, text: &str| {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .expect("start");
            std::io::Write::write_all(&mut zip, text.as_bytes()).expect("write");
        };
        put("mimetype", "application/epub+zip");
        put("META-INF/container.xml", &container);
        put(self.package_path, &package);
        for (name, text) in self.files {
            put(name, text);
        }
        zip.finish().expect("finish");
        path
    }
}

fn xhtml(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd">
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title><style>p {{ x: y }}</style></head>
<body>{body}</body></html>"#
    )
}

/// One spine document, `doc.xhtml`, beside the package at the archive root.
fn one_document(dir: &Path, body: &str) -> Book {
    let doc = xhtml(body);
    let path = Epub {
        package_path: "content.opf",
        manifest: &[("d", "doc.xhtml", XHTML)],
        spine: &["d"],
        files: &[("doc.xhtml", &doc)],
    }
    .write(dir);
    read_book(&path).expect("reads")
}

fn texts(book: &Book) -> Vec<(BlockKind, &str)> {
    book.blocks
        .iter()
        .map(|b| (b.kind, b.text.as_str()))
        .collect()
}

#[test]
fn documents_are_read_in_spine_order_not_manifest_or_archive_order() {
    let dir = TempDir::new("book");
    let (one, two, three) = (
        xhtml("<p>one</p>"),
        xhtml("<p>two</p>"),
        xhtml("<p>three</p>"),
    );
    let path = Epub {
        package_path: "content.opf",
        manifest: &[
            ("c", "c.xhtml", XHTML),
            ("a", "a.xhtml", XHTML),
            ("b", "b.xhtml", XHTML),
        ],
        spine: &["a", "b", "c"],
        files: &[("b.xhtml", &two), ("c.xhtml", &three), ("a.xhtml", &one)],
    }
    .write(dir.path());

    let book = read_book(&path).expect("reads");
    let read: Vec<&str> = book.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(read, ["one", "two", "three"]);
}

#[test]
fn headings_are_h1_to_h6_and_a_heading_dressed_as_a_paragraph_stays_one() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        r#"<h2>CHAPTER ONE</h2><h4>THE BOY WHO LIVED</h4>
           <p class="chapter-title"><span class="caps">VI</span></p><p>Text.</p>"#,
    );
    assert_eq!(
        texts(&book),
        [
            (BlockKind::Heading, "CHAPTER ONE"),
            (BlockKind::Heading, "THE BOY WHO LIVED"),
            (BlockKind::Paragraph, "VI"),
            (BlockKind::Paragraph, "Text."),
        ]
    );
}

#[test]
fn inline_markup_joins_its_block_and_whitespace_collapses() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        "<p class=\"first\"><span class=\"drop\">N</span>ot for the\n   first <i>time</i>,\nan argu\u{ad}ment<br/>broke&#160;&#160;out.</p>",
    );
    assert_eq!(
        texts(&book),
        [(
            BlockKind::Paragraph,
            "Not for the first time, an argument broke out."
        )]
    );
}

#[test]
fn an_element_holding_only_no_break_spaces_is_no_block() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        "<div id=\"ch1\">&#160;</div><h2>제 <span>1</span> 장</h2>",
    );
    assert_eq!(texts(&book), [(BlockKind::Heading, "제 1 장")]);
}

#[test]
fn a_nested_block_is_its_own_block_after_its_parents_own_text() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        "<blockquote>Addressed: <p>Mr. H. Potter</p><p>The Cupboard under the Stairs</p></blockquote><p>After.</p>",
    );
    assert_eq!(
        texts(&book),
        [
            (BlockKind::Paragraph, "Addressed:"),
            (BlockKind::Paragraph, "Mr. H. Potter"),
            (BlockKind::Paragraph, "The Cupboard under the Stairs"),
            (BlockKind::Paragraph, "After."),
        ]
    );
}

#[test]
fn loose_text_between_paragraphs_is_a_block_of_its_own_in_order() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        "<h3>PART ONE</h3>All happy families.<p>Everything was confusion.</p>On the third day<p>Next.</p>",
    );
    assert_eq!(
        texts(&book),
        [
            (BlockKind::Heading, "PART ONE"),
            (BlockKind::Paragraph, "All happy families."),
            (BlockKind::Paragraph, "Everything was confusion."),
            (BlockKind::Paragraph, "On the third day"),
            (BlockKind::Paragraph, "Next."),
        ]
    );
}

#[test]
fn a_heading_holding_a_nested_block_keeps_its_text_on_both_sides_a_heading() {
    let dir = TempDir::new("book");
    let book = one_document(dir.path(), "<h2>CHAPTER <div>ONE</div> ENDS</h2>");
    assert_eq!(
        texts(&book),
        [
            (BlockKind::Heading, "CHAPTER"),
            (BlockKind::Paragraph, "ONE"),
            (BlockKind::Heading, "ENDS"),
        ]
    );
}

#[test]
fn script_and_head_text_is_never_read() {
    let dir = TempDir::new("book");
    let book = one_document(dir.path(), "<script>var x = 1;</script><p>Kept.</p>");
    assert_eq!(texts(&book), [(BlockKind::Paragraph, "Kept.")]);
}

#[test]
fn ids_name_the_document_and_count_only_the_blocks_it_kept() {
    let dir = TempDir::new("book");
    let (first, second) = (xhtml("<p>a</p><div> </div><p>b</p>"), xhtml("<p>c</p>"));
    let path = Epub {
        package_path: "OEBPS/content.opf",
        manifest: &[
            ("one", "text/one.xhtml", XHTML),
            ("two", "text/two.xhtml", XHTML),
        ],
        spine: &["one", "two"],
        files: &[
            ("OEBPS/text/one.xhtml", &first),
            ("OEBPS/text/two.xhtml", &second),
        ],
    }
    .write(dir.path());

    let ids: Vec<(String, usize)> = read_book(&path)
        .expect("reads")
        .blocks
        .into_iter()
        .map(|b| (b.id.document, b.id.ordinal))
        .collect();
    assert_eq!(
        ids,
        [
            ("OEBPS/text/one.xhtml".to_string(), 0),
            ("OEBPS/text/one.xhtml".to_string(), 1),
            ("OEBPS/text/two.xhtml".to_string(), 0),
        ]
    );
}

#[test]
fn an_href_is_resolved_against_the_packages_directory_through_dot_dot_and_escapes() {
    let dir = TempDir::new("book");
    let doc = xhtml("<p>found</p>");
    let path = Epub {
        package_path: "epub/content.opf",
        manifest: &[("d", "../text/chapter%201.xhtml#start", XHTML)],
        spine: &["d"],
        files: &[("text/chapter 1.xhtml", &doc)],
    }
    .write(dir.path());

    let book = read_book(&path).expect("reads");
    assert_eq!(book.blocks[0].id.document, "text/chapter 1.xhtml");
    assert_eq!(book.blocks[0].text, "found");
}

#[test]
fn an_href_climbing_out_of_the_archive_is_refused() {
    let dir = TempDir::new("book");
    let path = Epub {
        package_path: "content.opf",
        manifest: &[("d", "../outside.xhtml", XHTML)],
        spine: &["d"],
        files: &[],
    }
    .write(dir.path());

    let error = read_book(&path).expect_err("refused");
    assert!(error.contains("climbs out of the archive"), "{error}");
}

#[test]
fn a_spine_item_that_is_not_a_document_is_passed_over() {
    let dir = TempDir::new("book");
    let doc = xhtml("<p>text</p>");
    let path = Epub {
        package_path: "content.opf",
        manifest: &[
            ("img", "cover.jpg", "image/jpeg"),
            ("d", "doc.xhtml", XHTML),
        ],
        spine: &["img", "d"],
        files: &[("doc.xhtml", &doc)],
    }
    .write(dir.path());

    assert_eq!(
        texts(&read_book(&path).expect("reads")),
        [(BlockKind::Paragraph, "text")]
    );
}

#[test]
fn a_document_that_is_not_well_formed_is_refused_by_name() {
    let dir = TempDir::new("book");
    let path = Epub {
        package_path: "content.opf",
        manifest: &[("d", "broken.xhtml", XHTML)],
        spine: &["d"],
        files: &[("broken.xhtml", "<html><body><p>unclosed</body></html>")],
    }
    .write(dir.path());

    let error = read_book(&path).expect_err("refused");
    assert!(error.contains("broken.xhtml"), "{error}");
    assert!(error.contains("book.epub"), "{error}");
}

#[test]
fn a_spine_id_the_manifest_lacks_is_refused_by_name() {
    let dir = TempDir::new("book");
    let path = Epub {
        package_path: "content.opf",
        manifest: &[],
        spine: &["ghost"],
        files: &[],
    }
    .write(dir.path());

    let error = read_book(&path).expect_err("refused");
    assert!(error.contains("`ghost`"), "{error}");
}

#[test]
fn a_zip_with_no_container_is_refused() {
    let dir = TempDir::new("book");
    let path = dir.path().join("not-an-epub.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).expect("create"));
    zip.start_file("page.png", zip::write::SimpleFileOptions::default())
        .expect("start");
    zip.finish().expect("finish");

    let error = read_book(&path).expect_err("refused");
    assert!(error.contains("META-INF/container.xml"), "{error}");
}

#[test]
fn the_packages_title_and_language_are_read() {
    let dir = TempDir::new("book");
    let book = one_document(dir.path(), "<p>x</p>");
    assert_eq!(book.title.as_deref(), Some("Beowulf"));
    assert_eq!(book.language.as_deref(), Some("en-US"));
}

#[test]
fn a_package_with_no_metadata_has_no_title_or_language() {
    let dir = TempDir::new("book");
    let book = read_book(&crate::test_support::epub(dir.path(), "b.epub", &["x"])).unwrap();
    assert_eq!((book.title, book.language), (None, None));
}

#[test]
fn percent_decoding_refuses_a_truncated_or_non_hex_escape() {
    assert_eq!(percent_decode("a%20b").as_deref(), Some("a b"));
    assert_eq!(percent_decode("a%2"), None);
    assert_eq!(percent_decode("a%zzb"), None);
    assert_eq!(percent_decode("a%+fb"), None, "a sign is no hex digit");
}

#[test]
fn ruby_glosses_are_not_read_into_the_text() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        "<p><ruby>漢<rp>(</rp><rt>かん</rt><rp>)</rp></ruby>字を読む。</p>",
    );
    assert_eq!(book.blocks[0].text, "漢字を読む。");
}

#[test]
fn xhtmls_named_references_read_as_their_characters() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        "<p>A&nbsp;B&mdash;C caf&eacute; &amp; &lt;b&gt;</p>",
    );
    // The no-break space is whitespace, collapsed with the rest.
    assert_eq!(book.blocks[0].text, "A B—C café & <b>");
}

#[test]
fn a_named_reference_no_xhtml_dtd_declares_is_refused_by_name() {
    // And a known name without its `;`, which is no reference at all.
    for body in ["<p>&bogus;</p>", "<p>&nbsp x</p>"] {
        let dir = TempDir::new("book");
        let doc = xhtml(body);
        let path = Epub {
            package_path: "content.opf",
            manifest: &[("d", "doc.xhtml", XHTML)],
            spine: &["d"],
            files: &[("doc.xhtml", &doc)],
        }
        .write(dir.path());

        let error = read_book(&path).expect_err(body);
        assert!(error.contains("doc.xhtml"), "{body}: {error}");
    }
}

fn lines(book: &Book) -> Vec<(&str, Option<u32>)> {
    book.blocks
        .iter()
        .map(|b| (b.text.as_str(), b.line))
        .collect()
}

/** Lines `first..=last` of a poem, `<p>` each, the text `line N`. */
fn verse(first: u32, last: u32) -> String {
    (first..=last).map(|n| format!("<p>line {n}</p>")).collect()
}

#[test]
fn a_paragraph_of_a_number_alone_numbers_the_next_line_and_is_no_block() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        &format!(
            "{}<div>10</div>{}<div>15</div>{}",
            verse(6, 9),
            verse(10, 14),
            verse(15, 16)
        ),
    );
    let numbered: Vec<_> = lines(&book)
        .into_iter()
        .filter(|(_, n)| n.is_some())
        .collect();
    assert_eq!(numbered, [("line 10", Some(10)), ("line 15", Some(15))]);
    assert_eq!(book.blocks.len(), 11, "no block of a number alone");
}

#[test]
fn a_verse_line_opening_on_its_number_has_it_as_its_line() {
    // Lattimore sets the number before a line break, Fitzgerald repeats the
    // number standing alone at the line's start; either is the line's.
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        &format!(
            "<p>335 <br/>Then resourceful Odysseus answered her:</p>{}\
             <p>340 I will lie now</p><div>345</div><p>345 as I have before</p>",
            verse(336, 339)
        ),
    );
    let numbered: Vec<_> = lines(&book)
        .into_iter()
        .filter(|(_, n)| n.is_some())
        .collect();
    assert_eq!(
        numbered,
        [
            ("Then resourceful Odysseus answered her:", Some(335)),
            ("I will lie now", Some(340)),
            ("as I have before", Some(345)),
        ]
    );
}

#[test]
fn a_number_no_other_bears_out_is_the_text_s_own() {
    // An address, a list of one of each, a year: no second number counts the
    // lines between, so none is a line's number.
    let dir = TempDir::new("book");
    let long = format!(
        "1883 {}",
        "was the year he was born, and the rest of a long paragraph. ".repeat(3)
    );
    let book = one_document(
        dir.path(),
        &format!(
            "<h2>1</h2><p>4 Privet Drive</p><p>1 wand</p><p>1 cauldron</p>\
             <p>1919 A NOTE BY THE TRANSLATOR</p><p>{long}</p><p>978014</p>"
        ),
    );
    assert_eq!(
        lines(&book),
        [
            ("1", None),
            ("4 Privet Drive", None),
            ("1 wand", None),
            ("1 cauldron", None),
            ("1919 A NOTE BY THE TRANSLATOR", None),
            (long.trim(), None),
            ("978014", None),
        ]
    );
    assert_eq!(book.blocks[0].kind, BlockKind::Heading);
}

/// Each mark of `block`, as the text it covers.
fn marked(block: &Block) -> Vec<(&MarkKind, &str)> {
    block
        .marks
        .iter()
        .map(|m| (&m.kind, &block.text[m.range.clone()]))
        .collect()
}

#[test]
fn inline_tags_mark_their_text() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        "<p>A <i>big</i> <b>bold</b> x<sup>2</sup> H<sub>2</sub>O <em>e</em> \
         <strong>s</strong> <cite>c</cite></p>",
    );
    let block = &book.blocks[0];
    assert_eq!(block.text, "A big bold x2 H2O e s c");
    assert_eq!(
        marked(block),
        [
            (&MarkKind::Italic, "big"),
            (&MarkKind::Bold, "bold"),
            (&MarkKind::Sup, "2"),
            (&MarkKind::Sub, "2"),
            (&MarkKind::Italic, "e"),
            (&MarkKind::Bold, "s"),
            (&MarkKind::Italic, "c"),
        ]
    );
    assert_eq!(block.marks[2].range, 12..13, "the first 2, in x2");
    assert_eq!(block.marks[3].range, 15..16, "the second 2, in H2O");
}

#[test]
fn marks_survive_collapsing_whitespace() {
    let dir = TempDir::new("book");
    let book = one_document(dir.path(), "<p>  one\n <i> two  three </i>  four</p>");
    let block = &book.blocks[0];
    assert_eq!(block.text, "one two three four");
    assert_eq!(marked(block), [(&MarkKind::Italic, "two three")]);
}

#[test]
fn marks_follow_the_verse_number_cut() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        &format!(
            "<p>5 Sing <i>goddess</i></p>{}<p>10 the wrath</p>",
            verse(6, 9)
        ),
    );
    let block = &book.blocks[0];
    assert_eq!(block.text, "Sing goddess");
    assert_eq!(block.line, Some(5));
    assert_eq!(marked(block), [(&MarkKind::Italic, "goddess")]);
}

#[test]
fn a_mark_over_a_cut_verse_number_is_clipped_or_dropped() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        &format!(
            "<p><b>5</b> Sing <i>goddess</i></p>{}<p>10 the wrath</p>",
            verse(6, 9)
        ),
    );
    let block = &book.blocks[0];
    assert_eq!(block.text, "Sing goddess");
    assert_eq!(block.line, Some(5));
    assert_eq!(marked(block), [(&MarkKind::Italic, "goddess")]);

    let book = one_document(
        dir.path(),
        &format!(
            "<p><i>5 Sing</i> goddess</p>{}<p>10 the wrath</p>",
            verse(6, 9)
        ),
    );
    let block = &book.blocks[0];
    assert_eq!(block.text, "Sing goddess");
    assert_eq!(marked(block), [(&MarkKind::Italic, "Sing")]);
}

#[test]
fn nested_marks_are_each_kept() {
    let dir = TempDir::new("book");
    let book = one_document(dir.path(), "<p><b>a <i>b</i></b></p>");
    assert_eq!(
        marked(&book.blocks[0]),
        [(&MarkKind::Bold, "a b"), (&MarkKind::Italic, "b")]
    );
}

#[test]
fn a_mark_over_nothing_is_dropped() {
    let dir = TempDir::new("book");
    let book = one_document(dir.path(), "<p>x<i> </i>y</p>");
    assert_eq!(book.blocks[0].text, "x y");
    assert_eq!(book.blocks[0].marks, Vec::<Mark>::new());
}

#[test]
fn a_mark_around_blocks_marks_each_block_it_holds() {
    let dir = TempDir::new("book");
    let book = one_document(dir.path(), "<div><i>a<p>b</p>c</i></div>");
    let read: Vec<_> = book.blocks.iter().map(marked).collect();
    assert_eq!(
        read,
        [
            [(&MarkKind::Italic, "a")],
            [(&MarkKind::Italic, "b")],
            [(&MarkKind::Italic, "c")],
        ]
    );
}

#[test]
fn ruby_marks_its_base_with_its_reading() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        "<p><ruby>坊<rp>(</rp><rt>ぼっ</rt><rp>)</rp></ruby>ちゃん</p>",
    );
    let block = &book.blocks[0];
    assert_eq!(block.text, "坊ちゃん");
    assert_eq!(
        marked(block),
        [(
            &MarkKind::Ruby {
                reading: "ぼっ".into()
            },
            "坊"
        )]
    );
}

#[test]
fn each_base_in_one_ruby_has_its_own_reading() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        "<p><ruby>漢<rt>かん</rt>字<rt>じ</rt></ruby></p>",
    );
    assert_eq!(
        marked(&book.blocks[0]),
        [
            (
                &MarkKind::Ruby {
                    reading: "かん".into()
                },
                "漢"
            ),
            (
                &MarkKind::Ruby {
                    reading: "じ".into()
                },
                "字"
            ),
        ]
    );
}

#[test]
fn rb_elements_are_bases() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        "<p><ruby><rb>東京</rb><rt>とうきょう</rt></ruby></p>",
    );
    assert_eq!(
        marked(&book.blocks[0]),
        [(
            &MarkKind::Ruby {
                reading: "とうきょう".into()
            },
            "東京"
        )]
    );
}

#[test]
fn a_ruby_with_no_reading_is_no_mark() {
    let dir = TempDir::new("book");
    let book = one_document(dir.path(), "<p><ruby>字<rt></rt></ruby></p>");
    assert_eq!(book.blocks[0].text, "字");
    assert!(book.blocks[0].marks.is_empty());
}

#[test]
fn marks_hold_over_multibyte_text_and_a_no_break_space() {
    let dir = TempDir::new("book");
    let book = one_document(dir.path(), "<p>日本&#160;<i>語</i></p>");
    let block = &book.blocks[0];
    assert_eq!(block.text, "日本 語");
    assert_eq!(marked(block), [(&MarkKind::Italic, "語")]);
}

#[test]
fn collapsing_maps_every_raw_offset() {
    let raw = " a\u{ad}b  c ";
    let (text, at) = collapse_mapped(raw);
    assert_eq!(text, "ab c");
    assert_eq!(at.len(), raw.len() + 1);
    assert_eq!(at[raw.find('a').unwrap()], 0);
    assert_eq!(at[raw.find('b').unwrap()], 1);
    assert_eq!(at[raw.find('c').unwrap()], 3);
    assert_eq!(at[raw.len()], text.len());
}

#[test]
fn numbering_maps_each_block_to_where_it_went() {
    let walked: Vec<Walked> = ["line 1", "2", "line 2", "line 3", "4", "line 4"]
        .into_iter()
        .map(|text| Walked {
            kind: BlockKind::Paragraph,
            text: text.to_string(),
            marks: Vec::new(),
        })
        .collect();
    let (out, at) = numbered(walked);
    assert_eq!(out.len(), 4);
    assert_eq!(at, [0, 1, 1, 2, 3, 3]);
}

/// One chapter at `text/ch.xhtml` whose `<head>` holds `head`, with `files`
/// beside it, read as a book.
fn styled(dir: &Path, head: &str, body: &str, files: &[(&str, &[u8])]) -> Book {
    let doc = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title>{head}</head><body>{body}</body></html>"#
    );
    let path = Epub {
        package_path: "content.opf",
        manifest: &[("d", "text/ch.xhtml", XHTML)],
        spine: &["d"],
        files: &[("text/ch.xhtml", &doc)],
    }
    .write(dir);
    if files.is_empty() {
        return read_book(&path).expect("reads");
    }
    let mut zip = zip::ZipWriter::new_append(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open"),
    )
    .expect("append");
    for (name, bytes) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .expect("start");
        std::io::Write::write_all(&mut zip, bytes).expect("write");
    }
    zip.finish().expect("finish");
    read_book(&path).expect("reads")
}

const STYLESHEET: &str = r#"<link rel="stylesheet" href="../css/style.css"/>"#;

#[test]
fn a_class_a_stylesheet_sets_italic_marks_italic() {
    let dir = TempDir::new("book");
    let book = styled(
        dir.path(),
        STYLESHEET,
        r#"<p>a <span class="it">word</span> b</p>"#,
        &[("css/style.css", b".it { font-style: italic }")],
    );
    assert_eq!(marked(&book.blocks[0]), [(&MarkKind::Italic, "word")]);
}

#[test]
fn an_inline_style_element_counts() {
    let dir = TempDir::new("book");
    let book = styled(
        dir.path(),
        "<style>.it { font-style: italic }</style>",
        r#"<p>a <span class="it">word</span> b</p>"#,
        &[],
    );
    assert_eq!(marked(&book.blocks[0]), [(&MarkKind::Italic, "word")]);
}

#[test]
fn normal_inside_italic_ends_it() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        r#"<p><i>a <span style="font-style:normal">b</span> c</i></p>"#,
    );
    assert_eq!(
        marked(&book.blocks[0]),
        [(&MarkKind::Italic, "a"), (&MarkKind::Italic, "c")]
    );
}

#[test]
fn a_tag_inside_the_same_state_adds_no_second_mark() {
    let dir = TempDir::new("book");
    let book = styled(
        dir.path(),
        "<style>.it { font-style: italic }</style>",
        r#"<p><span class="it">a <i>b</i> c</span></p>"#,
        &[],
    );
    assert_eq!(marked(&book.blocks[0]), [(&MarkKind::Italic, "a b c")]);
}

#[test]
fn a_class_on_a_block_marks_the_text_it_holds() {
    let dir = TempDir::new("book");
    let book = styled(
        dir.path(),
        "<style>p.q { font-weight: bold }</style>",
        r#"<p class="q">one</p><p>two</p>"#,
        &[],
    );
    assert_eq!(marked(&book.blocks[0]), [(&MarkKind::Bold, "one")]);
    assert!(book.blocks[1].marks.is_empty());
}

#[test]
fn a_missing_or_unreadable_stylesheet_leaves_the_book_readable() {
    let dir = TempDir::new("book");
    let head = r#"<link rel="stylesheet" href="../css/absent.css"/>
        <link rel="stylesheet" href="../css/bad.css"/>"#;
    let body = r#"<p>a <span class="it">word</span> b</p>"#;
    let book = styled(
        dir.path(),
        head,
        body,
        &[("css/bad.css", &[0xff, 0xfe, 0x2e, 0x69, 0x74])],
    );
    assert_eq!(book.blocks[0].text, "a word b");
    assert!(book.blocks[0].marks.is_empty());
}

/// A book of two spine documents, `text.xhtml` then `notes.xhtml`, with
/// `outside.xhtml` in the manifest but not the spine; each body may use
/// `epub:type`.
fn linked(dir: &Path, text: &str, notes: &str) -> Book {
    let page = |body: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>t</title></head><body>{body}</body></html>"#
        )
    };
    let (text, notes, outside) = (page(text), page(notes), page("<p id=\"o\">Out.</p>"));
    let path = Epub {
        package_path: "content.opf",
        manifest: &[
            ("t", "text.xhtml", XHTML),
            ("n", "notes.xhtml", XHTML),
            ("o", "outside.xhtml", XHTML),
        ],
        spine: &["t", "n"],
        files: &[
            ("text.xhtml", &text),
            ("notes.xhtml", &notes),
            ("outside.xhtml", &outside),
        ],
    }
    .write(dir);
    read_book(&path).expect("reads")
}

/// Each link in `book`, as the text it covers, where it goes and whether it
/// is a note.
fn links(book: &Book) -> Vec<(&str, &LinkTarget, bool)> {
    book.blocks
        .iter()
        .flat_map(|block| {
            block.marks.iter().filter_map(|m| match &m.kind {
                MarkKind::Link { to, note } => Some((&block.text[m.range.clone()], to, *note)),
                _ => None,
            })
        })
        .collect()
}

/// The index of the block whose text is `text`.
fn block_of(book: &Book, text: &str) -> usize {
    book.blocks
        .iter()
        .position(|b| b.text == text)
        .expect("a block holds the text")
}

#[test]
fn a_noteref_links_to_its_note_block_and_is_a_note() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p>Text<a epub:type="noteref" href="notes.xhtml#n1">1</a>.</p>"#,
        r#"<aside id="n1" epub:type="footnote"><p>The note.</p></aside>"#,
    );
    let note = block_of(&book, "The note.");
    assert_eq!(links(&book), [("1", &LinkTarget::Block(note), true)]);
}

#[test]
fn a_sup_link_is_a_note() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r##"<p>A<sup><a href="notes.xhtml#n2">2</a></sup> b<a href="#n3"><sup>3</sup></a></p><p id="n3">Three.</p>"##,
        r#"<p id="n2">Two.</p>"#,
    );
    assert_eq!(
        links(&book),
        [
            ("2", &LinkTarget::Block(block_of(&book, "Two.")), true),
            ("3", &LinkTarget::Block(block_of(&book, "Three.")), true),
        ]
    );
}

#[test]
fn a_link_into_an_endnote_section_is_a_note() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p>See <a href="notes.xhtml#e1">this</a>.</p>"#,
        r#"<section epub:type="endnotes"><ol><li id="e1"><p>An endnote.</p></li></ol></section>"#,
    );
    let note = block_of(&book, "An endnote.");
    assert_eq!(links(&book), [("this", &LinkTarget::Block(note), true)]);
}

#[test]
fn a_plain_in_book_link_is_not_a_note() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p>Go and <a href="text.xhtml#ch2">see chapter 2</a>.</p><h2 id="ch2">Chapter 2</h2>"#,
        "<p>Notes.</p>",
    );
    let heading = block_of(&book, "Chapter 2");
    assert_eq!(
        links(&book),
        [("see chapter 2", &LinkTarget::Block(heading), false)]
    );
}

#[test]
fn an_id_on_an_inline_element_targets_its_paragraph() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r##"<p><a href="notes.xhtml#x">go</a></p>"##,
        r#"<p>Before.</p><p>Lead <span id="x">here</span></p>"#,
    );
    let paragraph = block_of(&book, "Lead here");
    assert_eq!(links(&book), [("go", &LinkTarget::Block(paragraph), false)]);
}

#[test]
fn a_link_to_a_document_alone_targets_its_first_block() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p><a href="notes.xhtml">the notes</a></p>"#,
        "<h1>Notes</h1><p>One.</p>",
    );
    let first = block_of(&book, "Notes");
    assert_eq!(
        links(&book),
        [("the notes", &LinkTarget::Block(first), false)]
    );
}

#[test]
fn a_web_link_is_a_url() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p><a href="https://example.org/a">web</a> <a href="mailto:a@example.org">mail</a> <a href="javascript:void(0)">script</a></p>"#,
        "<p>Notes.</p>",
    );
    assert_eq!(
        links(&book),
        [
            (
                "web",
                &LinkTarget::Url("https://example.org/a".into()),
                false
            ),
            (
                "mail",
                &LinkTarget::Url("mailto:a@example.org".into()),
                false
            ),
        ]
    );
}

#[test]
fn an_unresolvable_link_is_no_mark() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r##"<p><a href="notes.xhtml#absent">a</a> <a href="outside.xhtml#o">b</a> <a href="../../text.xhtml#t">c</a> <a href="#">d</a></p>"##,
        "<p>Notes.</p>",
    );
    assert_eq!(book.blocks[0].text, "a b c d");
    assert_eq!(links(&book), []);
}

#[test]
fn a_link_to_a_dropped_verse_number_goes_to_the_next_block() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p><a href="notes.xhtml#l10">line ten</a></p>"#,
        &format!(
            "{}<p id=\"l10\">10</p>{}<p>15</p>{}",
            verse(6, 9),
            verse(10, 14),
            verse(15, 16)
        ),
    );
    assert!(
        book.blocks.iter().all(|b| b.text != "10"),
        "the number is cut"
    );
    let line = block_of(&book, "line 10");
    assert_eq!(
        links(&book),
        [("line ten", &LinkTarget::Block(line), false)]
    );
}

#[test]
fn links_into_note_and_notes_and_footnotes_sections_are_notes() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p><a href="notes.xhtml#a">a</a> <a href="notes.xhtml#b">b</a> <a href="notes.xhtml#c">c</a></p>"#,
        r#"<aside id="a" epub:type="note"><p>Note.</p></aside>
           <section epub:type="notes"><p id="b">Notes.</p></section>
           <section epub:type="footnotes"><p id="c">Footnotes.</p></section>"#,
    );
    assert_eq!(
        links(&book),
        [
            ("a", &LinkTarget::Block(block_of(&book, "Note.")), true),
            ("b", &LinkTarget::Block(block_of(&book, "Notes.")), true),
            ("c", &LinkTarget::Block(block_of(&book, "Footnotes.")), true),
        ]
    );
}

#[test]
fn a_link_to_a_notes_section_itself_is_no_note() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p><a href="notes.xhtml#notes">notes</a> <a href="notes.xhtml#one">one</a></p>"#,
        r#"<section id="notes" epub:type="endnotes"><h2>Notes</h2>
           <aside id="one" epub:type="endnote"><p>One.</p></aside></section>"#,
    );
    assert_eq!(
        links(&book),
        [
            ("notes", &LinkTarget::Block(block_of(&book, "Notes")), false),
            ("one", &LinkTarget::Block(block_of(&book, "One.")), true),
        ]
    );
}

#[test]
fn a_notes_lists_headings_are_no_notes_though_the_notes_under_them_are() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p><a href="notes.xhtml#title">title</a> <a href="notes.xhtml#part">part</a> <a href="notes.xhtml#n1">one</a> <a href="notes.xhtml#n2">two</a></p>"#,
        r#"<section epub:type="endnotes"><h2 id="title">Notes</h2><h3><span id="part">Part</span> 1</h3>
           <aside id="n1" epub:type="endnote"><p>One.</p></aside>
           <aside epub:type="endnote"><h4 id="n2">Two</h4><p>Said.</p></aside></section>"#,
    );
    assert_eq!(
        links(&book),
        [
            ("title", &LinkTarget::Block(block_of(&book, "Notes")), false),
            ("part", &LinkTarget::Block(block_of(&book, "Part 1")), false),
            ("one", &LinkTarget::Block(block_of(&book, "One.")), true),
            ("two", &LinkTarget::Block(block_of(&book, "Two")), true),
        ]
    );
}

#[test]
fn a_heading_inside_a_note_of_a_notes_list_is_the_notes_typed_or_not() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p id="r1">See<a href="notes.xhtml#n1"><sup>1</sup></a> <a href="notes.xhtml#n2">two</a> <a href="notes.xhtml#n3">three</a> <a href="notes.xhtml#top">top</a>.</p>"#,
        r#"<section epub:type="endnotes"><header><h2 id="top">Notes</h2></header><ol>
           <li id="n1"><h4><a href="text.xhtml#r1"><sup>1</sup></a></h4><p>One.</p></li>
           <li><h4 id="n2">Two</h4><p>Said.</p></li></ol>
           <aside epub:type="endnote"><h4 id="n3"><a href="text.xhtml#r1"><sup>3</sup></a> Three</h4></aside></section>"#,
    );
    let text = LinkTarget::Block(block_of(&book, "See1 two three top."));
    assert_eq!(
        links(&book),
        [
            ("1", &LinkTarget::Block(block_of(&book, "1")), true),
            ("two", &LinkTarget::Block(block_of(&book, "Two")), true),
            (
                "three",
                &LinkTarget::Block(block_of(&book, "3 Three")),
                true
            ),
            ("top", &LinkTarget::Block(block_of(&book, "Notes")), false),
            ("1", &text, false),
            ("3", &text, false),
        ]
    );
}

#[test]
fn a_heading_of_a_group_of_notes_and_the_group_itself_are_no_notes() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p><a href="notes.xhtml#notes">notes</a> <a href="notes.xhtml#c1">c1</a> <a href="notes.xhtml#g2">g2</a> <a href="notes.xhtml#n1">one</a> <a href="notes.xhtml#n2">two</a></p>"#,
        r#"<section epub:type="endnotes"><div class="title"><h1 id="notes">Notes</h1></div>
           <section><h3 id="c1">Chapter 1</h3><ol><li id="n1"><p>One.</p></li></ol></section>
           <section id="g2"><h3>Chapter 2</h3><div id="n2"><p>Two.</p></div></section></section>"#,
    );
    assert_eq!(
        links(&book),
        [
            ("notes", &LinkTarget::Block(block_of(&book, "Notes")), false),
            (
                "c1",
                &LinkTarget::Block(block_of(&book, "Chapter 1")),
                false
            ),
            (
                "g2",
                &LinkTarget::Block(block_of(&book, "Chapter 2")),
                false
            ),
            ("one", &LinkTarget::Block(block_of(&book, "One.")), true),
            ("two", &LinkTarget::Block(block_of(&book, "Two.")), true),
        ]
    );
}

#[test]
fn a_link_back_out_of_a_note_is_a_plain_jump_whatever_its_markup() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p id="ref1">Text<a epub:type="noteref" href="notes.xhtml#n1"><sup>1</sup></a>.</p>"#,
        r##"<aside id="n1" epub:type="footnote"><p><a href="text.xhtml#ref1"><sup>1</sup></a> The note, <a epub:type="noteref" href="#n2">2</a>.</p></aside>
           <aside id="n2" epub:type="footnote"><p>Two.</p></aside>"##,
    );
    assert_eq!(
        links(&book),
        [
            (
                "1",
                &LinkTarget::Block(block_of(&book, "1 The note, 2.")),
                true
            ),
            ("1", &LinkTarget::Block(block_of(&book, "Text1.")), false),
            ("2", &LinkTarget::Block(block_of(&book, "Two.")), true),
        ]
    );
}

#[test]
fn an_id_on_the_body_targets_its_documents_first_block() {
    let dir = TempDir::new("book");
    let text = xhtml(r#"<p><a href="b.xhtml#top">top</a></p>"#);
    let other = xhtml("<p>First.</p><p>Second.</p>").replace("<body>", r#"<body id="top">"#);
    let path = Epub {
        package_path: "content.opf",
        manifest: &[("a", "a.xhtml", XHTML), ("b", "b.xhtml", XHTML)],
        spine: &["a", "b"],
        files: &[("a.xhtml", &text), ("b.xhtml", &other)],
    }
    .write(dir.path());
    let book = read_book(&path).expect("reads");
    let first = block_of(&book, "First.");
    assert_eq!(links(&book), [("top", &LinkTarget::Block(first), false)]);
}

#[test]
fn a_links_href_resolves_against_its_own_documents_directory() {
    let dir = TempDir::new("book");
    let chapter =
        xhtml(r#"<p><a href="../notes/n.xhtml#x">see</a> <a href="n.xhtml#x">not here</a></p>"#);
    let notes = xhtml(r#"<p>Before.</p><p id="x">The target.</p>"#);
    let path = Epub {
        package_path: "OEBPS/content.opf",
        manifest: &[
            ("c", "text/ch1.xhtml", XHTML),
            ("n", "notes/n.xhtml", XHTML),
        ],
        spine: &["c", "n"],
        files: &[
            ("OEBPS/text/ch1.xhtml", &chapter),
            ("OEBPS/notes/n.xhtml", &notes),
        ],
    }
    .write(dir.path());
    let book = read_book(&path).expect("reads");
    assert_eq!(book.blocks[2].id.document, "OEBPS/notes/n.xhtml");
    let target = block_of(&book, "The target.");
    assert_eq!(links(&book), [("see", &LinkTarget::Block(target), false)]);
}

/// One spine document, `text/doc.xhtml`, a directory below the package.
fn in_text_directory(dir: &Path, body: &str) -> Book {
    let doc = xhtml(body);
    let path = Epub {
        package_path: "content.opf",
        manifest: &[("d", "text/doc.xhtml", XHTML)],
        spine: &["d"],
        files: &[("text/doc.xhtml", &doc)],
    }
    .write(dir);
    read_book(&path).expect("reads")
}

/// Each inline image in `book`: its block's text, where it stands, its path
/// and its alt text.
fn images(book: &Book) -> Vec<(&str, std::ops::Range<usize>, &str, &str)> {
    book.blocks
        .iter()
        .flat_map(|block| {
            block.marks.iter().filter_map(|m| match &m.kind {
                MarkKind::Image { src, alt, .. } => Some((
                    block.text.as_str(),
                    m.range.clone(),
                    src.as_str(),
                    alt.as_str(),
                )),
                _ => None,
            })
        })
        .collect()
}

fn figure(before: usize, src: &str, alt: &str) -> Figure {
    Figure {
        before,
        src: src.to_string(),
        alt: alt.to_string(),
        link: None,
        page: false,
    }
}

/// [`figure`], the whole of its spine document.
fn page(before: usize, src: &str, alt: &str) -> Figure {
    Figure {
        page: true,
        ..figure(before, src, alt)
    }
}

/// Each inline image in `book` by its path, and where its link goes and
/// whether it calls a note.
fn image_links(book: &Book) -> Vec<(&str, Option<(&LinkTarget, bool)>)> {
    book.blocks
        .iter()
        .flat_map(|block| {
            block.marks.iter().filter_map(|m| match &m.kind {
                MarkKind::Image { src, link, .. } => {
                    Some((src.as_str(), link.as_ref().map(|l| (&l.to, l.note))))
                }
                _ => None,
            })
        })
        .collect()
}

#[test]
fn an_image_set_in_a_link_carries_it_though_the_link_covers_no_text() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        concat!(
            r#"<p>Go <a href="notes.xhtml#x"><img src="next.png"/></a> on,"#,
            r#"<a epub:type="noteref" href="notes.xhtml#n1"> <img src="star.png"/> </a><img src="plain.png"/>"#,
            r#" <a href="https://example.com/"><img src="out.png"/></a><a href="nowhere.xhtml"><img src="broken.png"/></a>.</p>"#,
        ),
        r#"<p id="x">There.</p><aside id="n1" epub:type="footnote"><p>The note.</p></aside>"#,
    );
    assert_eq!(book.blocks[0].text, "Go on, .", "the text is as it was");
    let there = LinkTarget::Block(block_of(&book, "There."));
    let note = LinkTarget::Block(block_of(&book, "The note."));
    let out = LinkTarget::Url("https://example.com/".to_string());
    assert_eq!(
        image_links(&book),
        [
            ("next.png", Some((&there, false))),
            ("star.png", Some((&note, true))),
            ("plain.png", None),
            ("out.png", Some((&out, false))),
            ("broken.png", None),
        ]
    );
    assert_eq!(links(&book), [], "no link mark covers nothing");
}

#[test]
fn a_figure_set_in_a_link_carries_it_joined_in_its_own_document() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        concat!(
            r#"<p>Start.</p><div><a href="https://example.com/"><img src="ad.png"/></a></div>"#,
            r#"<p><a href="notes.xhtml#x"><img src="next.png"/></a></p><p>Mid.</p>"#,
            r#"<a href="nowhere.xhtml"><img src="broken.png"/></a>"#,
        ),
        r##"<p id="x">There.</p><p><a href="#y"><img src="back.png"/></a></p><p id="y">Why.</p>"##,
    );
    let read: Vec<&str> = book.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(read, ["Start.", "Mid.", "There.", "Why."]);
    let linked = |before, src, to: LinkTarget| Figure {
        link: Some(ImageLink { to, note: false }),
        ..figure(before, src, "")
    };
    assert_eq!(
        book.figures,
        [
            linked(1, "ad.png", LinkTarget::Url("https://example.com/".into())),
            linked(1, "next.png", LinkTarget::Block(2)),
            figure(2, "broken.png", ""),
            linked(3, "back.png", LinkTarget::Block(3)),
        ]
    );
}

#[test]
fn an_image_alone_between_paragraphs_is_a_figure() {
    let dir = TempDir::new("book");
    let book = in_text_directory(
        dir.path(),
        r#"<p>a</p><div><img src="../img/map.png" alt="Map"/></div><p>b</p>"#,
    );
    let read: Vec<&str> = book.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(read, ["a", "b"]);
    assert_eq!(book.figures, [figure(1, "img/map.png", "Map")]);
    assert_eq!(images(&book), []);
}

#[test]
fn an_image_inside_a_paragraph_is_an_inline_mark() {
    let dir = TempDir::new("book");
    let book = in_text_directory(dir.path(), r#"<p>x <img src="g.png" alt="*"/> y</p>"#);
    assert_eq!(book.blocks[0].text, "x y");
    assert_eq!(images(&book), [("x y", 2..2, "text/g.png", "*")]);
    assert_eq!(book.figures, []);
}

#[test]
fn an_svg_image_is_read_by_its_xlink_href() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="cover.jpg"/></svg>"#,
    );
    assert!(book.blocks.is_empty());
    assert_eq!(book.figures, [page(0, "cover.jpg", "")]);
}

#[test]
fn an_image_alone_in_its_document_is_a_page_and_one_beside_text_is_not() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<div><img src="cover.png" alt="Cover"/></div>"#,
        r#"<p>Text.</p><img src="map.png" alt="Map"/><p>More.</p>"#,
    );
    let read: Vec<&str> = book.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(read, ["Text.", "More."]);
    assert_eq!(
        book.figures,
        [page(0, "cover.png", "Cover"), figure(1, "map.png", "Map")]
    );
}

#[test]
fn two_images_alone_in_their_document_are_not_pages() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<div><img src="logo.png" alt="Logo"/><img src="title.png" alt="Title"/></div>"#,
        r#"<p>Text.</p>"#,
    );
    assert_eq!(
        book.figures,
        [
            figure(0, "logo.png", "Logo"),
            figure(0, "title.png", "Title")
        ]
    );
}

#[test]
fn a_figure_after_the_last_block_has_before_equal_to_the_block_count() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        r#"<p>a</p><p>b</p><img src="end.png" alt="The end"/>"#,
    );
    assert_eq!(book.blocks.len(), 2);
    assert_eq!(book.figures, [figure(2, "end.png", "The end")]);
}

#[test]
fn an_image_whose_src_climbs_out_is_left_out() {
    let dir = TempDir::new("book");
    let book = one_document(
        dir.path(),
        r#"<p>x <img src="../in.png"/> y</p><div><img src="../../out.png"/></div>"#,
    );
    assert_eq!(book.blocks[0].text, "x y");
    assert_eq!(images(&book), []);
    assert_eq!(book.figures, []);
}

#[test]
fn figures_follow_the_blocks_numbering_kept_across_documents() {
    let dir = TempDir::new("book");
    let book = linked(
        dir.path(),
        r#"<p>Start.</p><img src="a.png"/>"#,
        &format!(
            "{}<p>10</p><img src=\"b.png\"/>{}<p>15</p>{}",
            verse(6, 9),
            verse(10, 14),
            verse(15, 16)
        ),
    );
    let line = block_of(&book, "line 10");
    assert_eq!(
        book.figures,
        [figure(1, "a.png", ""), figure(line, "b.png", "")]
    );
}

#[test]
fn an_inline_image_follows_the_verse_number_cut() {
    let dir = TempDir::new("book");
    let book = in_text_directory(
        dir.path(),
        &format!(
            r#"<p>5 Sing <img src="g.png"/>goddess</p>{}<p>10 the wrath</p>"#,
            verse(6, 9)
        ),
    );
    assert_eq!(book.blocks[0].line, Some(5));
    assert_eq!(images(&book), [("Sing goddess", 5..5, "text/g.png", "")]);
}
