//! A character's card as of the chapter the reader has finished: haiyaku folds
//! the book's log up to the tapped document and says which characters the
//! tapped text names, and this module answers that as the JSON the reader
//! shows (`web/src/book.ts` mirrors these types).

use std::collections::{HashMap, HashSet};
use std::iter;
use std::sync::{Arc, Mutex, PoisonError};

use haiyaku::fold::{Cast, ChapterRef, Character, EntityKey, Fact};
use haiyaku::log::{Name, Sex};
use haiyaku::store::Holding;
use haiyaku::{Mention, NamePart, Scope};
use rusqlite::{Connection, ToSql};
use serde::Serialize;

use crate::account::Access;

/// The longest text a look-up is asked about, in UTF-16 units: the client
/// cuts its text to this and the server refuses a longer one.
pub const LONGEST_TEXT: usize = 128;

/// The longest text before the tapped word a look-up is handed, in UTF-16
/// units: the client cuts what it sends to this and the server refuses more.
pub const LONGEST_BEFORE: usize = 64;

/// What a reader tapped: `text` runs from the start of the tapped word, in
/// spine document `document` of the book whose file has digest `book`.
#[derive(Debug, Clone, Copy)]
pub struct Asked<'a> {
    pub book: &'a str,
    pub document: &'a str,
    pub text: &'a str,
    /// The block's text that ends where `text` starts, from the start of a
    /// word, so a name whose later word was tapped is read from its first;
    /// empty for none.
    pub before: &'a str,
    /// Whether the books before this one in its series are folded in too.
    pub earlier: bool,
    pub series: Option<&'a str>,
    pub order: Option<&'a str>,
}

/// The characters a tapped text names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    /// Whether haiyaku holds the book at all.
    pub known: bool,
    /// The series and order folded, or `None` for the book alone.
    pub scope: Option<ScopeUsed>,
    /// Why a series fold fell back to the book alone: `"not-visible"`.
    pub narrowed: Option<&'static str>,
    /// How much text the characters' name covers from `start`, in UTF-16
    /// units, so the reader can mark it; `None` when no one is named.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length: Option<usize>,
    /// Where the name begins, in UTF-16 units from the start of the tapped
    /// text: below 0 when it begins in the text before it, above 0 when it
    /// begins inside the tapped word. Not sent when 0.
    #[serde(skip_serializing_if = "is_zero")]
    pub start: isize,
    /// Those the text names by the name their card is headed by, then those
    /// it names only by another of their names; each of the two most
    /// recently seen first, as haiyaku orders them.
    pub characters: Vec<Card>,
}

/// A series fold's series and reading order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeUsed {
    pub series: String,
    pub order: String,
}

/// One character the text names, as the chapters read show them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub name: CardName,
    pub aliases: Vec<CardName>,
    pub matched: Matched,
    pub first: ChapterAt,
    pub last: ChapterAt,
    pub relations: Vec<CardRelation>,
}

/// A name as the text gives it, with the parts haiyaku told apart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardName {
    pub text: String,
    pub given: Option<String>,
    pub surname: Option<String>,
    pub title: Option<String>,
    /// `"male"` or `"female"`: the sex a title such as "Mrs." states.
    pub title_sex: Option<&'static str>,
}

/// The name the text matched, and which part of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Matched {
    pub text: String,
    /// `"whole"`, `"given"` or `"surname"`, or `"other"` for a part a
    /// later haiyaku adds.
    pub part: &'static str,
}

/// A chapter, by haiyaku's titles of its book and of itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChapterAt {
    pub book: String,
    pub chapter: u32,
    /// Empty for a chapter the book's log does not list.
    pub chapter_title: String,
}

/// What the chapters read say of the card's character and one other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardRelation {
    /// The other character's shown name.
    pub with: CardName,
    pub facts: Vec<CardFact>,
}

