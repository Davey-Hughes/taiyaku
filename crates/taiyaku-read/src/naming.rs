//! Ordering page and chapter names the way a reader would.

use std::cmp::Ordering;

/// One token of a name: a run of digits read as a number, or anything else.
///
/// Digit runs carry an optional `.digits` tail so `005.5` reads as one number
/// rather than as `5`, `.`, `5`. Without that, ordinary natural sorting still
/// puts `c005.5` before `c005`, because it ends up comparing the separator `.`
/// against the extension `.cbz`.
#[derive(Debug, PartialEq)]
enum Token {
    Number(f64),
    Text(String),
}

fn tokenize(name: &str) -> Vec<Token> {
    let chars: Vec<char> = name.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        if chars[i].is_ascii_digit() {
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            // A decimal point counts only when digits follow it.
            if i + 1 < chars.len() && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let text: String = chars[start..i].iter().collect();
            tokens.push(Token::Number(text.parse().unwrap_or(f64::MAX)));
        } else {
            while i < chars.len() && !chars[i].is_ascii_digit() {
                i += 1;
            }
            tokens.push(Token::Text(
                chars[start..i].iter().collect::<String>().to_lowercase(),
            ));
        }
    }
    tokens
}

/// The volume number in a directory's name.
///
/// Takes the first digit run immediately preceded — optionally across one space
/// — by `v`, `vol`, `vol.` or `第`, and falls back to the first digit run when
/// none of those appear. Leading zeros are stripped, so `v007`, `v7` and `第7巻`
/// group together.
///
/// The marker is searched for before the fallback is used because a number
/// appearing *before* the volume marker — `[2015] ONE PIECE v077` — would
/// otherwise be read from its first digit run (2015) rather than the volume
/// the name actually carries (77). A year in trailing parentheses, `ONE PIECE
/// v077 (2015)`, does not test this: `077` is already the first digit run
/// there, so the marker scan and a bare first-digit-run rule agree on it
/// regardless of which one runs.
///
/// **The scanner does not call this one.** Its `builtin-volume` rule calls
/// [`volume_number_marked`] below, because the fallback above is wrong for a
/// rule applied to every filename in a library: `Kaguya-sama - c001.cbz` has
/// no volume marker, and answering `1` for it gives every chapter file a
/// volume it does not have. This function keeps the fallback because both of
/// its callers — `inventory::walk`, and `taiyaku-cli`'s
/// `the_inventory_finds_every_duplicate_set` corpus check, which walks the
/// same `op-duplicates` directories — are handed something that *is* a
/// volume by construction, so a bare `069` is a volume number there.
///
/// Deliberately not a regex, and it stays that way now that the workspace
/// has one. `regex` arrived in `taiyaku-library` with the scanner's rule
/// layer on 2026-08-17 — the evidence the paragraph this replaces asked for
/// — but this is hand-written, measured on a 579-directory One Piece run,
/// and carries a guard a regex in a text box will not, since a year *before*
/// the marker (`[2015] ONE PIECE v077`) must not be read as the volume.
/// Re-expressing it as a default regex would be a second implementation of a
/// measured behaviour, and it would drift.
pub fn volume_number(name: &str) -> Option<u32> {
    const MARKERS: [&str; 4] = ["vol.", "vol", "v", "第"];

    let chars: Vec<char> = name.chars().collect();
    let mut fallback = None;
    let mut at = 0;
    while at < chars.len() {
        if !chars[at].is_ascii_digit() {
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && chars[at].is_ascii_digit() {
            at += 1;
        }
        let number: Option<u32> = chars[start..at].iter().collect::<String>().parse().ok();
        if fallback.is_none() {
            fallback = number;
        }
        let before = chars[..start].iter().collect::<String>().to_lowercase();
        let before = before.strip_suffix(' ').unwrap_or(&before);
        if MARKERS.iter().any(|marker| before.ends_with(marker)) {
            return number;
        }
    }
    fallback
}

/// A marker that introduces a number, and the suffix that must follow the
/// digits for it to count.
///
/// **`after` exists for `第` alone, and it is not optional decoration.** `第`
/// is an ordinal prefix, not a unit: `第69巻` is volume 69 and `第7話` is
/// chapter 7, and the counter that disambiguates them — `巻` for volumes,
/// `話` or `章` for chapters — comes *after* the digits. With `第` in both
/// marker sets and nothing looking past the number, every Japanese-named file
/// under the seeded defaults took a volume number and a chapter number with
/// the same value: measured, `ワンピース/第69巻.cbz` resolved to
/// `vol=Some(69.0) chap=Some(69.0)`. That is the defect Step 0 exists to
/// close, reached from the other side.
///
/// A bare `第7` with no counter is genuinely ambiguous and answers neither. A
/// layout that writes one is served by a `regex` rule, which is what the rule
/// layer's escape hatch is for.
struct Marker {
    before: &'static str,
    after: Option<&'static str>,
}

/// Volume markers. Unlike a regex alternation, the order here decides
/// nothing: [`marked_number`] checks every marker with `.any()` and returns
/// the same number whichever one matches, so there is no "longest match
/// wins" to preserve by ordering `vol.` before `vol`.
const VOLUME_MARKERS: [Marker; 4] = [
    Marker {
        before: "vol.",
        after: None,
    },
    Marker {
        before: "vol",
        after: None,
    },
    Marker {
        before: "v",
        after: None,
    },
    Marker {
        before: "第",
        after: Some("巻"),
    },
];

/// Chapter markers. `c` and `ch` are short enough to need the word-boundary
/// test in [`marked_number`]; see its doc. `話` and `章` both count chapters.
///
/// **`話` and `巻` are measured**, at 577 and 534 `第N…` basenames in the
/// development corpus, re-measured 2026-08-18 with the method written down —
/// see `docs/notes.md`'s kanji counter survey, which also records that the
/// `巻` figure this replaces (498) reproduces under no counting method at
/// all, while 577 holds under every one. `章` is included on knowledge of
/// the language rather than measurement — it appears zero times there.
/// `回` and `卷` are deliberately absent for the same reason: also measured
/// at zero, and adding either is a two-line change once a corpus shows one.
/// `章` stays despite scoring the same zero, because it can only ever fire
/// on an explicit `第N章` — it cannot produce a false positive the way a
/// wrong marker could — and dropping a correct Japanese counter to match
/// one corpus would be overfitting in the other direction.
const CHAPTER_MARKERS: [Marker; 9] = [
    Marker {
        before: "chapter",
        after: None,
    },
    Marker {
        before: "chp.",
        after: None,
    },
    Marker {
        before: "chp",
        after: None,
    },
    Marker {
        before: "ch.",
        after: None,
    },
    Marker {
        before: "ch",
        after: None,
    },
    Marker {
        before: "c.",
        after: None,
    },
    Marker {
        before: "c",
        after: None,
    },
    Marker {
        before: "第",
        after: Some("話"),
    },
    Marker {
        before: "第",
        after: Some("章"),
    },
];

/// The first digit run introduced by one of `markers`, with an optional
/// decimal part, and **no fallback**.
///
/// The scan `chapter_number` has always used, with the marker set lifted out
/// so `volume_number_marked` can share it rather than carry a second copy.
///
/// **A marker must start a word.** `c` and `v` are one character, so without
/// this they match the tail of ordinary words: `Scan by ABC 2019` reads as
/// 2019 because `abc` ends in `c`, and `Series Nov 7` reads as 7 because
/// `nov` ends in `v`. Both measured on the built function.
///
/// ASCII-only, because every kanji is alphanumeric to Unicode and
/// `単行本第7話` has to keep working.
///
/// `f64` because both callers want one: `member.chapter_declared` is `REAL`
/// for half-chapters, and `work.volume` is `REAL` because — in that column's
/// own words — "omnibus and half-volumes exist".
fn marked_number(name: &str, markers: &[Marker]) -> Option<f64> {
    let chars: Vec<char> = name.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        if !chars[at].is_ascii_digit() {
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && chars[at].is_ascii_digit() {
            at += 1;
        }
        // One decimal part, and only when a digit follows the point — so
        // `c001.cbz` reads as 1 rather than tripping over the extension.
        if chars.get(at) == Some(&'.') && chars.get(at + 1).is_some_and(char::is_ascii_digit) {
            at += 1;
            while at < chars.len() && chars[at].is_ascii_digit() {
                at += 1;
            }
        }
        let number: Option<f64> = chars[start..at].iter().collect::<String>().parse().ok();
        let before = chars[..start].iter().collect::<String>().to_lowercase();
        let before = before.strip_suffix([' ', '.']).unwrap_or(&before);
        // `strip_suffix` rather than `ends_with`, because the head it hands
        // back is exactly what the word-boundary test needs: the character
        // before the marker, or nothing at all.
        if markers.iter().any(|marker| {
            before.strip_suffix(marker.before).is_some_and(|head| {
                head.chars()
                    .next_back()
                    .is_none_or(|c| !c.is_ascii_alphanumeric())
            }) && marker.after.is_none_or(|counter| {
                // Compared directly against `chars[at..]` rather than
                // collected into a `String` first — that string would be
                // the rest of the name, built on every digit run this scan
                // finds regardless of whether any marker even has an
                // `after` to check, which is most of them: ten of the
                // eleven entries across both marker sets are `None` and
                // never look at it.
                let mut tail = chars[at..].iter().copied();
                counter.chars().all(|c| tail.next() == Some(c))
            })
        }) {
            return number;
        }
    }
    None
}

/// The volume number in a name, **only where a marker says so**.
///
/// [`volume_number`] directly above falls back to the first bare digit run
/// when no marker appears, which is right for `inventory` — it is handed a
/// directory that is a volume by construction. It is wrong for the scanner's
/// `builtin-volume` rule, which is applied to every filename in a library:
/// with the fallback, `Kaguya-sama - c001.cbz` comes back as volume 1, and
/// every chapter file in the corpus acquires a volume number it does not
/// have.
///
/// So this is the same question `chapter_number` answers for chapters, and
/// the same answer. The two functions are one scan over two marker sets; see
/// [`marked_number`].
///
/// A layout whose volumes carry no marker at all is served by a `regex` rule,
/// which is what the rule layer's escape hatch is for — not by loosening this.
pub fn volume_number_marked(name: &str) -> Option<f64> {
    marked_number(name, &VOLUME_MARKERS)
}

/// The chapter number in a name.
///
/// Takes the first digit run — with an optional decimal part — immediately
/// preceded, optionally across one space or a `.`, by `c`, `ch`, `chp`,
/// `chapter` or `第`. **There is no fallback**, which is the one place this
/// deliberately differs from [`volume_number`] directly above.
///
/// `volume_number` falls back to the first digit run when no marker appears,
/// and that is right for its caller: `inventory` hands it a directory that is
/// a volume by construction, so a bare `069` is a volume number. A chapter
/// parser with the same fallback would answer for every volume file in a
/// library — `Series - v01 (2023).cbz` would carry chapter 1 — and the
/// scanner's `builtin-chapter` rule (§4) would then set `chapter` on
/// everything it saw. A chapter number requires its marker.
///
/// `f64` rather than `u32` because `member.chapter_declared` is `REAL` and
/// half-chapters exist: `c001.5` is one file in the corpus's chapters root.
/// Leading zeros are stripped by the parse, so `c001` and `c1` agree.
///
/// `第` needs its own counter to say which number it is: `第7話` and `第12章`
/// are chapters, `第7巻` is a volume answered by [`volume_number_marked`]
/// instead, and a bare `第7` with no counter answers neither — see
/// [`Marker`]. Before that counter was checked, `第` matched anything after
/// it and every Japanese-named file under the seeded defaults took a volume
/// number and a chapter number with the same value.
///
/// **The dotted forms are listed as markers rather than trimmed**, which is
/// how [`volume_number`] above already handles `vol.`: its `MARKERS` carries
/// both `"vol."` and `"vol"`. `Ch. 7` has *two* separators between the marker
/// and the digit, and the single `strip_suffix` below removes one — so `ch.`
/// has to be a marker in its own right.
///
/// **A marker must start at a word boundary, and that guard is doing real
/// work.** `c` and `ch` are one and two characters, so without it they match
/// the *tail of ordinary words*: `Scan by ABC 2019` reads as chapter 2019
/// because `abc` ends in `c`, and `which 7` reads as chapter 7 because
/// `which` ends in `ch`. Both were measured on the built function rather than
/// reasoned about. This is the same hole the no-fallback rule above exists to
/// close, reached by a different route — a name with no chapter in it getting
/// a chapter number — so closing one without the other would have been half a
/// decision.
///
/// The boundary test is ASCII-only on purpose: `第7話` must keep working when
/// it follows a kanji, and `char::is_alphanumeric` would reject that, since
/// every kanji is alphanumeric to Unicode.
pub fn chapter_number(name: &str) -> Option<f64> {
    marked_number(name, &CHAPTER_MARKERS)
}

/// Compare two names in natural order.
#[must_use]
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (ta, tb) = (tokenize(a), tokenize(b));
    for pair in ta.iter().zip(tb.iter()) {
        let ordering = match pair {
            (Token::Number(x), Token::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
            (Token::Text(x), Token::Text(y)) => x.cmp(y),
            // Digits before letters, so `9.jpg` precedes `cover.jpg`.
            (Token::Number(_), Token::Text(_)) => Ordering::Less,
            (Token::Text(_), Token::Number(_)) => Ordering::Greater,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    ta.len().cmp(&tb.len())
}

#[cfg(test)]
mod tests;
