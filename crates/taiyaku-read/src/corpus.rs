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

/// Every `.epub` under `dir`, recursively, in sorted path order.
fn epubs_under(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("directory reads") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            epubs_under(&path, found);
        } else if path.extension().is_some_and(|e| e == "epub") {
            found.push(path);
        }
    }
}

/// A digest of everything a book's blocks carry that later work must not
/// change: the count, then each block's kind, line and text. Every field is
/// length-prefixed so two different block lists cannot hash alike by shifting
/// bytes from one field to the next.
fn book_digest(path: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};

    let Ok(book) = read_book(path) else {
        return "error".to_string();
    };
    let mut hash = Sha256::new();
    let mut field = |bytes: &[u8]| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    field(&(book.blocks.len() as u64).to_le_bytes());
    for block in &book.blocks {
        field(match block.kind {
            BlockKind::Heading => b"heading",
            BlockKind::Paragraph => b"paragraph",
        });
        field(format!("{:?}", block.line).as_bytes());
        field(block.text.as_bytes());
    }
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn json_string(text: &str) -> String {
    format!("{text:?}")
}

/// Guards that formatting work beside a block's text never changes the text,
/// kind, line or block count of any library book. The snapshot is written on
/// the first run, from the reader as it stood, and compared on every run after.
///
/// `TAIYAKU_BOOKS="$PWD/taiyaku-library/Books" TAIYAKU_BOOKS_SNAPSHOT="$PWD/target/book-text-snapshot.json" cargo test --release -p taiyaku-read corpus::every_library_book_keeps_its_text -- --nocapture`
#[test]
fn every_library_book_keeps_its_text() {
    let (Some(root), Some(snapshot)) = (
        std::env::var_os("TAIYAKU_BOOKS"),
        std::env::var_os("TAIYAKU_BOOKS_SNAPSHOT"),
    ) else {
        eprintln!("TAIYAKU_BOOKS or TAIYAKU_BOOKS_SNAPSHOT not set; skipping");
        return;
    };
    let root = std::path::Path::new(&root);
    let snapshot = std::path::Path::new(&snapshot);

    let mut paths = Vec::new();
    epubs_under(root, &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "{} holds no EPUB", root.display());

    let digests: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            let relative = path.strip_prefix(root).expect("under root");
            (relative.display().to_string(), book_digest(path))
        })
        .collect();

    // One entry per line so a difference can be reported by path without a
    // JSON parser, which this crate does not depend on.
    let lines: Vec<String> = digests
        .iter()
        .map(|(path, digest)| format!("  {}: {}", json_string(path), json_string(digest)))
        .collect();
    let current = format!("{{\n{}\n}}\n", lines.join(",\n"));

    if !snapshot.exists() {
        std::fs::write(snapshot, &current).expect("snapshot writes");
        eprintln!("wrote {} entries to {}", digests.len(), snapshot.display());
        return;
    }

    let saved = std::fs::read_to_string(snapshot).expect("snapshot reads");
    let saved_lines: std::collections::BTreeSet<&str> =
        saved.lines().map(|l| l.trim_end_matches(',')).collect();
    let current_lines: std::collections::BTreeSet<&str> =
        current.lines().map(|l| l.trim_end_matches(',')).collect();
    let differing: Vec<&&str> = saved_lines.symmetric_difference(&current_lines).collect();
    assert!(
        differing.is_empty(),
        "{} entries differ from {}:\n{}",
        differing.len(),
        snapshot.display(),
        differing
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
}
