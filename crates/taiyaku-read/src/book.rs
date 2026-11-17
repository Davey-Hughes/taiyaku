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

use self::styles::Styles;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub id: BlockId,
    pub kind: BlockKind,
    /// Whitespace collapsed to single spaces and trimmed; soft hyphens removed.
    pub text: String,
    /// The verse line's number where the book prints one (see [`numbered`]).
    pub line: Option<u32>,
    /// How the book sets parts of `text`, sorted by start, a longer mark first
    /// where two start together. Kept beside `text` rather than in it, so that
    /// every offset already counted in `text` (notes, sentences, alignments)
    /// stays where it was.
    pub marks: Vec<Mark>,
}

/// A part of a block's text the book sets apart, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    /// Byte offsets into [`Block::text`], always on character boundaries.
    pub range: std::ops::Range<usize>,
    pub kind: MarkKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkKind {
    Italic,
    Bold,
    Sup,
    Sub,
    /// A gloss set over the range, which is its base text.
    Ruby {
        reading: String,
    },
    /// `note` where the link is a footnote reference, to be opened in place
    /// rather than followed.
    Link {
        to: LinkTarget,
        note: bool,
    },
    /// An image inside a paragraph, at an empty range where it stands.
    Image {
        src: String,
        alt: String,
        link: Option<ImageLink>,
    },
}

/// The link an image is set in. It rides on the image, not on a
/// [`MarkKind::Link`], because a link's range covers text and an image takes
/// none: a link holding only an image would cover nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageLink {
    pub to: LinkTarget,
    /// As [`MarkKind::Link`]'s `note`.
    pub note: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// An index into [`Book::blocks`].
    Block(usize),
    Url(String),
}

/// An image standing between blocks rather than inside one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Figure {
    /// The index into [`Book::blocks`] of the block it comes before, the block
    /// count where it comes after the last.
    pub before: usize,
    /// The image's path inside the archive.
    pub src: String,
    pub alt: String,
    pub link: Option<ImageLink>,
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
    /// The images standing between blocks, in reading order.
    pub figures: Vec<Figure>,
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
    let mut figures = Vec::new();
    let mut sheets = HashMap::new();
    let mut anchors = Vec::with_capacity(package.spine.len());
    for document in &package.spine {
        let text = numbered_references(&entry_text(archive, document)?);
        let parsed = parse_xml(&text).map_err(|e| format!("{document}: {e}"))?;
        let styles = document_styles(archive, document, &parsed, &mut sheets);
        let walk = document_blocks(&parsed, document, styles);
        let (numbered, went) = numbered(walk.out);
        let first = blocks.len();
        let figures_from = figures.len();
        // A figure before a block `numbered` took out, or after the last,
        // comes before the next block kept: the next document's first, or
        // the block count at the book's end.
        figures.extend(walk.figures.into_iter().map(|figure| Figure {
            before: first + went.get(figure.before).copied().unwrap_or(numbered.len()),
            ..figure
        }));
        anchors.push(Anchors {
            first,
            went,
            kept: numbered.len(),
            figures: figures_from..figures.len(),
            ids: walk.ids,
            links: walk.links,
        });
        for (ordinal, (walked, line)) in numbered.into_iter().enumerate() {
            blocks.push(Block {
                id: BlockId {
                    document: document.clone(),
                    ordinal,
                },
                kind: walked.kind,
                text: walked.text,
                line,
                marks: walked.marks,
            });
        }
    }
    join_links(&package.spine, &anchors, &mut blocks, &mut figures);
    Ok(Book {
        title: package.title,
        language: package.language,
        blocks,
        figures,
    })
}

/// What one spine document's links, and the links into it, need once every
/// document is read: a link may point at a document not yet read.
struct Anchors {
    /// Where the document's blocks start in [`Book::blocks`].
    first: usize,
    /// Each walked block's index among the blocks [`numbered`] kept.
    went: Vec<usize>,
    /// How many blocks [`numbered`] kept.
    kept: usize,
    /// Where the document's figures sit in [`Book::figures`].
    figures: std::ops::Range<usize>,
    /// See [`Walk::ids`].
    ids: HashMap<String, (usize, bool)>,
    /// See [`Walk::links`].
    links: Vec<ReadLink>,
}

