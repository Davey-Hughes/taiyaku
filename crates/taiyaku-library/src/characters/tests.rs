use serde_json::{Value, json};

use super::*;
use crate::Library;
use crate::account::Access;
use crate::test_support::{
    Keepers, Kept, keepers, store_ferry_house, store_keepers_series, store_lamp_alone,
    store_lamp_keepers, store_names_abroad, store_second_lamp, store_weir_ledger,
};
use crate::tests::TempDir;

/// A's digest. Each invented book has its own, never shared with another.
fn lamp_keepers() -> String {
    "a1".repeat(32)
}

fn second_lamp() -> String {
    "b2".repeat(32)
}

fn lamp_alone() -> String {
    "c3".repeat(32)
}

fn names_abroad() -> String {
    "d4".repeat(32)
}

fn ferry_house() -> String {
    "f6".repeat(32)
}

fn weir_ledger() -> String {
    "a7".repeat(32)
}

/// A digest no test stores.
fn unstored() -> String {
    "e5".repeat(32)
}

/// A library holding the invented books and their series.
fn shelf() -> Library {
    let library = Library::open_in_memory().expect("opens");
    {
        let mut conn = library.conn();
        store_lamp_keepers(&mut conn, &lamp_keepers());
        store_second_lamp(&mut conn, &second_lamp());
        store_lamp_alone(&mut conn, &lamp_alone());
        store_names_abroad(&mut conn, &names_abroad());
        store_ferry_house(&mut conn, &ferry_house());
        store_weir_ledger(&mut conn, &weir_ledger());
        store_keepers_series(&mut conn, &lamp_keepers(), &second_lamp());
    }
    library
}

fn asked<'a>(book: &'a str, document: &'a str, text: &'a str) -> Asked<'a> {
    Asked {
        book,
        document,
        text,
        before: "",
        earlier: false,
        series: None,
        order: None,
    }
}

/// The answer for `text`, tapped in `document` of `book`.
fn ask(library: &Library, book: &str, document: &str, text: &str) -> Answer {
    ask_after(library, book, document, "", text)
}

/// The answer for `text`, tapped in `document` of `book` where `before`
/// is the text that ends at it.
fn ask_after(library: &Library, book: &str, document: &str, before: &str, text: &str) -> Answer {
    let asked = Asked {
        before,
        ..asked(book, document, text)
    };
    answer(&library.conn(), &asked, &Access::Every, library.titles()).expect("answers")
}

/// Where the name the answer marks starts, and how long it is.
fn marked(answer: &Answer) -> (isize, Option<usize>) {
    (answer.start, answer.length)
}

/// The answer for `text`, tapped in `document` of A.
fn ask_a(library: &Library, document: &str, text: &str) -> Answer {
    ask(library, &lamp_keepers(), document, text)
}

/// The answer for `text`, tapped in `document` of E.
fn ask_e(library: &Library, document: &str, text: &str) -> Answer {
    ask(library, &ferry_house(), document, text)
}

fn shown(answer: &Answer) -> Vec<&str> {
    answer
        .characters
        .iter()
        .map(|card| card.name.text.as_str())
        .collect()
}

/// The chapter each card's character was last seen in, in the cards' order.
fn last_seen(answer: &Answer) -> Vec<u32> {
    answer
        .characters
        .iter()
        .map(|card| card.last.chapter)
        .collect()
}

/// The name each card was matched by and the part of it that matched, in the
/// cards' order.
fn matched(answer: &Answer) -> Vec<(&str, &str)> {
    answer
        .characters
        .iter()
        .map(|card| (card.matched.text.as_str(), card.matched.part))
        .collect()
}

fn card<'a>(answer: &'a Answer, name: &str) -> &'a Card {
    answer
        .characters
        .iter()
        .find(|card| card.name.text == name)
        .unwrap_or_else(|| panic!("a card for {name} in {answer:?}"))
}

fn related(card: &Card) -> Vec<&str> {
    card.relations
        .iter()
        .map(|relation| relation.with.text.as_str())
        .collect()
}

fn fact<'a>(card: &'a Card, with: &str, kind: &str) -> &'a CardFact {
    card.relations
        .iter()
        .filter(|relation| relation.with.text == with)
        .flat_map(|relation| &relation.facts)
        .find(|fact| fact.kind == kind)
        .unwrap_or_else(|| panic!("a {kind} fact with {with} on {card:?}"))
}

fn at(book: &str, chapter: u32, chapter_title: &str) -> ChapterAt {
    ChapterAt {
        book: book.to_owned(),
        chapter,
        chapter_title: chapter_title.to_owned(),
    }
}

fn name(text: &str, given: Option<&str>, surname: Option<&str>) -> CardName {
    CardName {
        text: text.to_owned(),
        given: given.map(str::to_owned),
        surname: surname.map(str::to_owned),
        title: None,
        title_sex: None,
    }
}

/// An object's keys, sorted.
fn keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap_or_else(|| panic!("an object: {value}"))
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