/// One kind of relation between the two, with the words that state it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardFact {
    pub kind: &'static str,
    /// For a directed kind, which side it runs from: `"this"`, the card's
    /// character, or `"with"`, the other. Sent instead of haiyaku's entity
    /// ids, which mean nothing to the reader.
    pub from: Option<&'static str>,
    pub note: String,
    pub quote: String,
    pub first: ChapterAt,
    pub last: ChapterAt,
    pub overturned: Option<CardOverturn>,
}

/// The chapter read that showed a fact was never true.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardOverturn {
    pub note: String,
    pub quote: String,
    pub at: ChapterAt,
}

impl Answer {
    /// The answer for a book haiyaku does not hold.
    #[must_use]
    pub fn unknown() -> Self {
        Self {
            known: false,
            scope: None,
            narrowed: None,
            length: None,
            start: 0,
            characters: Vec::new(),
        }
    }
}

fn is_zero(start: &isize) -> bool {
    *start == 0
}

/// Each book's title and chapter titles, read from its log once and kept for
/// the `Library`'s life: haiyaku has no lighter read of them, and the how-to
/// (`docs/notes.md`) stops the server for a fill, so a restart refreshes them.
#[derive(Debug, Default)]
pub struct Titles {
    books: Mutex<HashMap<String, Arc<BookTitles>>>,
    #[cfg(test)]
    reads: std::sync::atomic::AtomicUsize,
}

/// One book's titles.
#[derive(Debug)]
pub(crate) struct BookTitles {
    title: String,
    chapters: HashMap<u32, String>,
}

impl Titles {
    /// The titles of the book with digest `sha256`, read from its log the
    /// first time they are asked for.
    pub(crate) fn of(&self, conn: &Connection, sha256: &str) -> Result<Arc<BookTitles>, String> {
        // Held across the read, so two answers in a new book read its log
        // once. A panic while held leaves the map without that book and
        // otherwise whole, so a poisoned lock is still a sound cache.
        let mut books = self.books.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(titles) = books.get(sha256) {
            return Ok(Arc::clone(titles));
        }
        #[cfg(test)]
        self.reads
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let log = haiyaku::store::read_book(conn, sha256)
            .map_err(|e| format!("reading the titles of book {sha256}: {e}"))?;
        let titles = Arc::new(BookTitles {
            title: log.book.title,
            chapters: log
                .chapters
                .into_iter()
                .map(|chapter| (chapter.number, chapter.title))
                .collect(),
        });
        books.insert(sha256.to_owned(), Arc::clone(&titles));
        Ok(titles)
    }

