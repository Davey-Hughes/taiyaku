//! Dictionaries a book's words are looked up in, one SQLite file a language,
//! built from Wiktionary's entries as kaikki.org extracts them.
//!
//! **Kept apart from the library's store**, which a backup copies whole: a
//! language's dictionary is far larger than a library's records and is built
//! again from its source, never edited.
//!
//! **A form finds its headword.** A reader meets `hablaron` and `говорит`,
//! and a dictionary lists `hablar` and `говорить`; Wiktionary records which
//! forms belong to which word, so a lookup goes from the word read to the
//! word listed and says which form led there.

use std::io::BufRead;

use rusqlite::{Connection, params};

mod korean;

pub use korean::lookup_korean;

/// What a lookup finds: a headword, its part of speech and its first senses,
/// and the form of it that was looked up where that was not the headword.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub word: String,
    pub pos: String,
    pub glosses: Vec<String>,
    pub via: Option<String>,
}

/// What an import wrote, and how many lines it could not read.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Imported {
    pub entries: usize,
    pub forms: usize,
    pub skipped: usize,
}

/// The senses kept of a word, the first in Wiktionary's order: a lookup is
/// read beside the text, at a glance, so a word's first few senses serve it
/// and the rest only lengthen the panel. Chosen, not measured.
const MOST_GLOSSES: usize = 5;

