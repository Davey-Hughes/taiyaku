//! Korean words as a book prints them, with a particle on: 해리가, 학교에서.
//!
//! **Particles never come off a word with an entry of its own**, so 바다 is
//! the sea and never 바 with 다 on it. A word found only as a form of another
//! is stripped as well, since 그는 is a form of 글다 and also 그 with 는 on.
//! Each particle is taken off only where it agrees with the stem's final
//! consonant, which keeps 집는 from finding 집. **An affix is no stem**: its
//! entry answers only for the word as written, so 거라 shows the ending -거라
//! and 거라는 is 거 with 라는 on.

use rusqlite::Connection;

use super::{Found, key, lookup};

/// How many particles may be stacked on a stem: 학교에서는 is two.
const DEPTH: usize = 3;

/// What a particle needs of the stem's last syllable.
#[derive(Clone, Copy)]
enum Needs {
    Consonant,
    /// The 으로-forms: 로 itself follows a final ㄹ, so 으로 does not.
    ConsonantNotRieul,
    Vowel,
    /// The 로-forms, which follow a vowel or a final ㄹ.
    VowelOrRieul,
    Either,
}

/// Shortest first, which is the order `strip` tries them in.
const PARTICLES: &[(&str, Needs)] = &[
    ("은", Needs::Consonant),
    ("이", Needs::Consonant),
    ("을", Needs::Consonant),
    ("과", Needs::Consonant),
    ("는", Needs::Vowel),
    ("가", Needs::Vowel),
    ("를", Needs::Vowel),
    ("와", Needs::Vowel),
    ("로", Needs::VowelOrRieul),
    ("랑", Needs::Vowel),
    ("나", Needs::Vowel),
    ("다", Needs::Vowel),
    ("의", Needs::Either),
    ("에", Needs::Either),
    ("께", Needs::Either),
    ("도", Needs::Either),
    ("만", Needs::Either),
    ("들", Needs::Either),
    ("야", Needs::Vowel),
    ("인", Needs::Either),
    ("으로", Needs::ConsonantNotRieul),
    ("이랑", Needs::Consonant),
    ("이나", Needs::Consonant),
    ("이다", Needs::Either),
    ("로는", Needs::VowelOrRieul),
    ("로서", Needs::VowelOrRieul),
    ("로써", Needs::VowelOrRieul),
    ("예요", Needs::Vowel),
    ("라고", Needs::Vowel),
    ("에서", Needs::Either),
    ("에게", Needs::Either),
    ("한테", Needs::Either),
    ("께서", Needs::Either),
    ("까지", Needs::Either),
    ("부터", Needs::Either),
    ("처럼", Needs::Either),
    ("보다", Needs::Either),
    ("하고", Needs::Either),
    ("마다", Needs::Either),
    ("조차", Needs::Either),
    ("밖에", Needs::Either),
    ("들은", Needs::Either),
    ("들이", Needs::Either),
    ("들을", Needs::Either),
    ("들의", Needs::Either),
    ("였다", Needs::Vowel),
    ("였고", Needs::Vowel),
    ("이야", Needs::Consonant),
    ("라는", Needs::Vowel),
    ("라도", Needs::Vowel),
    ("이고", Needs::Either),
    ("만큼", Needs::Either),
    ("대로", Needs::Either),
    ("으로는", Needs::ConsonantNotRieul),
    ("으로서", Needs::ConsonantNotRieul),
    ("으로써", Needs::ConsonantNotRieul),
    ("이에요", Needs::Consonant),
    ("입니다", Needs::Either),
    ("이었다", Needs::Consonant),
    ("이라고", Needs::Consonant),
    ("에서는", Needs::Either),
    ("에서도", Needs::Either),
    ("에게서", Needs::Either),
    ("에게는", Needs::Either),
    ("한테서", Needs::Either),
    ("들에게", Needs::Either),
    ("였는데", Needs::Vowel),
    ("이었고", Needs::Consonant),
    ("이라는", Needs::Consonant),
    ("이라도", Needs::Consonant),
    ("이었는데", Needs::Consonant),
];