/// A link as its document's walk read it, before its target is known.
struct ReadLink {
    href: String,
    /// Whether its own markup makes it a note reference.
    reference: bool,
    /// Whether it sits inside a note.
    from_note: bool,
}

/// The schemes a link may leave the book by. Any other (`javascript:`,
/// `data:`, a file) is nothing a reader should be sent to.
const WEB_SCHEMES: &[&str] = &["http", "https", "mailto"];

/// Give each link mark in `blocks`, and each image's link there and in
/// `figures`, its target, in place of the index into its document's
/// [`Walk::links`] it was read with, and drop each whose target cannot be
/// found: a broken link is never worth refusing a book over. An image whose
/// link is dropped is kept, unlinked.
fn join_links(spine: &[String], anchors: &[Anchors], blocks: &mut [Block], figures: &mut [Figure]) {
    let by_path: HashMap<&str, &Anchors> = spine.iter().map(String::as_str).zip(anchors).collect();
    let total = blocks.len();
    for (document, anchor) in spine.iter().zip(anchors) {
        let targets: Vec<Option<(LinkTarget, bool)>> = anchor
            .links
            .iter()
            .map(|link| {
                link_target(&link.href, document, &by_path, total).map(|(to, into_note)| {
                    // A note's way back to its reference, often set as the
                    // reference is, would otherwise open the text as a note.
                    let back_out = link.from_note && !into_note;
                    (to, !back_out && (link.reference || into_note))
                })
            })
            .collect();
        let joined = |read: usize| targets.get(read).cloned().flatten();
        let join_image = |link: &mut Option<ImageLink>| {
            if let Some(ImageLink {
                to: LinkTarget::Block(read),
                ..
            }) = *link
            {
                *link = joined(read).map(|(to, note)| ImageLink { to, note });
            }
        };
        for block in &mut blocks[anchor.first..anchor.first + anchor.kept] {
            block.marks.retain_mut(|mark| match &mut mark.kind {
                MarkKind::Link {
                    to: LinkTarget::Block(read),
                    ..
                } => match joined(*read) {
                    Some((to, note)) => {
                        mark.kind = MarkKind::Link { to, note };
                        true
                    }
                    None => false,
                },
                MarkKind::Image { link, .. } => {
                    join_image(link);
                    true
                }
                _ => true,
            });
        }
        for figure in &mut figures[anchor.figures.clone()] {
            join_image(&mut figure.link);
        }
    }
}

/// Where `href`, a link in `document`, goes, and whether its target sits
/// inside a note. An in-book target is the block holding the element its
/// fragment names, or the block after where that element stands if it holds
/// none; a document named alone is its first block.
fn link_target(
    href: &str,
    document: &str,
    by_path: &HashMap<&str, &Anchors>,
    total: usize,
) -> Option<(LinkTarget, bool)> {
    let href = href.trim();
    if let Some(scheme) = scheme(href) {
        return WEB_SCHEMES
            .iter()
            .any(|web| web.eq_ignore_ascii_case(scheme))
            .then(|| (LinkTarget::Url(href.to_string()), false));
    }
    let (path, fragment) = href.split_once('#').unwrap_or((href, ""));
    // `href="#"` is a link to nowhere, kept for a script or a style.
    if path.is_empty() && fragment.is_empty() {
        return None;
    }
    let path = if path.is_empty() {
        document.to_string()
    } else {
        let directory = document.rsplit_once('/').map_or("", |(dir, _)| dir);
        resolve(directory, path).ok()?
    };
    let anchor = by_path.get(path.as_str())?;
    let (walked, into_note) = if fragment.is_empty() {
        (0, false)
    } else {
        *anchor
            .ids
            .get(fragment)
            .or_else(|| percent_decode(fragment).and_then(|id| anchor.ids.get(&id)))?
    };
    // An id at or after the document's last block names no block of its own
    // there, and lands on the next document's first, if there is one.
    let kept = anchor.went.get(walked).copied().unwrap_or(anchor.kept);
    let at = anchor.first + kept;
    (at < total).then_some((LinkTarget::Block(at), into_note))
}

