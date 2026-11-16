use super::*;
use crate::Library;

/// A store holding work 1 and nothing pinned.
fn store() -> Library {
    let library = Library::open_in_memory().expect("opens");
    library
        .conn()
        .execute(
            "INSERT INTO work (id, content_key, slug, title, reading, hub)
             VALUES (1, 'k', 's', 'T', 'ltr', 'en')",
            [],
        )
        .expect("a work");
    library
}

fn at(edition: &str, hub_block: u32, block: u32) -> Pin {
    Pin {
        edition: edition.to_string(),
        hub_block,
        block,
    }
}

#[test]
fn pins_read_back_by_edition_then_hub_block() {
    let library = store();
    let conn = library.conn();
    for p in [at("ko", 9, 7), at("fr", 3, 3), at("ko", 2, 2)] {
        pin(&conn, 1, &p).expect("pins");
    }
    assert_eq!(
        pins(&conn, 1).expect("reads"),
        [at("fr", 3, 3), at("ko", 2, 2), at("ko", 9, 7)]
    );
}

#[test]
fn a_pin_takes_away_those_on_its_edition_it_crosses_and_no_others() {
    let library = store();
    let conn = library.conn();
    for p in [
        at("ko", 2, 2),
        at("ko", 5, 6),
        at("ko", 9, 12),
        at("fr", 5, 9),
    ] {
        pin(&conn, 1, &p).expect("pins");
    }
    pin(&conn, 1, &at("ko", 6, 5)).expect("pins across");
    assert_eq!(
        pins(&conn, 1).expect("reads"),
        [
            at("fr", 5, 9),
            at("ko", 2, 2),
            at("ko", 6, 5),
            at("ko", 9, 12)
        ],
        "5→6 crosses 6→5 and goes"
    );
    pin(&conn, 1, &at("ko", 6, 8)).expect("pins again");
    assert_eq!(
        pins(&conn, 1).expect("reads"),
        [
            at("fr", 5, 9),
            at("ko", 2, 2),
            at("ko", 6, 8),
            at("ko", 9, 12)
        ],
        "one hub block holds one pin"
    );
}

#[test]
fn unpin_says_whether_there_was_a_pin() {
    let library = store();
    let conn = library.conn();
    pin(&conn, 1, &at("ko", 2, 3)).expect("pins");
    assert_eq!(unpin(&conn, 1, "ko", 2), Ok(true));
    assert_eq!(unpin(&conn, 1, "ko", 2), Ok(false));
    assert_eq!(pins(&conn, 1).expect("reads"), []);
}

#[test]
fn a_works_removal_takes_its_pins() {
    let library = store();
    let conn = library.conn();
    pin(&conn, 1, &at("ko", 2, 3)).expect("pins");
    conn.execute("DELETE FROM work WHERE id = 1", [])
        .expect("removes");
    let left: i64 = conn
        .query_row("SELECT COUNT(*) FROM book_pin", [], |row| row.get(0))
        .expect("counts");
    assert_eq!(left, 0);
}