#[test]
fn a_card_holds_only_what_the_finished_chapters_say() {
    let library = shelf();
    let unmet = ask_a(&library, "OEBPS/c2.xhtml", "Mrs. Vell said nothing.");
    assert!(unmet.known);
    assert_eq!(shown(&unmet), Vec::<&str>::new());

    let in_c2 = ask_a(
        &library,
        "OEBPS/c2.xhtml",
        "Ned Reed knew his cousin at once.",
    );
    let ned = card(&in_c2, "Ned Reed");
    assert_eq!(related(ned), ["Tom Field"]);
    assert!(
        ned.relations
            .iter()
            .flat_map(|relation| &relation.facts)
            .all(|fact| fact.note != "cousins"),
        "{ned:?}"
    );

    let back = ask_a(&library, "OEBPS/c3.xhtml", "Mara Vell came back.");
    assert_eq!(shown(&back), ["Mara Vell"]);
    let in_c3 = ask_a(
        &library,
        "OEBPS/c3.xhtml",
        "Ned Reed saw that Tom Field had lied.",
    );
    let ned = card(&in_c3, "Ned Reed");
    assert_eq!(related(ned), ["Tom Field", "Mara Vell"]);
    let family = fact(ned, "Mara Vell", "family");
    assert_eq!(
        (family.note.as_str(), family.quote.as_str()),
        ("cousins", "Ned Reed knew his cousin at once.")
    );
}

#[test]
fn an_overturned_fact_is_marked_once_its_chapter_is_read() {
    let library = shelf();
    let in_c3 = ask_a(
        &library,
        "OEBPS/c3.xhtml",
        "Ned Reed saw that Tom Field had lied.",
    );
    let ned = card(&in_c3, "Ned Reed");
    assert_eq!(fact(ned, "Tom Field", "friend").overturned, None);
    assert_eq!(fact(ned, "Tom Field", "mentor").overturned, None);

    let in_c4 = ask_a(&library, "OEBPS/c4.xhtml", "Reeds put out the lamp.");
    let ned = card(&in_c4, "Ned Reed");
    assert_eq!(
        fact(ned, "Tom Field", "friend").overturned,
        Some(CardOverturn {
            note: "Tom only pretended".to_owned(),
            quote: "Ned Reed saw that Tom Field had lied.".to_owned(),
            at: at("The Lamp Keepers", 3, "Return"),
        })
    );
    assert_eq!(fact(ned, "Tom Field", "mentor").overturned, None);
}

#[test]
fn a_directed_fact_says_which_side_it_runs_from() {
    let library = shelf();
    let in_c2 = ask_a(
        &library,
        "OEBPS/c2.xhtml",
        "Ned Reed knew his cousin at once.",
    );
    let ned = card(&in_c2, "Ned Reed");
    assert_eq!(fact(ned, "Tom Field", "mentor").from, Some("with"));
    assert_eq!(fact(ned, "Tom Field", "friend").from, None);

    let in_c2 = ask_a(
        &library,
        "OEBPS/c2.xhtml",
        "Tom Field showed Ned Reed the wick.",
    );
    let tom = card(&in_c2, "Tom Field");
    assert_eq!(fact(tom, "Ned Reed", "mentor").from, Some("this"));
    assert_eq!(fact(tom, "Ned Reed", "friend").from, None);
}

#[test]
fn several_characters_come_most_recently_seen_first() {
    let library = shelf();
    let reeds = ask_a(&library, "OEBPS/c4.xhtml", "Reeds put out the lamp.");
    assert_eq!(shown(&reeds), ["Ned Reed", "Ada Reed"]);
    for (card, matched) in reeds.characters.iter().zip(["Ned Reed", "Ada Reed"]) {
        assert_eq!(
            card.matched,
            Matched {
                text: matched.to_owned(),
                part: "surname",
            }
        );
    }
    assert_eq!(reeds.length, Some(5));
}

#[test]
fn a_tap_on_a_characters_own_name_lists_that_character_first() {
    let library = shelf();
    let named = ask_e(&library, "OEBPS/c3.xhtml", "Mr. Dunn to the crew now.");
    assert_eq!(shown(&named), ["Mr. Dunn", "Ira Dunn"]);
    assert_eq!(
        last_seen(&named),
        [1, 2],
        "Ira Dunn is the more recently seen, so haiyaku lists him first"
    );
    assert_eq!(
        matched(&named),
        [("Mr. Dunn", "whole"), ("Mr. Dunn", "whole")],
        "Ira Dunn is named by another of his names, the whole of it"
    );
}

#[test]
fn cards_named_by_another_name_stay_most_recently_seen_first() {
    let library = shelf();
    let named = ask_e(&library, "OEBPS/c5.xhtml", "Mr. Dunn watched the river.");
    assert_eq!(shown(&named), ["Mr. Dunn", "Hal Dunn", "Ira Dunn"]);
    assert_eq!(
        last_seen(&named),
        [1, 3, 2],
        "the two named by another name were last seen in different chapters"
    );
}

