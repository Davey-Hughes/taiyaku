//! A block's sentences, and the pairing of two books' sentences inside the
//! block links the book aligner found.
//!
//! **Inside a link, never across one.** A translation splits and joins
//! sentences as it does paragraphs, so the same program pairs them, but only
//! within a pair of block groups: a sentence pairing can refine the book's
//! alignment and never contradict it.
//!
//! **Split by punctuation, not by a dictionary.** A stop ends a sentence
//! where the next word does not continue it. That reads the scripts the
//! library holds without a model of any of them, and its mistakes (a stop
//! after an abbreviation the list does not know) cost a pairing a wider
//! step, not a wrong one.

use std::ops::Range;

use crate::{BlockLink, align_texts};

/// Stops followed by a space where they end a sentence.
const STOPS: &[char] = &['.', '!', '?', '…', '‼', '⁇', '⁈', '⁉'];

/// Stops of scripts that put no space between sentences.
const WIDE_STOPS: &[char] = &['。', '！', '？', '｡'];

/// What may close a quotation or an aside after its stop and still belong to
/// its sentence.
const CLOSERS: &[char] = &[
    '"', '\'', '”', '’', '»', '›', ')', ']', '）', '」', '』', '】', '〉', '》', '〕',
];

/// What may open the next sentence directly after a wide stop's closer.
const OPENERS: &[char] = &['「', '『', '（', '【', '〈', '《', '“', '‘'];

/// Dashes a reply's attribution starts with in Russian and Spanish dialogue,
/// looked past for the word that says whether a sentence continues.
const DASHES: &[char] = &['—', '–', '-'];

/// Words a point follows without ending a sentence: titles, which name the
/// person after them.
const TITLES: &[&str] = &[
    "Mr", "Mrs", "Ms", "Dr", "St", "Mt", "Jr", "Sr", "Sra", "Srta", "Prof", "Rev", "Gen", "Col",
    "Capt", "Lt", "Sgt", "Hon", "Fr",
];

/// The byte ranges of `text`'s sentences, in order, each trimmed of the space
/// around it. None for text that is all space.
#[must_use]
pub fn sentences(text: &str) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut ends = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i].1;
        let wide = WIDE_STOPS.contains(&c);
        if !wide && !STOPS.contains(&c) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() && (STOPS.contains(&chars[j].1) || WIDE_STOPS.contains(&chars[j].1)) {
            j += 1;
        }
        let stops_end = j;
        while j < chars.len() && CLOSERS.contains(&chars[j].1) {
            j += 1;
        }
        let end = chars.get(j).map_or(text.len(), |&(at, _)| at);
        if ends_here(&chars, i, stops_end, j, wide) {
            ends.push(end);
        }
        i = j.max(i + 1);
    }
    ends.push(text.len());

    let mut out = Vec::new();
    let mut start = 0;
    for end in ends {
        let piece = &text[start..end];
        let lead = piece.len() - piece.trim_start().len();
        let trail = piece.len() - piece.trim_end().len();
        if lead + trail < piece.len() {
            out.push(start + lead..end - trail);
        }
        start = end;
    }
    out
}

/// Whether the stops at `chars[stop..stops_end]`, closed up to `next`, end a
/// sentence.
fn ends_here(
    chars: &[(usize, char)],
    stop: usize,
    stops_end: usize,
    next: usize,
    wide: bool,
) -> bool {
    let Some(&(_, after)) = chars.get(next) else {
        return true;
    };
    if wide {
        // A wide stop needs no space; past a closer, the sentence runs on
        // unless the next one begins (「何だ。」と言った。 is one).
        return next == stops_end || after.is_whitespace() || OPENERS.contains(&after);
    }
    if !after.is_whitespace() {
        return false;
    }
    if stops_end - stop == 1
        && chars[stop].1 == '.'
        && next == stops_end
        && abbreviated(chars, stop)
    {
        return false;
    }
    let word = chars[next..]
        .iter()
        .map(|&(_, c)| c)
        .find(|c| !c.is_whitespace() && !DASHES.contains(c));
    !word.is_some_and(char::is_lowercase)
}

