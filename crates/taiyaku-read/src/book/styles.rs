//! The italic and bold a book's stylesheets set, read without a CSS engine.
//!
//! Only what marks need is read: `font-style` and `font-weight`, set by a
//! rule whose selector is one class (`.it`) or one tag and one class
//! (`span.it`). Anything else, from descendant selectors to `@media` blocks,
//! is skipped, so a book is never refused over CSS and a rule the scanner
//! cannot place never marks text it was not meant for.

use std::{collections::HashMap, rc::Rc};

/// What a rule or attribute says of an element's italic and bold; `None` says
/// nothing, so the state it inherits stands.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(super) struct Style {
    pub italic: Option<bool>,
    pub bold: Option<bool>,
}

impl Style {
    /// `later` laid over `self`: what it says wins, what it leaves unsaid stays.
    fn then(self, later: Style) -> Style {
        Style {
            italic: later.italic.or(self.italic),
            bold: later.bold.or(self.bold),
        }
    }
}

/// The sheets a document answers to, in the order it reads them. A clone
/// shares the sheets rather than copying their rules, so a sheet every
/// chapter links is held once.
#[derive(Clone, Default, Debug)]
pub(super) struct Styles {
    sheets: Vec<Rc<Sheet>>,
}

/// One sheet's rules, kept by the class each names, so an element is matched
/// against the rules naming one of its classes rather than against them all.
#[derive(Default, Debug)]
struct Sheet {
    by_class: HashMap<String, Vec<Rule>>,
}

/// One selector's declarations. `order` is its place in its sheet, since
/// among rules of one weight the later one wins however the element's class
/// attribute lists the classes.
#[derive(Debug)]
struct Rule {
    tag: Option<String>,
    order: usize,
    style: Style,
}

impl Styles {
    /// The rules of `css`. Never fails: what cannot be read is skipped.
    pub fn parse(css: &str) -> Self {
        let css = without_comments(css);
        let mut sheet = Sheet::default();
        let mut order = 0;
        let mut rest = css.as_str();
        while let Some(stop) = rest.find(['{', ';']) {
            let prelude = rest[..stop].trim();
            let opens_block = rest.as_bytes()[stop] == b'{';
            rest = &rest[stop + 1..];
            if !opens_block {
                continue;
            }
            let (body, after) = block(rest);
            rest = after;
            if prelude.starts_with('@') {
                continue;
            }
            let style = declarations(body);
            if style == Style::default() {
                continue;
            }
            for selector in prelude.split(',') {
                if let Some((tag, class)) = simple_selector(selector.trim()) {
                    let rule = Rule { tag, order, style };
                    sheet.by_class.entry(class).or_default().push(rule);
                    order += 1;
                }
            }
        }
        Styles {
            sheets: vec![Rc::new(sheet)],
        }
    }

    /// `other`'s sheets laid over these, as sheets read later are.
    pub fn extend(&mut self, other: Styles) {
        self.sheets.extend(other.sheets);
    }

    /// The style of an element named `tag` with the space-separated `classes`
    /// and the given `style` attribute. A rule naming the tag outranks one
    /// naming the class alone, among equals the later in the source wins, and
    /// the attribute outranks all, as in CSS.
    pub fn of(&self, tag: &str, classes: &str, style_attr: Option<&str>) -> Style {
        // Each matching rule weighed by whether it names the tag, then by its
        // place in the source: its sheet's, then its own in the sheet.
        let mut matching: Vec<((bool, usize, usize), Style)> = Vec::new();
        for (at, sheet) in self.sheets.iter().enumerate() {
            for class in classes.split_whitespace() {
                for rule in sheet.by_class.get(class).into_iter().flatten() {
                    // Case-blind, as a rule's tag is lowercased when read and
                    // the element's may not be.
                    if rule
                        .tag
                        .as_ref()
                        .is_none_or(|t| t.eq_ignore_ascii_case(tag))
                    {
                        matching.push(((rule.tag.is_some(), at, rule.order), rule.style));
                    }
                }
            }
        }
        matching.sort_by_key(|&(weight, _)| weight);
        let style = matching
            .iter()
            .fold(Style::default(), |style, &(_, rule)| style.then(rule));
        match style_attr {
            Some(attr) => style.then(declarations(attr)),
            None => style,
        }
    }
}

fn without_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("/*") {
        out.push_str(&rest[..at]);
        rest = match rest[at + 2..].find("*/") {
            Some(end) => &rest[at + 2 + end + 2..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

/// The text up to the `}` closing a block already opened, and what follows it.
/// Nested braces (an `@media` block's rules) are counted so the block ends at
/// its own close.
fn block(text: &str) -> (&str, &str) {
    let mut depth = 1;
    for (at, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (&text[..at], &text[at + 1..]);
                }
            }
            _ => {}
        }
    }
    (text, "")
}

/// `selector` as the (tag, class) it names, when it names exactly a class or
/// a tag with a class and nothing else.
fn simple_selector(selector: &str) -> Option<(Option<String>, String)> {
    let (tag, class) = selector.split_once('.')?;
    let ident = |s: &str| {
        s.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    };
    if !ident(tag) || class.is_empty() || !ident(class) {
        return None;
    }
    let tag = (!tag.is_empty()).then(|| tag.to_ascii_lowercase());
    Some((tag, class.to_string()))
}

/// What the `font-style` and `font-weight` declarations in `text` say, the
/// last of each winning.
fn declarations(text: &str) -> Style {
    let mut style = Style::default();
    for declaration in text.split(';') {
        let Some((name, value)) = declaration.split_once(':') else {
            continue;
        };
        let value = value.to_ascii_lowercase();
        let value = value.replace("!important", "");
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "font-style" => match value {
                "italic" | "oblique" => style.italic = Some(true),
                "normal" => style.italic = Some(false),
                _ => {}
            },
            "font-weight" => match value {
                "bold" | "bolder" => style.bold = Some(true),
                "normal" | "lighter" => style.bold = Some(false),
                number => {
                    if let Ok(weight) = number.parse::<u32>() {
                        style.bold = Some(weight >= 600);
                    }
                }
            },
            _ => {}
        }
    }
    style
}

#[cfg(test)]
mod tests;