#[test]
fn a_part_of_the_shown_name_comes_before_a_part_of_another_name() {
    let library = shelf();
    let family = ask_e(&library, "OEBPS/c5.xhtml", "Dunns kept the ferry.");
    assert_eq!(
        shown(&family),
        ["Hal Dunn", "Ira Dunn", "Mr. Dunn", "Nell Carrow"]
    );
    assert_eq!(
        last_seen(&family),
        [3, 2, 1, 4],
        "Nell Carrow is the most recently seen, so haiyaku lists her first"
    );
    assert_eq!(
        matched(&family),
        [
            ("Hal Dunn", "surname"),
            ("Ira Dunn", "surname"),
            ("Mr. Dunn", "surname"),
            ("Mrs. Dunn", "surname"),
        ],
        "only Nell Carrow has the surname from another of her names"
    );
}

#[test]
fn a_tap_on_no_ones_shown_name_keeps_haiyakus_order() {
    let library = shelf();
    let ferrymen = ask_e(&library, "OEBPS/c5.xhtml", "The ferryman slept.");
    assert_eq!(shown(&ferrymen), ["Hal Dunn", "Ira Dunn"]);
    assert_eq!(last_seen(&ferrymen), [3, 2]);
    assert_eq!(
        matched(&ferrymen),
        [("the ferryman", "whole"), ("the ferryman", "whole")]
    );
    let other_names = |shown: &str| -> Vec<&str> {
        let aliases = &card(&ferrymen, shown).aliases;
        aliases.iter().map(|alias| alias.text.as_str()).collect()
    };
    assert_eq!(
        (other_names("Hal Dunn"), other_names("Ira Dunn")),
        (
            vec!["Mr. Dunn", "the ferryman"],
            vec!["the ferryman", "Mr. Dunn"]
        ),
        "the name tapped is the later-seen card's second other name and the earlier-seen card's first"
    );
}

#[test]
fn a_given_name_alone_matches_as_given() {
    let library = shelf();
    let ned = ask_a(&library, "OEBPS/c4.xhtml", "Ned put out the lamp.");
    assert_eq!(shown(&ned), ["Ned Reed"]);
    assert_eq!(
        ned.characters[0].matched,
        Matched {
            text: "Ned Reed".to_owned(),
            part: "given",
        }
    );
    assert_eq!(ned.length, Some(3));
}

#[test]
fn a_title_states_either_sex() {
    let titled = |title: &str, sex| Name {
        text: format!("{title} Vell"),
        given: None,
        surname: Some("Vell".to_owned()),
        title: Some(title.to_owned()),
        title_sex: Some(sex),
    };
    assert_eq!(card_name(&titled("Mr.", Sex::Male)).title_sex, Some("male"));
    assert_eq!(
        card_name(&titled("Mrs.", Sex::Female)).title_sex,
        Some("female")
    );
}

#[test]
fn a_card_carries_every_name_with_its_labels() {
    let library = shelf();
    let back = ask_a(&library, "OEBPS/c3.xhtml", "Mara Vell came back.");
    let mara = card(&back, "Mara Vell");
    assert_eq!(mara.name, name("Mara Vell", Some("Mara"), Some("Vell")));
    assert_eq!(
        mara.aliases,
        [CardName {
            title: Some("Mrs.".to_owned()),
            title_sex: Some("female"),
            ..name("Mrs. Vell", None, Some("Vell"))
        }]
    );
}

#[test]
fn first_and_last_name_haiyakus_book_and_chapter_titles() {
    let library = shelf();
    let in_c4 = ask_a(&library, "OEBPS/c4.xhtml", "Reeds put out the lamp.");
    let ned = card(&in_c4, "Ned Reed");
    assert_eq!(ned.first, at("The Lamp Keepers", 1, "The Lamp"));
    assert_eq!(ned.last, at("The Lamp Keepers", 3, "Return"));
}

#[test]
fn a_fact_said_again_runs_from_the_chapter_first_saying_it_to_the_last() {
    let library = shelf();
    let in_c4 = ask_a(&library, "OEBPS/c4.xhtml", "Ned Reed");
    let family = fact(card(&in_c4, "Ned Reed"), "Mara Vell", "family");
    assert_eq!(
        (&family.first, &family.last),
        (
            &at("The Lamp Keepers", 2, "Dusk"),
            &at("The Lamp Keepers", 3, "Return")
        )
    );
}

#[test]
fn length_is_counted_in_utf16_units() {
    let library = shelf();
    let book = names_abroad();
    let eida = ask(&library, &book, "OEBPS/c2.xhtml", "에이다는 웃었다.");
    assert_eq!((shown(&eida), eida.length), (vec!["에이다"], Some(3)));
    let yoshida = ask(&library, &book, "OEBPS/c2.xhtml", "𠮷田さんは来た。");
    assert_eq!((shown(&yoshida), yoshida.length), (vec!["𠮷田"], Some(3)));
}

#[test]
fn no_match_sends_no_length() {
    let library = shelf();
    let the = ask_a(&library, "OEBPS/c4.xhtml", "The Reeds put out the lamp.");
    assert!(the.known);
    assert_eq!(shown(&the), Vec::<&str>::new());
    let sent = serde_json::to_value(&the).expect("serializes");
    assert_eq!(keys(&sent), ["characters", "known", "narrowed", "scope"]);
}

