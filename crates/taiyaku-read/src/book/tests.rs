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