    /// How many logs [`Self::of`] has read.
    #[cfg(test)]
    fn reads(&self) -> usize {
        self.reads.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl BookTitles {
    fn at(&self, chapter: u32) -> ChapterAt {
        ChapterAt {
            book: self.title.clone(),
            chapter,
            chapter_title: self.chapters.get(&chapter).cloned().unwrap_or_default(),
        }
    }
}

/// Whether haiyaku holds a log of the book with digest `sha256`.
///
/// # Errors
///
/// When haiyaku's tables cannot be read.
pub fn holds(conn: &Connection, sha256: &str) -> Result<bool, String> {
    match haiyaku::store::series_holding(conn, sha256) {
        Ok(_) => Ok(true),
        Err(haiyaku::Error::NotFound(_)) => Ok(false),
        Err(e) => Err(format!("reading the series holding book {sha256}: {e}")),
    }
}

/// The scope a tap folds: the book alone unless earlier books are asked for
/// and the book is in a series. Then the series named, if it holds the book,
/// else the first of `holding`, and the order named, if that series has it
/// holding the book, else its first-ranked; a series and an order that no
/// longer hold the book fall back rather than fail.
///
/// The order is always named: `cast_at`'s own default is the series'
/// first-ranked order even where that order lacks the book, and then the
/// fold fails.
pub(crate) fn choose_scope(
    holding: &[Holding],
    earlier: bool,
    series: Option<&str>,
    order: Option<&str>,
) -> Scope {
    if !earlier {
        return Scope::Book;
    }
    let Some(first) = holding.first() else {
        return Scope::Book;
    };
    let held = series
        .and_then(|named| holding.iter().find(|holds| holds.series == named))
        .unwrap_or(first);
    let order = order
        .and_then(|named| held.orders.iter().find(|name| *name == named))
        .or_else(|| held.orders.first());
    Scope::Series {
        series: held.series.clone(),
        order: order.cloned(),
    }
}

/// The digests among `books` of the books `access` sees: those of a member
/// in a directory it reaches, by the rule browse counts by. `Access::Every`
/// adds no clause, so a book the library does not hold is seen by no one.
///
/// # Errors
///
/// When the library cannot be read.
pub(crate) fn books_seen(
    conn: &Connection,
    access: &Access,
    books: &[String],
) -> Result<HashSet<String>, String> {
    let grant = access.grant_clause(|holes| crate::browse::member_under("m", holes));
    let sql = format!(
        "SELECT DISTINCT m.sha256 FROM member m WHERE m.sha256 IN ({}){grant}",
        crate::browse::holes(books.len())
    );
    // Plain `?` holes bind in the order the text holds them: the digests'
    // list, then the grant's.
    let mut values: Vec<&dyn ToSql> = books.iter().map(|book| book as &dyn ToSql).collect();
    values.extend(access.bound().iter().map(|root| root as &dyn ToSql));
    let mut seen = conn
        .prepare(&sql)
        .map_err(|e| format!("reading which books the account sees: {e}"))?;
    seen.query_map(values.as_slice(), |row| row.get(0))
        .and_then(Iterator::collect)
        .map_err(|e| format!("reading which books the account sees: {e}"))
}

/// The characters `asked.text` names, as the chapters finished before
/// `asked.document` show them, with the books before it in its series when
/// `asked.earlier` and `access` sees every one of them. The cut is haiyaku's:
/// it is handed the document and folds no chapter the reader is still in.
///
/// # Errors
///
/// When haiyaku's tables, or the library's directories, cannot be read.
pub fn answer(
    conn: &Connection,
    asked: &Asked<'_>,
    access: &Access,
    titles: &Titles,
) -> Result<Answer, String> {
    let holding = match haiyaku::store::series_holding(conn, asked.book) {
        Ok(holding) => holding,
        Err(haiyaku::Error::NotFound(_)) => return Ok(Answer::unknown()),
        Err(e) => {
            return Err(format!(
                "reading the series holding book {}: {e}",
                asked.book
            ));
        }
    };
    let scope = choose_scope(&holding, asked.earlier, asked.series, asked.order);
    let Some(Folded {
        cast,
        scope,
        narrowed,
    }) = fold(conn, asked, access, scope)?
    else {
        // A document the book's log does not list, such as a spine item
        // haiyaku skipped: the book is known, and nothing is folded there.
        return Ok(Answer {
            known: true,
            ..Answer::unknown()
        });
    };
    let (mut mentions, start, length) = named(&cast, asked.before, asked.text);
    // A card headed by another name than the one tapped, coming first, reads
    // as the tapped name not being known as a name. The sort is stable, so
    // on either side haiyaku's order stands.
    mentions.sort_by_key(|mention| *mention.name != mention.character.name);
    let chapters = Chapters { conn, titles };
    let characters = mentions
        .iter()
        .map(|mention| card(mention, &cast, &chapters))
        .collect::<Result<_, _>>()?;
    Ok(Answer {
        known: true,
        scope,
        narrowed,
        length,
        start,
        characters,
    })
}

/// Whom a tap names, with where their name starts and how long it is, as
/// [`Answer`] counts the two; no one, 0 and `None` where no name is found.
///
/// haiyaku reads a name from the place it is handed onward, so a tap on a
/// later word of a name finds the name only when read from its first word,
/// and a tap on a stuttered word ("Ne-Ne-Ned") only when read from its last
/// part. [`places`] gives the places in the order tried, and the first that
/// names someone whose name reaches into `text` answers.
fn named<'a>(cast: &'a Cast, before: &str, text: &str) -> (Vec<Mention<'a>>, isize, Option<usize>) {
    let around = [before, text].concat();
    let tapped = before.len();
    // The reader counts its text in UTF-16 units.
    let units = |text: &str| text.encode_utf16().count();
    places(before, text)
        .find_map(|from| {
            let mentions = haiyaku::characters_named(cast, &around[from..]);
            // Every mention covers the same text, so the first measures them
            // all. A mention's `len` ends a match in the text haiyaku was
            // handed, so the slice below cannot split a character.
            let to = from + mentions.first()?.len;
            // A name that ends before the tapped text is not the one tapped.
            (to > tapped).then(|| {
                let start = units(&around[..from]).cast_signed() - units(before).cast_signed();
                (mentions, start, Some(units(&around[from..to])))
            })
        })
        .unwrap_or_default()
}

/// Where in `before` followed by `text` a look-up is tried from, as byte
/// offsets in the order tried: the first letter or digit of each word of
/// `before` (a word is what white space parts), earliest first, so that of
/// two names holding the tapped word the one that starts earlier answers;
/// the start of `text`, whatever it begins with; then, inside the tapped
/// word, the first letter or digit past each run of hyphens, earliest first.
fn places(before: &str, text: &str) -> impl Iterator<Item = usize> {
    let tapped = before.len();
    // The tapped word alone: a hyphen in a later word is in no word the
    // reader touched.
    let word = text.split(char::is_whitespace).next().unwrap_or_default();
    let past_hyphens = first_letters(word, false, |c| matches!(c, '-' | '\u{2010}' | '\u{2011}'));
    first_letters(before, true, char::is_whitespace)
        .chain(iter::once(tapped))
        .chain(past_hyphens.map(move |at| tapped + at))
}

/// The byte offset of the first letter or digit past each run of characters
/// that `parts` holds for, and of the text's first of all when `at_start`;
/// none past a run that no letter or digit follows before the next run.
///
/// A letter or digit, never the mark before it: haiyaku passes over what
/// stands before a text's first word and counts it inside the match, so
/// the reader would mark a quotation mark or a dash as part of the name.
fn first_letters(
    text: &str,
    at_start: bool,
    parts: impl Fn(char) -> bool,
) -> impl Iterator<Item = usize> {
    let mut wanted = at_start;
    text.char_indices().filter_map(move |(at, c)| {
        let first = wanted && c.is_alphanumeric();
        wanted = parts(c) || (wanted && !first);
        first.then_some(at)
    })
}

/// A cast, with the scope and the narrowing the answer names it by.
struct Folded {
    cast: Cast,
    scope: Option<ScopeUsed>,
    narrowed: Option<&'static str>,
}

/// The cast `scope` folds at the tapped document, or `None` where haiyaku
/// folds nothing there. A series fold that takes in a book `access` does not
/// see is folded again over the book alone: an account limited to some
/// libraries must not read, on a card, quotes from a book outside them.
fn fold(
    conn: &Connection,
    asked: &Asked<'_>,
    access: &Access,
    scope: Scope,
) -> Result<Option<Folded>, String> {
    let Some(cast) = cast_at(conn, asked, &scope)? else {
        return Ok(None);
    };
    let Scope::Series { series, order } = scope else {
        return Ok(Some(Folded {
            cast,
            scope: None,
            narrowed: None,
        }));
    };
    let seen = books_seen(conn, access, &cast.books)?;
    if cast.books.iter().all(|book| seen.contains(book)) {
        return Ok(Some(Folded {
            cast,
            scope: Some(ScopeUsed {
                series,
                // `choose_scope` names the order of every series it chooses.
                order: order.unwrap_or_default(),
            }),
            narrowed: None,
        }));
    }
    Ok(cast_at(conn, asked, &Scope::Book)?.map(|cast| Folded {
        cast,
        scope: None,
        narrowed: Some("not-visible"),
    }))
}

/// haiyaku's fold over `scope` at the tapped document, or `None` for its
/// `NotFound`.
fn cast_at(conn: &Connection, asked: &Asked<'_>, scope: &Scope) -> Result<Option<Cast>, String> {
    match haiyaku::cast_at(conn, asked.book, asked.document, scope) {
        Ok(cast) => Ok(Some(cast)),
        Err(haiyaku::Error::NotFound(_)) => Ok(None),
        Err(e) => Err(format!(
            "folding book {} at {}: {e}",
            asked.book, asked.document
        )),
    }
}

/// Names chapters by haiyaku's titles of their books.
struct Chapters<'a> {
    conn: &'a Connection,
    titles: &'a Titles,
}