#[test]
fn a_tap_on_the_surname_after_a_title_names_the_one_so_titled() {
    let library = shelf();
    let (book, c2) = (ferry_house(), "OEBPS/c2.xhtml");
    let bare = ask(&library, &book, c2, "Dunn, and Ira Dunn came.");
    assert_eq!(shown(&bare), ["Mr. Dunn", "Ira Dunn", "Hal Dunn"]);
    assert_eq!(marked(&bare), (0, Some(4)));

    let titled = ask_after(
        &library,
        &book,
        c2,
        "They called for Mr. ",
        "Dunn, and Ira Dunn came.",
    );
    assert_eq!(shown(&titled), ["Mr. Dunn"]);
    assert_eq!(matched(&titled), [("Mr. Dunn", "whole")]);
    assert_eq!(marked(&titled), (-4, Some(8)));
    let sent = serde_json::to_value(&titled).expect("serializes");
    assert_eq!((&sent["start"], &sent["length"]), (&json!(-4), &json!(8)));
}

#[test]
fn a_tap_on_the_surname_after_the_given_name_names_the_one_so_named() {
    let library = shelf();
    let (book, c4) = (lamp_keepers(), "OEBPS/c4.xhtml");
    let bare = ask(&library, &book, c4, "Reed saw that Tom Field had lied.");
    assert_eq!(shown(&bare), ["Ned Reed", "Ada Reed"]);

    let named = ask_after(
        &library,
        &book,
        c4,
        "Ned ",
        "Reed saw that Tom Field had lied.",
    );
    assert_eq!(shown(&named), ["Ned Reed"]);
    assert_eq!(matched(&named), [("Ned Reed", "whole")]);
    assert_eq!(marked(&named), (-4, Some(8)));
}

#[test]
fn a_name_before_the_tap_that_does_not_reach_it_is_passed_over() {
    let library = shelf();
    let (book, c2) = (lamp_keepers(), "OEBPS/c2.xhtml");
    let tom = ask_after(
        &library,
        &book,
        c2,
        "Ned Reed and ",
        "Tom Field kept it burning.",
    );
    assert_eq!(shown(&tom), ["Tom Field"]);
    assert_eq!(marked(&tom), (0, Some(9)));

    let ned = ask_after(&library, &book, c2, "Ada Reed", ", Ned Reed said.");
    assert_eq!(
        shown(&ned),
        ["Ned Reed"],
        "a name that ends where the tapped text starts has not reached it"
    );
    assert_eq!(marked(&ned), (0, Some(10)));
}