/// Whether the word before the point at `chars[point]` is an initial or a
/// title.
fn abbreviated(chars: &[(usize, char)], point: usize) -> bool {
    let start = chars[..point]
        .iter()
        .rposition(|&(_, c)| !c.is_alphabetic())
        .map_or(0, |at| at + 1);
    let word: String = chars[start..point].iter().map(|&(_, c)| c).collect();
    word.chars().count() == 1 && word.chars().all(char::is_uppercase)
        || TITLES.contains(&word.as_str())
}

/// The most block links one pairing of sentences may span, which bounds the
/// sentences one [`align_texts`] call pairs: a run of lopsided links would
/// otherwise join into one pairing as long as the run. Four is the figure
/// `docs/notes.md`, "Books: sentences paired inside the block links", tried.
const MOST_LINKS: usize = 4;

/// The links between two books' sentences, numbered through each book:
/// `a[i]` and `b[j]` are block `i`'s and `j`'s sentences, `blocks` the block
/// links. A pairing that ends lopsided joins the next links, up to
/// [`MOST_LINKS`], since one edition may end a paragraph with the sentence
/// the other starts its next with; a link one side of which is empty, a
/// passage the book's alignment left out, is never joined. A side of one
/// sentence pairs whole with the other.
#[must_use]
pub fn align_sentences<S: AsRef<str>>(
    a: &[Vec<S>],
    b: &[Vec<S>],
    blocks: &[BlockLink],
) -> Vec<BlockLink> {
    let first = |book: &[Vec<S>]| -> Vec<usize> {
        let mut at = Vec::with_capacity(book.len() + 1);
        let mut n = 0;
        at.push(0);
        for block in book {
            n += block.len();
            at.push(n);
        }
        at
    };
    let (a_at, b_at) = (first(a), first(b));
    let pair = |links: &[BlockLink]| -> Vec<BlockLink> {
        let (Some(head), Some(tail)) = (links.first(), links.last()) else {
            return Vec::new();
        };
        let (a_blocks, b_blocks) = (head.a.start..tail.a.end, head.b.start..tail.b.end);
        let a_run = a_at[a_blocks.start]..a_at[a_blocks.end];
        let b_run = b_at[b_blocks.start]..b_at[b_blocks.end];
        if a_run.is_empty() && b_run.is_empty() {
            return Vec::new();
        }
        if a_run.len() <= 1 || b_run.len() <= 1 {
            return vec![BlockLink { a: a_run, b: b_run }];
        }
        align_texts(&texts(a, a_blocks), &texts(b, b_blocks))
            .into_iter()
            .map(|p| BlockLink {
                a: a_run.start + p.a.start..a_run.start + p.a.end,
                b: b_run.start + p.b.start..b_run.start + p.b.end,
            })
            .collect()
    };
    let paired = |link: &BlockLink| !link.a.is_empty() && !link.b.is_empty();
    let lopsided = |found: &[BlockLink]| {
        found
            .last()
            .is_some_and(|l| l.a.len() != 1 || l.b.len() != 1)
    };

    let mut links = Vec::new();
    let mut k = 0;
    while k < blocks.len() {
        let mut end = k + 1;
        let mut found = pair(&blocks[k..end]);
        while end < blocks.len()
            && end - k < MOST_LINKS
            && paired(&blocks[end - 1])
            && paired(&blocks[end])
            && lopsided(&found)
        {
            end += 1;
            found = pair(&blocks[k..end]);
        }
        links.extend(found);
        k = end;
    }
    links
}

/// The sentences of `book`'s blocks `blocks`, in order.
fn texts<S: AsRef<str>>(book: &[Vec<S>], blocks: Range<usize>) -> Vec<&str> {
    book[blocks].iter().flatten().map(AsRef::as_ref).collect()
}

#[cfg(test)]
mod tests;
