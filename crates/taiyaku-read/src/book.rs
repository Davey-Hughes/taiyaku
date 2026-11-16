//! Reading an EPUB as an ordered list of text blocks.
//!
//! The measurement design's §4
//! (`docs/superpowers/specs/2026-09-30-book-alignment-measurement-design.md`):
//! an aligner for books matches paragraphs the way `solve` matches pages, and
//! this is where a book becomes paragraphs. A block is one block-level
//! element that carries text of its own, in spine order and document order.
//!
//! **Strict XML, refused by name.** Every spine document is parsed with
//! `roxmltree`, and one that is not well-formed fails the whole read with its
//! path in the error. A recovering HTML parser would guess, and an aligner
//! scoring text a guess produced cannot say which of its mistakes are its own.
//!
//! **What this leaves out.** Notes are not taken out of the flow, verse lines
//! are not told from prose, and no page markers are kept: the spec's
//! `NoteBody`, `VerseLine` and `page_marker` are not read. A printed line
//! number is, as its line's ([`Block::line`]), so that it is neither text
//! nor a block of its own.

use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek},
    path::Path,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub id: BlockId,
    pub kind: BlockKind,
    /// Whitespace collapsed to single spaces and trimmed; soft hyphens removed.
    pub text: String,
    /// The verse line's number where the book prints one (see [`numbered`]).
    pub line: Option<u32>,
}

/// Where a block sits: its spine document, and its place among that
/// document's blocks, counting from 0.
///
/// Stable within one file and no further: a re-published edition may number
/// its blocks differently.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId {
    /// The document's path inside the EPUB archive, as resolved from the package.
    pub document: String,
    pub ordinal: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// `h1` to `h6`. Only markup decides this: a heading dressed as a `<p>`
    /// stays a paragraph, which is why the aligner must not lean on structure.
    Heading,
    Paragraph,
}

/// An EPUB's text, in reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    /// The package's first `dc:title`, or `None` if it has none.
    pub title: Option<String>,
    /// The package's first `dc:language`, as written, or `None` if it has none.
    pub language: Option<String>,
    pub blocks: Vec<Block>,
}

/// Elements whose text is a block of its own. Anything else is inline, and
/// its text joins the nearest enclosing block.
const BLOCK_ELEMENTS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "caption",
    "dd",
    "div",
    "dt",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "li",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
];

/// Elements whose text is never read: not part of the book's prose. `rt` and
/// `rp` are ruby's reading glosses and their fallback brackets, set over the
/// text in print rather than in its line, so reading them in would lengthen
/// every glossed passage the aligner measures.
const SKIPPED_ELEMENTS: &[&str] = &["head", "script", "style", "rt", "rp"];

/// The spine's document types. A spine item of any other type (an image, say)
/// carries no text to read, so it is passed over rather than refused.
const DOCUMENT_TYPES: &[&str] = &["application/xhtml+xml", "text/html"];

const SOFT_HYPHEN: char = '\u{ad}';

/// Read the EPUB at `path` into blocks.
///
/// # Errors
///
/// Returns an error naming the file, and where it applies the document inside
/// it, when the file is not a zip, when `META-INF/container.xml` or the package
/// it names is missing or malformed, when the spine names an item the manifest
/// lacks, when an href climbs out of the archive, or when a spine document is
/// not well-formed XML.
pub fn read_book(path: &Path) -> Result<Book, String> {
    let named = |e: String| format!("{}: {e}", path.display());
    let file = File::open(path).map_err(|e| named(e.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| named(e.to_string()))?;
    read_archive(&mut archive).map_err(named)
}

fn read_archive<R: Read + Seek>(archive: &mut zip::ZipArchive<R>) -> Result<Book, String> {
    let container = entry_text(archive, "META-INF/container.xml")?;
    let package_path = package_path(&container)?;
    let package_text = entry_text(archive, &package_path)?;
    let package = Package::parse(&package_text, &package_path)?;

    let mut blocks = Vec::new();
    for document in &package.spine {
        let text = numbered_references(&entry_text(archive, document)?);
        let parsed = parse_xml(&text).map_err(|e| format!("{document}: {e}"))?;
        for (ordinal, (kind, text, line)) in
            numbered(document_blocks(&parsed)).into_iter().enumerate()
        {
            blocks.push(Block {
                id: BlockId {
                    document: document.clone(),
                    ordinal,
                },
                kind,
                text,
                line,
            });
        }
    }
    Ok(Book {
        title: package.title,
        language: package.language,
        blocks,
    })
}

fn entry_text<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<String, String> {
    let mut entry = archive.by_name(name).map_err(|e| format!("{name}: {e}"))?;
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|e| format!("{name}: {e}"))?;
    Ok(text)
}