#[test]
fn with_nothing_before_the_answer_is_what_it_was() {
    let library = shelf();
    let ada = ask_a(&library, "OEBPS/c2.xhtml", "Ada Reed lit the lamp.");
    let lamp = r#"{"book":"The Lamp Keepers","chapter":1,"chapterTitle":"The Lamp"}"#;
    assert_eq!(
        serde_json::to_string(&ada).expect("serializes"),
        [
            r#"{"known":true,"scope":null,"narrowed":null,"length":8,"characters":[{"#,
            r#""name":{"text":"Ada Reed","given":"Ada","surname":"Reed","title":null,"titleSex":null},"#,
            r#""aliases":[],"matched":{"text":"Ada Reed","part":"whole"},"#,
            &format!(r#""first":{lamp},"last":{lamp},"relations":[]}}]}}"#),
        ]
        .concat()
    );
}

#[test]
fn a_stuttered_word_names_whom_its_last_part_names() {
    let library = shelf();
    let (book, c4) = (lamp_keepers(), "OEBPS/c4.xhtml");
    let ned = ask(&library, &book, c4, "Ne-Ne-Ned put out the lamp.");
    assert_eq!(shown(&ned), ["Ned Reed"]);
    assert_eq!(matched(&ned), [("Ned Reed", "given")]);
    assert_eq!(marked(&ned), (6, Some(3)));

    for hyphen in ['\u{2010}', '\u{2011}'] {
        let ned = ask(
            &library,
            &book,
            c4,
            &format!("Ne{hyphen}Ned put out the lamp."),
        );
        assert_eq!(shown(&ned), ["Ned Reed"], "after {hyphen:?}");
        assert_eq!(
            marked(&ned),
            (3, Some(3)),
            "{hyphen:?} is one UTF-16 unit and three bytes"
        );
    }

    let yoshida = ask(
        &library,
        &names_abroad(),
        "OEBPS/c2.xhtml",
        "𠮷-𠮷田さんは来た。",
    );
    assert_eq!(shown(&yoshida), ["𠮷田"]);
    assert_eq!(
        marked(&yoshida),
        (3, Some(3)),
        "𠮷 is two UTF-16 units, one character and four bytes"
    );
}

#[test]
fn a_stutter_is_looked_for_in_the_tapped_word_alone_and_after_its_start() {
    let library = shelf();
    let (book, c4) = (lamp_keepers(), "OEBPS/c4.xhtml");
    let whole = ask(&library, &book, c4, "Ned-Reed put out the lamp.");
    assert_eq!(
        (shown(&whole), marked(&whole)),
        (vec!["Ned Reed"], (0, Some(8))),
        "the word's start names one Reed, and past its hyphen it would name both"
    );

    let earliest = ask(&library, &book, c4, "Er-Ned-Reed put out the lamp.");
    assert_eq!(
        (shown(&earliest), marked(&earliest)),
        (vec!["Ned Reed"], (3, Some(8))),
        "past the first hyphen names one Reed, and past the second it would name both"
    );

    let next_word = ask(&library, &book, c4, "So Ne-Ned put out the lamp.");
    assert_eq!(shown(&next_word), Vec::<&str>::new());
    let ends_the_word = ask(&library, &book, c4, "Ne- Ned put out the lamp.");
    assert_eq!(shown(&ends_the_word), Vec::<&str>::new());
}

#[test]
fn a_dash_between_two_names_is_no_place_to_read_from() {
    let library = shelf();
    let ned = ask_after(
        &library,
        &lamp_keepers(),
        "OEBPS/c4.xhtml",
        "Ada Reed \u{2014} ",
        "Ned Reed put out the lamp.",
    );
    assert_eq!(shown(&ned), ["Ned Reed"]);
    assert_eq!(marked(&ned), (0, Some(8)));
}

#[test]
fn a_name_inside_a_word_before_the_tap_is_no_place_to_read_from() {
    let library = shelf();
    let after = ask_after(
        &library,
        &lamp_keepers(),
        "OEBPS/c4.xhtml",
        "McNed ",
        "Reed saw that Tom Field had lied.",
    );
    assert_eq!(
        (shown(&after), marked(&after)),
        (vec!["Ned Reed", "Ada Reed"], (0, Some(4))),
        "the tap answers as with nothing before it: a word is read from its first letter alone"
    );
}

#[test]
fn a_mark_that_opens_a_name_before_the_tap_is_outside_the_name() {
    let library = shelf();
    let (book, c2) = (ferry_house(), "OEBPS/c2.xhtml");
    let quoted = ask_after(
        &library,
        &book,
        c2,
        "\u{201c}Mr. ",
        "Dunn kept the ferry house.",
    );
    assert_eq!(shown(&quoted), ["Mr. Dunn"]);
    assert_eq!(marked(&quoted), (-4, Some(8)));

    let bracketed = ask_after(
        &library,
        &book,
        c2,
        "They called (Mr. ",
        "Dunn kept the ferry house.",
    );
    assert_eq!(
        (shown(&bracketed), marked(&bracketed)),
        (vec!["Mr. Dunn"], (-4, Some(8))),
        "a mark opens a later word of the text before the tap"
    );
}

#[test]
fn a_stutter_is_read_from_the_letter_past_its_hyphens() {
    let library = shelf();
    let (book, c4) = (lamp_keepers(), "OEBPS/c4.xhtml");
    let doubled = ask(&library, &book, c4, "Ne--Ned put out the lamp.");
    assert_eq!(shown(&doubled), ["Ned Reed"]);
    assert_eq!(marked(&doubled), (4, Some(3)));

    let dashed = ask(&library, &book, c4, "Ne-\u{2014}Ned put out the lamp.");
    assert_eq!(
        (shown(&dashed), marked(&dashed)),
        (vec!["Ned Reed"], (4, Some(3))),
        "what follows the hyphen is no letter"
    );
}

#[test]
fn a_hangul_name_tapped_on_its_second_word_is_measured_in_utf16_units() {
    let library = shelf();
    let (book, c2) = (names_abroad(), "OEBPS/c2.xhtml");
    let bare = ask(&library, &book, c2, "마루는 늦었다.");
    assert_eq!(matched(&bare), [("다온 마루", "surname")]);
    assert_eq!(marked(&bare), (0, Some(2)));

    let daon = ask_after(&library, &book, c2, "다온 ", "마루는 늦었다.");
    assert_eq!(matched(&daon), [("다온 마루", "whole")]);
    assert_eq!(
        marked(&daon),
        (-3, Some(5)),
        "the particle is outside the name, and a syllable is one unit and three bytes"
    );

    let yoshida = ask_after(&library, &book, c2, "𠮷", "田さんは来た。");
    assert_eq!(shown(&yoshida), ["𠮷田"]);
    assert_eq!(
        marked(&yoshida),
        (-2, Some(3)),
        "𠮷 is two UTF-16 units, one character and four bytes"
    );
}

#[test]
fn of_two_names_that_reach_the_tap_the_one_that_starts_earlier_answers() {
    let library = shelf();
    let (book, c2) = (weir_ledger(), "OEBPS/c2.xhtml");
    let later = ask_after(&library, &book, c2, "Edda ", "Sorrel kept the key.");
    assert_eq!(shown(&later), ["Edda Sorrel"]);
    assert_eq!(marked(&later), (-5, Some(11)));

    let earlier = ask_after(&library, &book, c2, "Dame Edda ", "Sorrel kept the key.");
    assert_eq!(shown(&earlier), ["Dame Edda Sorrel"]);
    assert_eq!(marked(&earlier), (-10, Some(16)));
}

/// KNOWN WRONG, held as it is: a name is read on from a word of `before`
/// across a sentence's end or a closing quotation mark, since haiyaku parts
/// words at every character that is not a letter or a digit and so reads
/// "Ira. Dunn" as the name Ira Dunn. With nothing before it, the same tap
/// lists everyone of that surname. A match
/// that stops at punctuation would be haiyaku's to add (`docs/notes.md`,
/// "Seen at the prototype and by the tests").
#[test]
fn known_wrong_a_name_is_read_across_a_sentence_end_or_a_closing_quote() {
    let library = shelf();
    let (book, c2) = (ferry_house(), "OEBPS/c2.xhtml");
    let past_a_stop = ask_after(&library, &book, c2, "He rowed to Ira. ", "Dunn was gone.");
    assert_eq!(shown(&past_a_stop), ["Ira Dunn"]);
    assert_eq!(marked(&past_a_stop), (-5, Some(9)));

    let past_a_quote = ask_after(&library, &book, c2, "\u{201c}Ira,\u{201d} ", "Dunn said.");
    assert_eq!(shown(&past_a_quote), ["Ira Dunn"]);
    assert_eq!(marked(&past_a_quote), (-6, Some(10)));
}

#[test]
fn an_unknown_book_is_not_known() {
    let library = shelf();
    let unknown = ask(&library, &unstored(), "OEBPS/c1.xhtml", "Ned Reed");
    assert_eq!(unknown, Answer::unknown());
    assert_eq!(
        serde_json::to_value(&unknown).expect("serializes"),
        json!({"known": false, "scope": null, "narrowed": null, "characters": []})
    );
}

#[test]
fn a_document_haiyaku_skipped_is_known_with_no_characters() {
    let library = shelf();
    assert_eq!(
        ask_a(&library, "OEBPS/notes.xhtml", "Ned Reed"),
        Answer {
            known: true,
            scope: None,
            narrowed: None,
            length: None,
            start: 0,
            characters: Vec::new(),
        }
    );
}

#[test]
fn the_answer_sends_exactly_these_keys() {
    let library = shelf();
    let in_c4 = ask_a(&library, "OEBPS/c4.xhtml", "Ned Reed");
    assert_eq!(shown(&in_c4), ["Ned Reed"]);
    let sent = serde_json::to_value(&in_c4).expect("serializes");
    let name_keys = ["given", "surname", "text", "title", "titleSex"];
    let chapter_keys = ["book", "chapter", "chapterTitle"];
    assert_eq!(
        keys(&sent),
        ["characters", "known", "length", "narrowed", "scope"]
    );
    let ned = &sent["characters"][0];
    assert_eq!(
        keys(ned),
        ["aliases", "first", "last", "matched", "name", "relations"]
    );
    assert_eq!(keys(&ned["name"]), name_keys);
    assert_eq!(keys(&ned["matched"]), ["part", "text"]);
    assert_eq!(keys(&ned["first"]), chapter_keys);
    assert_eq!(keys(&ned["last"]), chapter_keys);
    let relations = ned["relations"].as_array().expect("relations");
    assert_eq!(relations.len(), 2);
    let mut overturned = 0;
    for relation in relations {
        assert_eq!(keys(relation), ["facts", "with"]);
        assert_eq!(keys(&relation["with"]), name_keys);
        for fact in relation["facts"].as_array().expect("facts") {
            assert_eq!(
                keys(fact),
                [
                    "first",
                    "from",
                    "kind",
                    "last",
                    "note",
                    "overturned",
                    "quote"
                ]
            );
            assert_eq!(keys(&fact["first"]), chapter_keys);
            assert_eq!(keys(&fact["last"]), chapter_keys);
            if !fact["overturned"].is_null() {
                assert_eq!(keys(&fact["overturned"]), ["at", "note", "quote"]);
                assert_eq!(keys(&fact["overturned"]["at"]), chapter_keys);
                overturned += 1;
            }
        }
    }
    assert_eq!(overturned, 1);

    let mara = ask_a(&library, "OEBPS/c4.xhtml", "Mrs. Vell");
    let sent = serde_json::to_value(&mara).expect("serializes");
    assert_eq!(keys(&sent["characters"][0]["aliases"][0]), name_keys);

    let scope = ScopeUsed {
        series: "keepers".to_owned(),
        order: "printed".to_owned(),
    };
    assert_eq!(
        keys(&serde_json::to_value(&scope).expect("serializes")),
        ["order", "series"]
    );
}

#[test]
fn titles_are_read_once_per_book() {
    let library = shelf();
    ask_a(
        &library,
        "OEBPS/c3.xhtml",
        "Ned Reed saw that Tom Field had lied.",
    );
    ask_a(&library, "OEBPS/c4.xhtml", "Reeds put out the lamp.");
    ask(
        &library,
        &names_abroad(),
        "OEBPS/c2.xhtml",
        "에이다는 웃었다.",
    );
    assert_eq!(library.titles().reads(), 2);
}

#[test]
fn a_chapter_the_log_lacks_has_an_empty_title() {
    let library = shelf();
    let titles = Titles::default()
        .of(&library.conn(), &lamp_keepers())
        .expect("reads A's titles");
    assert_eq!(titles.at(5), at("The Lamp Keepers", 5, ""));
}

#[test]
fn the_titles_of_a_book_haiyaku_lacks_are_an_error() {
    let library = shelf();
    let titles = Titles::default();
    assert!(titles.of(&library.conn(), &unstored()).is_err());
}

#[test]
fn holds_says_whether_haiyaku_holds_a_book() {
    let library = shelf();
    let conn = library.conn();
    assert_eq!(holds(&conn, &lamp_keepers()), Ok(true));
    assert_eq!(holds(&conn, &lamp_alone()), Ok(true));
    assert_eq!(holds(&conn, &unstored()), Ok(false));
}

#[test]
fn a_series_read_that_fails_is_an_error() {
    let library = shelf();
    let conn = library.conn();
    conn.execute_batch("DROP TABLE haiyaku_order_book")
        .expect("drops the orders' books");
    assert!(holds(&conn, &lamp_keepers()).is_err());
    let book = lamp_keepers();
    let asked = asked(&book, "OEBPS/c2.xhtml", "Ned Reed");
    assert!(answer(&conn, &asked, &Access::Every, &Titles::default()).is_err());
}

#[test]
fn a_fold_that_fails_is_an_error() {
    let library = shelf();
    let conn = library.conn();
    conn.execute_batch("DROP TABLE haiyaku_appearance")
        .expect("drops the appearances");
    assert_eq!(holds(&conn, &lamp_keepers()), Ok(true));
    let book = lamp_keepers();
    let asked = asked(&book, "OEBPS/c2.xhtml", "Ned Reed");
    assert!(answer(&conn, &asked, &Access::Every, &Titles::default()).is_err());
}

#[test]
fn choose_scope_takes_the_named_series_and_order_and_falls_back_to_the_first() {
    let holding = |series: &str, orders: [&str; 2]| Holding {
        series: series.to_owned(),
        orders: orders.map(str::to_owned).to_vec(),
    };
    let held = [
        holding("anthology", ["told", "printed"]),
        holding("keepers", ["printed", "author"]),
    ];
    assert_eq!(
        choose_scope(&held, false, Some("keepers"), Some("author")),
        Scope::Book
    );
    assert_eq!(choose_scope(&[], true, None, None), Scope::Book);
    let series = |series: &str, order: &str| Scope::Series {
        series: series.to_owned(),
        order: Some(order.to_owned()),
    };
    for (named, order, chosen) in [
        (None, None, series("anthology", "told")),
        (Some("keepers"), None, series("keepers", "printed")),
        (Some("keepers"), Some("author"), series("keepers", "author")),
        (Some("keepers"), Some("told"), series("keepers", "printed")),
        (
            Some("gone"),
            Some("printed"),
            series("anthology", "printed"),
        ),
        (Some("gone"), Some("gone"), series("anthology", "told")),
    ] {
        assert_eq!(
            choose_scope(&held, true, named, order),
            chosen,
            "series {named:?}, order {order:?}"
        );
    }
}

/// The keepers library in memory, with the books `shelve` names shelved. The
/// directory is returned so its files last as long as the test.
fn keepers_library(shelve: &[&str]) -> (Library, Keepers, TempDir) {
    let dir = crate::tests::tempdir("characters-keepers");
    let library = Library::open_in_memory().expect("opens");
    let keepers = keepers(&library, dir.path(), shelve);
    (library, keepers, dir)
}

/// A tap on "Ned Reed wrote to Wren Hollis." in B's second chapter, earlier
/// books asked for.
fn earlier_in_b(keepers: &Keepers) -> Asked<'_> {
    Asked {
        book: &keepers.b.sha256,
        document: "OEBPS/c2.xhtml",
        text: "Ned Reed wrote to Wren Hollis.",
        before: "",
        earlier: true,
        series: None,
        order: None,
    }
}