/// The pronouns that, with 는 or 를 on, spell a form of a rarer word: 그는 is
/// also a form of 글다 and 나를 of 나르다, and a book nearly always means the
/// pronoun.
const PRONOUNS: &[&str] = &["그", "나", "너", "우리"];

/// What `conn` has for a Korean `word`. A word with an entry under its own key
/// is answered as [`lookup`] answers it. Any other word has particles taken
/// off, each headword so found giving the word's key in `via`; a word found
/// only as a form of something is stripped too, its results after the form's,
/// except that a pronoun with 는 or 를 on lists the pronoun first.
///
/// # Errors
///
/// As [`lookup`].
pub fn lookup_korean(conn: &Connection, word: &str) -> Result<Vec<Found>, String> {
    let mut found = lookup(conn, word)?;
    // Only a form-table hit has a `via`: a hit under the word's own key is
    // the word, and 바다 must stay the sea rather than 바 with 다 on it.
    if found.iter().any(|f| f.via.is_none()) {
        return Ok(found);
    }
    let written = key(word);
    let mut stripped = strip(conn, &written, DEPTH)?;
    // A stem that was itself a form would say the stem; the reader met the
    // whole word, so that is what the panel names.
    for f in &mut stripped {
        f.via = Some(written.clone());
    }
    if is_pronoun_with_topic_or_object(&written) {
        std::mem::swap(&mut found, &mut stripped);
    }
    for f in stripped {
        if !found.contains(&f) {
            found.push(f);
        }
    }
    Ok(found)
}

/// Whether `written` is one of [`PRONOUNS`] with 는 or 를 on. No other particle
/// counts: 나와 is usually 나오다, so there the verb stays first.
fn is_pronoun_with_topic_or_object(written: &str) -> bool {
    ["는", "를"].iter().any(|particle| {
        written
            .strip_suffix(particle)
            .is_some_and(|stem| PRONOUNS.contains(&stem))
    })
}

/// The first stem of `word`, shortest strip first, that finds anything but
/// an affix.
fn strip(conn: &Connection, word: &str, depth: usize) -> Result<Vec<Found>, String> {
    if depth == 0 {
        return Ok(Vec::new());
    }
    for (particle, needs) in PARTICLES {
        let Some(stem) = word.strip_suffix(particle) else {
            continue;
        };
        if stem.is_empty() || !agrees(stem, *needs) {
            continue;
        }
        let found = readings_of_stem(conn, stem)?;
        if !found.is_empty() {
            return Ok(found);
        }
        let found = strip(conn, stem, depth - 1)?;
        if !found.is_empty() {
            return Ok(found);
        }
    }
    Ok(Vec::new())
}

/// What `conn` has for `stem`, less its affixes. [`key`] trims a headword's
/// hyphen, so the suffix -거라 is filed under 거라, where it would answer for
/// 거라는 in place of 거.
fn readings_of_stem(conn: &Connection, stem: &str) -> Result<Vec<Found>, String> {
    let mut found = lookup(conn, stem)?;
    found.retain(|f| !is_affix(&f.word));
    Ok(found)
}

/// Whether `headword` is written as a prefix, suffix or infix, with a hyphen
/// on the side a stem would join.
fn is_affix(headword: &str) -> bool {
    headword.starts_with('-') || headword.ends_with('-')
}

/// Whether a particle needing `needs` may follow `stem`. A stem not ending in
/// a Hangul syllable has no final consonant to read, so it takes either.
fn agrees(stem: &str, needs: Needs) -> bool {
    let Some(last) = stem.chars().last() else {
        return false;
    };
    let code = last as u32;
    if !(0xAC00..=0xD7A3).contains(&code) {
        return true;
    }
    let final_consonant = (code - 0xAC00) % 28;
    match needs {
        Needs::Either => true,
        Needs::Consonant => final_consonant != 0,
        Needs::ConsonantNotRieul => final_consonant != 0 && final_consonant != 8,
        Needs::Vowel => final_consonant == 0,
        Needs::VowelOrRieul => final_consonant == 0 || final_consonant == 8,
    }
}

#[cfg(test)]
mod tests;