/// The scheme `href` opens on, if it is an absolute URL.
fn scheme(href: &str) -> Option<&str> {
    let (scheme, _) = href.split_once(':')?;
    let mut chars = scheme.chars();
    (chars.next()?.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)))
    .then_some(scheme)
}

/// The styles `document` answers to: its linked stylesheets, then its
/// `<style>` elements. A sheet that is missing or is not UTF-8 adds none, since
/// styling is never worth refusing a book over. `sheets` holds each sheet read,
/// by path, so one shared by every chapter is parsed once.
fn document_styles<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    document: &str,
    parsed: &roxmltree::Document<'_>,
    sheets: &mut HashMap<String, Styles>,
) -> Styles {
    let directory = document.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut styles = Styles::default();
    for link in parsed.descendants().filter(|n| is_named(n, "link")) {
        let stylesheet = link.attribute("rel").is_some_and(|rel| {
            rel.split_whitespace()
                .any(|r| r.eq_ignore_ascii_case("stylesheet"))
        });
        let Some(path) = link
            .attribute("href")
            .filter(|_| stylesheet)
            .and_then(|href| resolve(directory, href).ok())
        else {
            continue;
        };
        let sheet = sheets.entry(path).or_insert_with_key(|path| {
            entry_text(archive, path).map_or_else(|_| Styles::default(), |css| Styles::parse(&css))
        });
        styles.extend(sheet.clone());
    }
    for style in parsed.descendants().filter(|n| is_named(n, "style")) {
        let css: String = style
            .descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect();
        styles.extend(Styles::parse(&css));
    }
    styles
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
fn numbered(blocks: Vec<Walked>) -> (Vec<(Walked, Option<u32>)>, Vec<usize>) {
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
    for (at, Walked { kind, text, .. }) in blocks.iter().enumerate() {
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
    let mut went = Vec::with_capacity(blocks.len());
    let mut pending = None;
    for (at, walked) in blocks.into_iter().enumerate() {
        // A number alone, taken out, maps to the next block kept (the line it
        // numbers), so that a link to it still lands.
        went.push(out.len());
        match taken.get(&at) {
            Some(c) if c.alone => pending = Some(c.n),
            Some(c) => {
                pending = None;
                out.push((without_opening_number(walked), Some(c.n)));
            }
            None if walked.kind == BlockKind::Paragraph => out.push((walked, pending.take())),
            None => out.push((walked, None)),
        }
    }
    (out, went)
}

/// `walked` with the number it opens on, and the space after it, cut away.
fn without_opening_number(walked: Walked) -> Walked {
    let cut = walked
        .text
        .find(' ')
        .map_or(walked.text.len(), |space| space + 1);
    let mut marks: Vec<Mark> = walked
        .marks
        .into_iter()
        .filter_map(|mark| {
            let range = mark.range.start.saturating_sub(cut)..mark.range.end.saturating_sub(cut);
            let image = matches!(mark.kind, MarkKind::Image { .. });
            (image || !range.is_empty()).then_some(Mark {
                range,
                kind: mark.kind,
            })
        })
        .collect();
    // Clipping can bring a mark that started inside the number level with
    // one that started after it, and shorter.
    sort_marks(&mut marks);
    Walked {
        kind: walked.kind,
        text: walked.text[cut..].to_string(),
        marks,
    }
}

fn sort_marks(marks: &mut [Mark]) {
    marks.sort_by(|a, b| {
        a.range
            .start
            .cmp(&b.range.start)
            .then(b.range.end.cmp(&a.range.end))
    });
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
fn document_blocks(doc: &roxmltree::Document<'_>, document: &str, styles: Styles) -> Walk {
    let mut walk = Walk {
        styles,
        directory: document
            .rsplit_once('/')
            .map_or("", |(dir, _)| dir)
            .to_string(),
        ..Walk::default()
    };
    if let Some(body) = doc.descendants().find(|n| is_named(n, "body")) {
        walk.record_id(body);
        walk.within(body, BlockKind::Paragraph);
        walk.flush(BlockKind::Paragraph);
    }
    walk
}

/// A block as the walk reads it, before any verse number is taken out.
#[derive(Debug)]
struct Walked {
    kind: BlockKind,
    text: String,
    marks: Vec<Mark>,
}

#[derive(Default)]
struct Walk {
    out: Vec<Walked>,
    /// The current block's text as the document holds it, uncollapsed.
    text: String,
    /// The current block's closed marks, as ranges of `text`.
    marks: Vec<Mark>,
    /// The marks still open, innermost last, each with where it opened in
    /// `text`.
    open: Vec<(MarkKind, usize)>,
    /// Where the italic now in force opened in `text`, if it is. Italic and
    /// bold are states a tag, a class or a style turns on and off, so they
    /// are kept apart from `open`'s marks, which a tag alone sets.
    italic: Option<usize>,
    /// Where the bold now in force opened in `text`, if it is.
    bold: Option<usize>,
    /// The stylesheets the document being read answers to.
    styles: Styles,
    /// Each `id` the document gives an element, the first where two share
    /// one: the index in `out` of the block it falls in (of the next block
    /// read, if it holds no text), and whether it sits inside a note.
    ids: HashMap<String, (usize, bool)>,
    /// Each link read. Its mark stands in for the target with its index
    /// here, since the target may lie in a document not yet read;
    /// [`join_links`] puts the target in.
    links: Vec<ReadLink>,
    /// The directory of the document being read, which its images' paths
    /// are relative to.
    directory: String,
    /// The images read standing between blocks, each `before` the index in
    /// `out` of the block after it.
    figures: Vec<Figure>,
}

impl Walk {
    /// Read `node`'s children, `kind` being the kind of the innermost block
    /// element around them.
    fn within(&mut self, node: roxmltree::Node<'_, '_>, kind: BlockKind) {
        for child in node.children() {
            self.child(child, kind);
        }
    }

    /// Read one child of a block: text joins the block, and an element is read
    /// under the italic and bold its tag, class and style leave in force.
    fn child(&mut self, child: roxmltree::Node<'_, '_>, kind: BlockKind) {
        if child.is_text() {
            self.text.push_str(child.text().unwrap_or(""));
        } else if child.is_element() {
            let name = child.tag_name().name();
            if SKIPPED_ELEMENTS.contains(&name) {
                return;
            }
            let outside = (self.italic, self.bold);
            self.enter_style(child);
            let block = BLOCK_ELEMENTS.contains(&name);
            if !block {
                self.record_id(child);
            }
            match name {
                "ruby" => self.ruby(child, kind),
                _ if block => {
                    self.flush(kind);
                    // After the flush, since the text before it is a block
                    // of its own, and the element's is the next.
                    self.record_id(child);
                    let inner = kind_of(child);
                    self.within(child, inner);
                    self.flush(inner);
                }
                "br" => self.text.push(' '),
                "img" | "image" => self.image(child),
                "a" => match child.attribute("href") {
                    Some(href) => {
                        let mark = MarkKind::Link {
                            to: LinkTarget::Block(self.links.len()),
                            note: false,
                        };
                        self.links.push(ReadLink {
                            href: href.to_string(),
                            reference: is_note_reference(child),
                            from_note: in_note(child),
                        });
                        self.marked_within(mark, child, kind);
                    }
                    None => self.within(child, kind),
                },
                name => match inline_mark(name) {
                    Some(mark) => self.marked_within(mark, child, kind),
                    None => self.within(child, kind),
                },
            }
            self.turn(MarkKind::Italic, outside.0.is_some());
            self.turn(MarkKind::Bold, outside.1.is_some());
        }
    }

    /// Read `element`'s children under `mark`.
    fn marked_within(&mut self, mark: MarkKind, element: roxmltree::Node<'_, '_>, kind: BlockKind) {
        self.open.push((mark, self.text.len()));
        self.within(element, kind);
        if let Some((mark, start)) = self.open.pop() {
            self.marks.push(Mark {
                range: start..self.text.len(),
                kind: mark,
            });
        }
    }

    /// Record `element`, an `img` or an SVG `image`, where it stands in the
    /// text, as an empty [`MarkKind::Image`] mark, with the innermost link
    /// open around it: whether it is a figure or sits in a line is known only
    /// once its block ends ([`Self::flush`]). An image whose path is not one
    /// inside the archive is left out, since an image is never worth refusing
    /// a book over.
    fn image(&mut self, element: roxmltree::Node<'_, '_>) {
        let src = if is_named(&element, "img") {
            element.attribute("src")
        } else {
            // SVG 2's `href`, else SVG 1.1's `xlink:href`.
            element.attribute("href").or_else(|| {
                element
                    .attributes()
                    .find(|a| a.name() == "href")
                    .map(|a| a.value())
            })
        };
        let Some(src) = src
            .map(str::trim)
            .filter(|src| !src.is_empty() && scheme(src).is_none())
            .and_then(|src| resolve(&self.directory, src).ok())
        else {
            return;
        };
        let (alt, _) = collapse_mapped(element.attribute("alt").unwrap_or(""));
        let link = self.open.iter().rev().find_map(|(mark, _)| match mark {
            MarkKind::Link { to, note } => Some(ImageLink {
                to: to.clone(),
                note: *note,
            }),
            _ => None,
        });
        let at = self.text.len();
        self.marks.push(Mark {
            range: at..at,
            kind: MarkKind::Image { src, alt, link },
        });
    }

    fn record_id(&mut self, element: roxmltree::Node<'_, '_>) {
        if let Some(id) = element.attribute("id") {
            let at = self.out.len();
            self.ids
                .entry(id.to_string())
                .or_insert_with(|| (at, in_note(element)));
        }
    }

    /// Put `element`'s italic and bold in force: what its class or style says,
    /// else what its tag implies, else what it inherits.
    fn enter_style(&mut self, element: roxmltree::Node<'_, '_>) {
        let name = element.tag_name().name();
        let style = self.styles.of(
            name,
            element.attribute("class").unwrap_or(""),
            element.attribute("style"),
        );
        let (tag_italic, tag_bold) = tag_style(name);
        let italic = style.italic.or(tag_italic).unwrap_or(self.italic.is_some());
        let bold = style.bold.or(tag_bold).unwrap_or(self.bold.is_some());
        self.turn(MarkKind::Italic, italic);
        self.turn(MarkKind::Bold, bold);
    }

    /// Set whether `kind`, italic or bold, is on at the end of the text so
    /// far: a mark opens where it turns on and closes where it turns off, so
    /// the same state is never marked twice over the same text.
    fn turn(&mut self, kind: MarkKind, on: bool) {
        let at = self.text.len();
        let slot = if kind == MarkKind::Italic {
            &mut self.italic
        } else {
            &mut self.bold
        };
        match (slot.take(), on) {
            (None, true) => *slot = Some(at),
            (Some(start), true) => *slot = Some(start),
            (Some(start), false) => self.marks.push(Mark {
                range: start..at,
                kind,
            }),
            (None, false) => {}
        }
    }

    /// Read a `ruby` element: its bases join the text, and each `rt` marks
    /// the base read since the ruby opened or since the previous `rt`, so
    /// `<ruby>漢<rt>かん</rt>字<rt>じ</rt></ruby>` glosses each character on
    /// its own. The `rp` fallback parentheses are never text.
    fn ruby(&mut self, ruby: roxmltree::Node<'_, '_>, kind: BlockKind) {
        let mut base = self.text.len();
        for child in ruby.children() {
            if is_named(&child, "rt") {
                let raw: String = child
                    .descendants()
                    .filter(|n| n.is_text())
                    .filter_map(|n| n.text())
                    .collect();
                let (reading, _) = collapse_mapped(&raw);
                if !reading.is_empty() && base < self.text.len() {
                    self.marks.push(Mark {
                        range: base..self.text.len(),
                        kind: MarkKind::Ruby { reading },
                    });
                }
                base = self.text.len();
            } else {
                self.child(child, kind);
            }
        }
    }

    /// End the current block. A mark still open is cut here and opens again
    /// at the start of the next block, so italics set around several
    /// paragraphs mark each of them and no mark crosses a block.
    fn flush(&mut self, kind: BlockKind) {
        let end = self.text.len();
        for (mark, start) in &mut self.open {
            self.marks.push(Mark {
                range: *start..end,
                kind: mark.clone(),
            });
            *start = 0;
        }
        for (kind, slot) in [
            (MarkKind::Italic, &mut self.italic),
            (MarkKind::Bold, &mut self.bold),
        ] {
            if let Some(start) = slot {
                self.marks.push(Mark {
                    range: *start..end,
                    kind,
                });
                *start = 0;
            }
        }
        let (text, at) = collapse_mapped(&self.text);
        self.text.clear();
        let raw = std::mem::take(&mut self.marks);
        if text.is_empty() {
            // A block of images alone is the figures between its
            // neighbours, never a block of no text.
            for mark in raw {
                if let MarkKind::Image { src, alt, link } = mark.kind {
                    self.figures.push(Figure {
                        before: self.out.len(),
                        src,
                        alt,
                        link,
                    });
                }
            }
            return;
        }
        let mut marks: Vec<Mark> = raw
            .into_iter()
            .filter_map(|mark| {
                if let MarkKind::Image { .. } = mark.kind {
                    // On the space between two words it stands before the
                    // second, as a range starting there would.
                    let mut at = at[mark.range.start];
                    if text.as_bytes().get(at) == Some(&b' ') {
                        at += 1;
                    }
                    return Some(Mark {
                        range: at..at,
                        kind: mark.kind,
                    });
                }
                let range = within_words(&text, at[mark.range.start]..at[mark.range.end]);
                (!range.is_empty()).then_some(Mark {
                    range,
                    kind: mark.kind,
                })
            })
            .collect();
        sort_marks(&mut marks);
        self.out.push(Walked { kind, text, marks });
    }
}

/// The EPUB structural vocabulary's namespace, which `epub:type` belongs to.
const OPS: &str = "http://www.idpf.org/2007/ops";

/// The `epub:type` values that make an element a note.
const NOTE_TYPES: &[&str] = &["footnote", "endnote", "rearnote", "note"];

/// The `epub:type` values that make an element a list of notes: what sits
/// inside one is a note, but the list itself, the groups it sets its notes
/// in ([`NOTE_GROUPS`]) and their headings are not.
const NOTE_LIST_TYPES: &[&str] = &["footnotes", "endnotes", "rearnotes", "notes"];

/// `node`'s `epub:type` values. One with no namespace counts too, as some
/// books write it that way.
fn epub_types<'a>(node: roxmltree::Node<'a, '_>) -> impl Iterator<Item = &'a str> {
    node.attributes()
        .filter(|a| a.name() == "type" && a.namespace().is_none_or(|ns| ns == OPS))
        .flat_map(|a| a.value().split_whitespace())
}

/// Whether `element` is a note or sits inside one.
fn in_note(element: roxmltree::Node<'_, '_>) -> bool {
    // `ancestors` starts at `element` itself.
    if element
        .ancestors()
        .any(|n| epub_types(n).any(|t| NOTE_TYPES.contains(&t)))
    {
        return true;
    }
    let Some(list) = element
        .ancestors()
        .skip(1)
        .find(|n| epub_types(*n).any(|t| NOTE_LIST_TYPES.contains(&t)))
    else {
        return false;
    };
    // Down from the list: a heading met among the groups is the list's or a
    // group's, where a contents entry or a "see Notes" points, and opened as
    // a note it would show a title instead of notes. The first element that
    // is no group is an untyped note, and a heading inside it is the note's,
    // its links the note's way back.
    let mut path: Vec<_> = element.ancestors().take_while(|n| *n != list).collect();
    path.reverse();
    for n in path {
        if is_heading(n) {
            return false;
        }
        if !is_note_group(n) {
            return true;
        }
    }
    // A group linked to itself lands on its first block: a group's heading,
    // or, where it opens on none, a note set in a `div`.
    !opens_on_heading(element)
}

/// The elements a notes list groups its notes in, by chapter say, rather than
/// set a note as.
const NOTE_GROUPS: &[&str] = &["div", "section", "nav", "header", "hgroup", "ol", "ul"];

fn is_note_group(node: roxmltree::Node<'_, '_>) -> bool {
    NOTE_GROUPS.contains(&node.tag_name().name())
}

/// Whether the first thing `group` holds, read down through any groups
/// inside it, is a heading.
fn opens_on_heading(group: roxmltree::Node<'_, '_>) -> bool {
    let mut node = group;
    loop {
        let first = node.children().find(|c| {
            c.is_element() || (c.is_text() && c.text().is_some_and(|t| !t.trim().is_empty()))
        });
        match first {
            Some(c) if is_heading(c) => return true,
            Some(c) if c.is_element() && is_note_group(c) => node = c,
            _ => return false,
        }
    }
}

/// Whether the link `a` is a note reference by its own markup: so typed, or
/// set as a superscript, the way printed books set a note's number.
fn is_note_reference(a: roxmltree::Node<'_, '_>) -> bool {
    epub_types(a).any(|t| t == "noteref")
        || a.descendants().any(|n| is_named(&n, "sup"))
        || a.ancestors().any(|n| is_named(&n, "sup"))
}

/// The italic and bold a tag implies, before any stylesheet is read.
fn tag_style(name: &str) -> (Option<bool>, Option<bool>) {
    match name {
        "i" | "em" | "cite" => (Some(true), None),
        "b" | "strong" => (None, Some(true)),
        _ => (None, None),
    }
}

/// The mark an inline element sets over its text, if any.
fn inline_mark(name: &str) -> Option<MarkKind> {
    match name {
        "sup" => Some(MarkKind::Sup),
        "sub" => Some(MarkKind::Sub),
        _ => None,
    }
}

/// `range` of collapsed `text` without a space at either end: whitespace
/// inside an element's edges collapses into the space between words, which
/// belongs to neither.
fn within_words(text: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let bytes = text.as_bytes();
    let mut range = range;
    if range.start < range.end && bytes[range.start] == b' ' {
        range.start += 1;
    }
    if range.start < range.end && bytes[range.end - 1] == b' ' {
        range.end -= 1;
    }
    range
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

/// Unicode whitespace, no-break spaces included, becomes one space, the ends
/// are trimmed and soft hyphens removed. A no-break space is layout, never
/// content to align on.
///
/// The second value maps each byte offset of `text`, and its length, to the
/// collapsed text. A character kept maps to where it now stands; whitespace or
/// a soft hyphen maps to the end of what was kept before it, so that a range
/// ending there ends with that word, and a range starting there starts on the
/// space [`within_words`] steps over.
fn collapse_mapped(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut at = Vec::with_capacity(text.len() + 1);
    let mut gap = false;
    for c in text.chars() {
        if c.is_whitespace() {
            gap = true;
        } else if c != SOFT_HYPHEN {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
        }
        let here = out.len();
        at.extend(std::iter::repeat_n(here, c.len_utf8()));
        if !c.is_whitespace() && c != SOFT_HYPHEN {
            out.push(c);
        }
    }
    at.push(out.len());
    (out, at)
}

/// Matched on the local name alone: packages and documents in the wild mix
/// prefixed, default and missing namespaces for the same elements.
fn is_named(node: &roxmltree::Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

mod entities;
mod styles;

#[cfg(test)]
mod tests;