fn answered(library: &Library, asked: &Asked<'_>, access: &Access) -> Answer {
    answer(&library.conn(), asked, access, library.titles()).expect("answers")
}

/// What the answer sends as its `scope` and its `narrowed`.
fn sent_scope(answer: &Answer) -> (Value, Value) {
    let sent = serde_json::to_value(answer).expect("serializes");
    (sent["scope"].clone(), sent["narrowed"].clone())
}

fn keepers_in(order: &str) -> Value {
    json!({"series": "keepers", "order": order})
}

#[test]
fn earlier_off_folds_the_book_alone() {
    let (library, keepers, _dir) = keepers_library(&["A", "B", "C"]);
    let alone = Asked {
        earlier: false,
        ..earlier_in_b(&keepers)
    };
    let alone = answered(&library, &alone, &Access::Every);
    assert_eq!(shown(&alone), Vec::<&str>::new());
    assert_eq!(sent_scope(&alone), (Value::Null, Value::Null));
}

#[test]
fn earlier_on_folds_the_earlier_book_in_the_first_ranked_order() {
    let (library, keepers, _dir) = keepers_library(&["A", "B", "C"]);
    let earlier = answered(&library, &earlier_in_b(&keepers), &Access::Every);
    assert_eq!(shown(&earlier), ["Ned Reed"]);
    assert_eq!(card(&earlier, "Ned Reed").first.book, "The Lamp Keepers");
    assert_eq!(sent_scope(&earlier), (keepers_in("printed"), Value::Null));
}