impl Chapters<'_> {
    fn at(&self, chapter: &ChapterRef) -> Result<ChapterAt, String> {
        Ok(self
            .titles
            .of(self.conn, &chapter.book)?
            .at(chapter.chapter))
    }
}

fn card(mention: &Mention<'_>, cast: &Cast, chapters: &Chapters<'_>) -> Result<Card, String> {
    let character = mention.character;
    Ok(Card {
        name: card_name(&character.name),
        aliases: character.aliases.iter().map(card_name).collect(),
        matched: Matched {
            text: mention.name.text.clone(),
            part: part_name(mention.part),
        },
        first: chapters.at(&character.first)?,
        last: chapters.at(&character.last)?,
        relations: relations_of(character, cast, chapters)?,
    })
}

/// The cast's relations that hold `character` on either side, in the cast's
/// order.
fn relations_of(
    character: &Character,
    cast: &Cast,
    chapters: &Chapters<'_>,
) -> Result<Vec<CardRelation>, String> {
    let mut relations = Vec::new();
    for relation in &cast.relations {
        let other = if relation.a == character.key {
            &relation.b
        } else if relation.b == character.key {
            &relation.a
        } else {
            continue;
        };
        // haiyaku relates only characters of its cast; a pair it did not
        // would have no name to show.
        let Some(with) = cast.characters.iter().find(|shown| shown.key == *other) else {
            continue;
        };
        let facts = relation
            .facts
            .iter()
            .map(|fact| card_fact(fact, &character.key, chapters))
            .collect::<Result<_, _>>()?;
        relations.push(CardRelation {
            with: card_name(&with.name),
            facts,
        });
    }
    Ok(relations)
}