/// Form tags that name no form a text prints: a spelling in another script,
/// and the labels of Wiktionary's inflection tables.
const NOT_PRINTED: &[&str] = &["romanization", "table-tags", "inflection-template", "class"];

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS entry (
      key     TEXT NOT NULL,
      word    TEXT NOT NULL,
      pos     TEXT NOT NULL,
      glosses TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS entry_by_key ON entry (key);
    CREATE TABLE IF NOT EXISTS form (
      form  TEXT NOT NULL,
      lemma TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS form_by_form ON form (form);
";

/// A word as it is looked up: without the punctuation round it, in lower
/// case, and without the stress marks Wiktionary writes on Russian and its
/// books do not print.
#[must_use]
pub fn key(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric())
        .chars()
        .filter(|c| !matches!(c, '\u{300}' | '\u{301}'))
        .collect::<String>()
        .to_lowercase()
}

/// Fill `conn` from kaikki.org's JSON lines, one entry a line, in one
/// transaction. A line that is not an entry is counted and passed over.
///
/// # Errors
///
/// If reading fails, or SQLite refuses the schema or a write.
pub fn import(conn: &Connection, lines: impl BufRead) -> Result<Imported, String> {
    conn.execute_batch(SCHEMA)
        .map_err(|e| format!("making the dictionary's tables: {e}"))?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the import: {e}"))?;
    let mut done = Imported::default();
    {
        let mut entry = tx
            .prepare("INSERT INTO entry (key, word, pos, glosses) VALUES (?1, ?2, ?3, ?4)")
            .map_err(|e| format!("preparing an entry: {e}"))?;
        let mut form = tx
            .prepare("INSERT INTO form (form, lemma) VALUES (?1, ?2)")
            .map_err(|e| format!("preparing a form: {e}"))?;
        for line in lines.lines() {
            let line = line.map_err(|e| format!("reading the dictionary: {e}"))?;
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                done.skipped += 1;
                continue;
            };
            let Some(word) = value["word"].as_str() else {
                done.skipped += 1;
                continue;
            };
            let word_key = key(word);
            let mut glosses = Vec::new();
            for sense in value["senses"].as_array().into_iter().flatten() {
                let lemmas: Vec<&str> = sense["form_of"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|f| f["word"].as_str())
                    .collect();
                // A sub-sense lists its parent's gloss before its own, so its
                // own is the last, and a parent met again is passed over.
                let own = sense["glosses"]
                    .as_array()
                    .and_then(|g| g.last())
                    .and_then(serde_json::Value::as_str);
                if let Some(own) =
                    own.filter(|g| lemmas.is_empty() && !glosses.iter().any(|h| h == g))
                {
                    glosses.push(own.to_string());
                }
                for lemma in lemmas {
                    form.execute(params![word_key, key(lemma)])
                        .map_err(|e| format!("writing a form of {lemma}: {e}"))?;
                    done.forms += 1;
                }
            }
            for f in value["forms"].as_array().into_iter().flatten() {
                let printed = !f["tags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_str)
                    .any(|t| NOT_PRINTED.contains(&t));
                if let Some(text) = f["form"].as_str().filter(|_| printed) {
                    let form_key = key(text);
                    if !form_key.is_empty() && form_key != word_key {
                        form.execute(params![form_key, word_key])
                            .map_err(|e| format!("writing a form of {word}: {e}"))?;
                        done.forms += 1;
                    }
                }
            }
            if !glosses.is_empty() {
                glosses.truncate(MOST_GLOSSES);
                let pos = value["pos"].as_str().unwrap_or("");
                let glosses = serde_json::to_string(&glosses).unwrap_or_default();
                entry
                    .execute(params![word_key, word, pos, glosses])
                    .map_err(|e| format!("writing {word}: {e}"))?;
                done.entries += 1;
            }
        }
    }
    tx.commit()
        .map_err(|e| format!("finishing the import: {e}"))?;
    Ok(done)
}

/// What `conn` has for `word`: its own entries, then those of each headword
/// it is a form of, each saying the form it was found by.
///
/// # Errors
///
/// If SQLite refuses a read, the dictionary having no tables among the causes.
pub fn lookup(conn: &Connection, word: &str) -> Result<Vec<Found>, String> {
    let looked = key(word);
    if looked.is_empty() {
        return Ok(Vec::new());
    }
    let mut found = entries(conn, &looked, None)?;
    let mut lemmas = conn
        .prepare("SELECT DISTINCT lemma FROM form WHERE form = ?1 AND lemma != ?1")
        .map_err(|e| format!("reading forms: {e}"))?;
    let lemmas: Vec<String> = lemmas
        .query_map([&looked], |r| r.get(0))
        .map_err(|e| format!("reading forms: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("reading forms: {e}"))?;
    for lemma in lemmas {
        found.extend(entries(conn, &lemma, Some(&looked))?);
    }
    Ok(found)
}

/// The most chars of a text `longest_at` tries as one word. A tap looks up
/// the word it lands on, and 16 chars covers a word with its inflection; a
/// longer set phrase is left to a drag-select and the existing look-up. It
/// also bounds the lookups a tap costs.
pub const LONGEST_PREFIX: usize = 16;

/// Whether a char ends a word. A word goes on through an alphanumeric char
/// (a mark with the Alphabetic property among them), a combining mark
/// `is_combining_mark` names, and a variation selector; every other char
/// ends it. Without a Unicode-category crate that is how whitespace,
/// punctuation and symbols are told, so a non-alphabetic mark outside those
/// ranges and a format char, a zero-width space or joiner among them, end a
/// word too.
fn ends_word(c: char) -> bool {
    c.is_whitespace()
        || (!c.is_alphanumeric() && !is_combining_mark(c) && !is_variation_selector(c))
}

/// A combining mark known without a Unicode-category crate: the diacritics
/// U+0300 to U+036F, Russian's stress marks among them, and the kana voicing
/// marks U+3099 and U+309A. It belongs to the char before it, so a word goes
/// on through one and a tap's match never ends before one: まと and a
/// decomposed まど differ only in the mark.
fn is_combining_mark(c: char) -> bool {
    matches!(c, '\u{300}'..='\u{36F}' | '\u{3099}' | '\u{309A}')
}

/// Whether a char after a word is read as part of it: a variation selector,
/// or a stress mark `key` strips, which a learner's book prints on a word its
/// dictionary lists without one.
fn is_taken_into_match(c: char) -> bool {
    is_variation_selector(c) || matches!(c, '\u{300}' | '\u{301}')
}

/// A variation selector: U+FE00 to U+FE0F pick a standardized variant of the
/// char before them, U+FE0E and U+FE0F its text or emoji form for any char,
/// and U+E0100 to U+E01EF an ideograph's variant. One changes the glyph, not
/// the word, so `key` trims it at a word's end with the rest of what is not
/// alphanumeric, and a tap's match takes in those after it.
fn is_variation_selector(c: char) -> bool {
    matches!(c, '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
}

/// The longest word `text` begins with, its length in chars and what
/// `lookup` finds for it, found by trying each prefix from the longest down.
/// The length takes in the variation selectors and stress marks after the
/// word, and a prefix any other combining mark then follows is not tried.
/// Nothing matching gives `(0, [])`.
///
/// # Errors
///
/// If SQLite refuses a read, the dictionary having no tables among the causes.
pub fn longest_at(conn: &Connection, text: &str) -> Result<(usize, Vec<Found>), String> {
    let chars: Vec<char> = text
        .chars()
        .take(LONGEST_PREFIX)
        .take_while(|&c| !ends_word(c))
        .collect();
    for length in (1..=chars.len()).rev() {
        // Read from `text`, not `chars`: what follows the sixteenth char
        // still belongs to it.
        let taken_in = text
            .chars()
            .skip(length)
            .take_while(|&c| is_taken_into_match(c))
            .count();
        if text
            .chars()
            .nth(length + taken_in)
            .is_some_and(is_combining_mark)
        {
            continue;
        }
        let prefix: String = chars[..length].iter().collect();
        // `lookup` trims and strips stress marks, so a prefix it would answer
        // under another spelling is not this text's word.
        if key(&prefix) != prefix.to_lowercase() {
            continue;
        }
        let found = lookup(conn, &prefix)?;
        if !found.is_empty() {
            return Ok((length + taken_in, found));
        }
    }
    Ok((0, Vec::new()))
}

/// The entries listed under `key`, found by `via` where it is a form of them.
fn entries(conn: &Connection, key: &str, via: Option<&str>) -> Result<Vec<Found>, String> {
    let mut stmt = conn
        .prepare("SELECT word, pos, glosses FROM entry WHERE key = ?1 ORDER BY rowid")
        .map_err(|e| format!("reading entries: {e}"))?;
    stmt.query_map([key], |r| {
        let glosses: String = r.get(2)?;
        Ok(Found {
            word: r.get(0)?,
            pos: r.get(1)?,
            glosses: serde_json::from_str(&glosses).unwrap_or_default(),
            via: via.map(str::to_string),
        })
    })
    .map_err(|e| format!("reading entries: {e}"))?
    .collect::<Result<_, _>>()
    .map_err(|e| format!("reading entries: {e}"))
}

#[cfg(test)]
mod tests;