#[test]
fn a_named_order_the_series_lacks_falls_back_to_the_first_ranked() {
    let (library, keepers, _dir) = keepers_library(&["A", "B", "C"]);
    let gone = Asked {
        order: Some("gone"),
        ..earlier_in_b(&keepers)
    };
    let gone = answered(&library, &gone, &Access::Every);
    assert_eq!(shown(&gone), ["Ned Reed"]);
    assert_eq!(sent_scope(&gone), (keepers_in("printed"), Value::Null));
}

#[test]
fn an_order_that_reads_the_book_first_folds_nothing_before_it() {
    let (library, keepers, _dir) = keepers_library(&["A", "B", "C"]);
    let author = Asked {
        order: Some("author"),
        ..earlier_in_b(&keepers)
    };
    let author = answered(&library, &author, &Access::Every);
    assert_eq!(shown(&author), Vec::<&str>::new());
    assert_eq!(sent_scope(&author), (keepers_in("author"), Value::Null));
}

#[test]
fn a_book_in_no_series_folds_alone() {
    let (library, keepers, _dir) = keepers_library(&["A", "B", "C"]);
    let iris = Asked {
        book: &keepers.c.sha256,
        text: "Iris Moor left.",
        ..earlier_in_b(&keepers)
    };
    let iris = answered(&library, &iris, &Access::Every);
    assert_eq!(shown(&iris), ["Iris Moor"]);
    assert_eq!(sent_scope(&iris), (Value::Null, Value::Null));
}