fn card_fact(fact: &Fact, this: &EntityKey, chapters: &Chapters<'_>) -> Result<CardFact, String> {
    let overturned = match &fact.overturned {
        Some(overturn) => Some(CardOverturn {
            note: overturn.note.clone(),
            quote: overturn.quote.clone(),
            at: chapters.at(&overturn.at)?,
        }),
        None => None,
    };
    Ok(CardFact {
        kind: fact.kind.as_str(),
        from: fact
            .from
            .as_ref()
            .map(|from| if from == this { "this" } else { "with" }),
        note: fact.note.clone(),
        quote: fact.quote.clone(),
        first: chapters.at(&fact.first)?,
        last: chapters.at(&fact.last)?,
        overturned,
    })
}

fn card_name(name: &Name) -> CardName {
    CardName {
        text: name.text.clone(),
        given: name.given.clone(),
        surname: name.surname.clone(),
        title: name.title.clone(),
        title_sex: name.title_sex.map(|sex| match sex {
            Sex::Male => "male",
            Sex::Female => "female",
        }),
    }
}

fn part_name(part: NamePart) -> &'static str {
    match part {
        NamePart::Whole => "whole",
        NamePart::Given => "given",
        NamePart::Surname => "surname",
        // `NamePart` is non-exhaustive: a part a later haiyaku adds still
        // answers, and the reader shows it as a match of no named part.
        _ => "other",
    }
}

#[cfg(test)]
mod tests;
