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