#[test]
fn a_series_fold_through_a_book_the_account_cannot_see_narrows_to_the_book() {
    let (library, keepers, _dir) = keepers_library(&["A", "B", "C"]);
    let root = |kept: &Kept| kept.root.expect("shelved");
    let asked = earlier_in_b(&keepers);

    let b_alone = answered(&library, &asked, &Access::roots(vec![root(&keepers.b)]));
    assert_eq!(shown(&b_alone), Vec::<&str>::new());
    assert_eq!(sent_scope(&b_alone), (Value::Null, json!("not-visible")));

    let both = Access::roots(vec![root(&keepers.a), root(&keepers.b)]);
    let both = answered(&library, &asked, &both);
    assert_eq!(shown(&both), ["Ned Reed"]);
    assert_eq!(sent_scope(&both), (keepers_in("printed"), Value::Null));
}

#[test]
fn a_series_fold_through_a_book_the_library_does_not_hold_narrows_for_every_account() {
    let (library, keepers, _dir) = keepers_library(&["B", "C"]);
    let narrowed = answered(&library, &earlier_in_b(&keepers), &Access::Every);
    assert_eq!(shown(&narrowed), Vec::<&str>::new());
    assert_eq!(sent_scope(&narrowed), (Value::Null, json!("not-visible")));
}

#[test]
fn books_seen_is_the_grant() {
    let (library, keepers, _dir) = keepers_library(&["A", "B", "C"]);
    let conn = library.conn();
    let books = [
        keepers.a.sha256.clone(),
        keepers.b.sha256.clone(),
        names_abroad(),
    ];
    let seen = |access: &Access| books_seen(&conn, access, &books).expect("reads the grant");
    let b_root = keepers.b.root.expect("B is shelved");
    assert_eq!(
        seen(&Access::roots(vec![b_root])),
        HashSet::from([keepers.b.sha256.clone()])
    );
    assert_eq!(
        seen(&Access::Every),
        HashSet::from([keepers.a.sha256.clone(), keepers.b.sha256.clone()])
    );
    assert_eq!(seen(&Access::roots(vec![])), HashSet::new());
}

/// A grant that cannot be read is an error, never a series folded unchecked.
#[test]
fn a_grant_read_that_fails_is_an_error() {
    let (library, keepers, _dir) = keepers_library(&["A", "B", "C"]);
    let conn = library.conn();
    conn.execute_batch("DROP TABLE member_root")
        .expect("drops the members' directories");
    let b_root = Access::roots(vec![keepers.b.root.expect("B is shelved")]);
    assert!(books_seen(&conn, &b_root, std::slice::from_ref(&keepers.b.sha256)).is_err());
    let asked = earlier_in_b(&keepers);
    assert!(answer(&conn, &asked, &b_root, &Titles::default()).is_err());
}
