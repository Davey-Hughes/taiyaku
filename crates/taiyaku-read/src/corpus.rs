//! Standing checks against the real corpus. Gated on `TAIYAKU_CORPUS` and
//! skipping cleanly without it.
//!
//! Every EPUB under `$TAIYAKU_CORPUS/books/` reads, with a summary printed.
//!
//! The books corpus is one directory per pair, its EPUBs
//! copied in from a library (spec §2); nothing under it is committed.
//!
//! It asserts only that each book reads and yields text: the spec sets no pass
//! bar on extraction, and the counts it prints are for reading, not for
//! asserting.
//!
//! `TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-read corpus:: -- --nocapture`

use crate::{BlockKind, read_book};

#[test]
fn every_corpus_book_reads() {
    let Some(root) = std::env::var_os("TAIYAKU_CORPUS") else {
        eprintln!("TAIYAKU_CORPUS not set; skipping");
        return;
    };
    let books = std::path::Path::new(&root).join("books");
    let Ok(pairs) = std::fs::read_dir(&books) else {
        eprintln!("{} does not exist; skipping", books.display());
        return;
    };

    let mut paths: Vec<_> = pairs
        .map(|pair| pair.expect("pair").path())
        .filter(|pair| pair.is_dir())
        .flat_map(|pair| std::fs::read_dir(pair).expect("pair reads"))
        .map(|entry| entry.expect("entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "epub"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "{} holds no EPUB", books.display());

    for path in paths {
        let book = read_book(&path).unwrap_or_else(|e| panic!("{e}"));
        let headings = book
            .blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Heading)
            .count();
        let documents = book
            .blocks
            .iter()
            .map(|b| &b.id.document)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let chars: usize = book.blocks.iter().map(|b| b.text.chars().count()).sum();
        println!(
            "{}: language {:?}, {} documents with text, {} blocks ({headings} headings), {chars} characters",
            path.display(),
            book.language,
            documents,
            book.blocks.len(),
        );
        assert!(
            !book.blocks.is_empty(),
            "{} yielded no text",
            path.display()
        );
    }
}