/// XHTML documents routinely carry a `<!DOCTYPE>`, which `roxmltree` refuses
/// unless told otherwise. The DTD itself is never fetched.
fn parse_xml(text: &str) -> Result<roxmltree::Document<'_>, String> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    roxmltree::Document::parse_with_options(text, options).map_err(|e| e.to_string())
}

/// `text` with each XHTML named character reference written as the numeric
/// one it stands for, `&nbsp;` as `&#160;`: the parser declares none of them,
/// since it reads no DTD. A name no XHTML DTD declares is left for the parser
/// to refuse, as it refuses anything else not well-formed.
fn numbered_references(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let name_len = rest[1..]
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(rest.len() - 1);
        let name = &rest[1..=name_len];
        let ends = rest[1 + name_len..].starts_with(';');
        match entities::NAMED.binary_search_by(|(known, _)| known.cmp(&name)) {
            Ok(i) if ends => {
                out.push_str(&format!("&#{};", entities::NAMED[i].1));
                rest = &rest[name_len + 2..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn package_path(container: &str) -> Result<String, String> {
    let doc = parse_xml(container).map_err(|e| format!("META-INF/container.xml: {e}"))?;
    doc.descendants()
        .find(|n| is_named(n, "rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .map(str::to_string)
        .ok_or_else(|| "META-INF/container.xml: no rootfile names a package".to_string())
}

struct Package {
    title: Option<String>,
    language: Option<String>,
    /// Each readable spine document's path inside the archive, in spine order.
    spine: Vec<String>,
}

impl Package {
    fn parse(text: &str, package_path: &str) -> Result<Self, String> {
        let doc = parse_xml(text).map_err(|e| format!("{package_path}: {e}"))?;
        let base = match package_path.rfind('/') {
            Some(slash) => &package_path[..slash],
            None => "",
        };

        let manifest: HashMap<&str, (&str, &str)> = doc
            .descendants()
            .filter(|n| is_named(n, "item"))
            .filter_map(|n| {
                let id = n.attribute("id")?;
                let href = n.attribute("href")?;
                Some((id, (href, n.attribute("media-type").unwrap_or(""))))
            })
            .collect();

        let mut spine = Vec::new();
        for itemref in doc.descendants().filter(|n| is_named(n, "itemref")) {
            let idref = itemref
                .attribute("idref")
                .ok_or_else(|| format!("{package_path}: a spine itemref has no idref"))?;
            let (href, media_type) = manifest.get(idref).ok_or_else(|| {
                format!("{package_path}: the spine names `{idref}`, which the manifest lacks")
            })?;
            if DOCUMENT_TYPES.contains(media_type) {
                spine.push(resolve(base, href).map_err(|e| format!("{package_path}: {e}"))?);
            }
        }

        let first = |name: &str| {
            doc.descendants()
                .find(|n| is_named(n, name))
                .and_then(|n| n.text())
                .map(|text| text.trim().to_string())
                .filter(|text| !text.is_empty())
        };
        Ok(Self {
            title: first("title"),
            language: first("language"),
            spine,
        })
    }
}

/// `href` as a path inside the archive: percent-decoded, any fragment dropped,
/// and its `.` and `..` segments resolved against `base`, the package's
/// directory. An href that climbs above the archive's root is refused rather
/// than clamped, since clamping would read some other file.
fn resolve(base: &str, href: &str) -> Result<String, String> {
    let without_fragment = href.split('#').next().unwrap_or(href);
    let decoded = percent_decode(without_fragment)
        .ok_or_else(|| format!("`{href}` is not valid percent-encoded UTF-8"))?;
    let mut segments: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for segment in decoded.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments
                    .pop()
                    .ok_or_else(|| format!("`{href}` climbs out of the archive"))?;
            }
            other => segments.push(other),
        }
    }
    Ok(segments.join("/"))
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The longest a paragraph may be and still be read as a verse line that opens
/// on its number: a printed line, which prose that opens on a year is not.
const LINE_LENGTH: usize = 120;

/// `blocks` with the verse line numbers a book prints taken out of the text.
/// No markup says what a line number is (each publisher names its own
/// classes), so it is read from its shape: a paragraph of a number alone
/// numbers the next paragraph, and a paragraph no longer than [`LINE_LENGTH`]
/// opening on a number and a space numbers itself. Either is taken only where
/// the next or the last such number bears it out, the two numbers differing by
/// the lines between them, give or take [`LINE_SLACK`]: line numbers count
/// lines, and an address, a list or a year opening a note counts nothing.
fn numbered(blocks: Vec<(BlockKind, String)>) -> Vec<(BlockKind, String, Option<u32>)> {
    /// A number a paragraph may print for a line: where, its value, which
    /// line it numbers counted in lines, and whether it stands alone.
    struct Candidate {
        at: usize,
        n: u32,
        line: usize,
        alone: bool,
    }
    let mut candidates = Vec::new();
    let mut lines = 0;
    for (at, (kind, text)) in blocks.iter().enumerate() {
        if *kind != BlockKind::Paragraph {
            continue;
        }
        if let Some(n) = number(text) {
            candidates.push(Candidate {
                at,
                n,
                line: lines,
                alone: true,
            });
            continue;
        }
        if let Some(n) = opening_number(text) {
            candidates.push(Candidate {
                at,
                n,
                line: lines,
                alone: false,
            });
        }
        lines += 1;
    }
    let agree = |a: &Candidate, b: &Candidate| {
        let numbers = i64::from(b.n) - i64::from(a.n);
        let between = b.line as i64 - a.line as i64;
        (numbers == 0 && between == 0)
            || (between > 0 && numbers > 0 && (numbers - between).abs() <= LINE_SLACK)
    };
    let mut taken = std::collections::HashMap::new();
    for (i, c) in candidates.iter().enumerate() {
        let before = i.checked_sub(1).map(|j| &candidates[j]);
        let after = candidates.get(i + 1);
        if before.is_some_and(|b| agree(b, c)) || after.is_some_and(|a| agree(c, a)) {
            taken.insert(c.at, c);
        }
    }

    let mut out = Vec::with_capacity(blocks.len());
    let mut pending = None;
    for (at, (kind, text)) in blocks.into_iter().enumerate() {
        match taken.get(&at) {
            Some(c) if c.alone => pending = Some(c.n),
            Some(c) => {
                pending = None;
                let rest = text.split_once(' ').map_or("", |(_, rest)| rest);
                out.push((kind, rest.to_string(), Some(c.n)));
            }
            None if kind == BlockKind::Paragraph => out.push((kind, text, pending.take())),
            None => out.push((kind, text, None)),
        }
    }
    out
}

/// How far two line numbers may differ from the lines counted between them
/// and still bear each other out: a line some printings split in two.
const LINE_SLACK: i64 = 2;

/// The number a short paragraph opens on, a space after it.
fn opening_number(text: &str) -> Option<u32> {
    if text.chars().count() > LINE_LENGTH {
        return None;
    }
    text.split_once(' ').and_then(|(head, _)| number(head))
}

/// `text` as a line number: one to five digits and nothing else.
fn number(text: &str) -> Option<u32> {
    (1..=5)
        .contains(&text.len())
        .then_some(text)
        .filter(|t| t.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|t| t.parse().ok())
}

/// The document's blocks, in document order: each run of text between block
/// boundaries is a block of the innermost block element holding it. Text a
/// block element holds outside its nested blocks is split around them, as a
/// browser lays it out, so a loose line after a paragraph stays after it.
fn document_blocks(doc: &roxmltree::Document<'_>) -> Vec<(BlockKind, String)> {
    let mut walk = Walk::default();
    if let Some(body) = doc.descendants().find(|n| is_named(n, "body")) {
        walk.within(body, BlockKind::Paragraph);
        walk.flush(BlockKind::Paragraph);
    }
    walk.out
}

#[derive(Default)]
struct Walk {
    out: Vec<(BlockKind, String)>,
    text: String,
}

impl Walk {
    /// Read `node`'s children, `kind` being the kind of the innermost block
    /// element around them.
    fn within(&mut self, node: roxmltree::Node<'_, '_>, kind: BlockKind) {
        for child in node.children() {
            if child.is_text() {
                self.text.push_str(child.text().unwrap_or(""));
            } else if child.is_element() {
                match child.tag_name().name() {
                    name if SKIPPED_ELEMENTS.contains(&name) => {}
                    name if BLOCK_ELEMENTS.contains(&name) => {
                        self.flush(kind);
                        let inner = kind_of(child);
                        self.within(child, inner);
                        self.flush(inner);
                    }
                    "br" => self.text.push(' '),
                    _ => self.within(child, kind),
                }
            }
        }
    }

    fn flush(&mut self, kind: BlockKind) {
        let text = collapse(&self.text);
        self.text.clear();
        if !text.is_empty() {
            self.out.push((kind, text));
        }
    }
}

fn kind_of(node: roxmltree::Node<'_, '_>) -> BlockKind {
    if is_heading(node) {
        BlockKind::Heading
    } else {
        BlockKind::Paragraph
    }
}

fn is_heading(node: roxmltree::Node<'_, '_>) -> bool {
    matches!(
        node.tag_name().name(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
    )
}

/// Unicode whitespace, no-break spaces included, becomes one space, and the
/// ends are trimmed. A no-break space is layout, never content to align on.
fn collapse(text: &str) -> String {
    text.split(|c: char| c.is_whitespace())
        .map(|word| word.replace(SOFT_HYPHEN, ""))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Matched on the local name alone: packages and documents in the wild mix
/// prefixed, default and missing namespaces for the same elements.
fn is_named(node: &roxmltree::Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

mod entities;

#[cfg(test)]
mod tests;
