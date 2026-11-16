//! A work whose editions are books: reflowable EPUBs, read as text blocks
//! rather than as pages.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, OnceLock},
};

use axum::{
    extract::{Path as AxPath, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use taiyaku_core::{BlockLink, Sidecar};
use taiyaku_library::book_pins;

use crate::ServeState;

/// Whether `edition`, keyed `key`, is a book: one member, in `books`, the
/// editions [`books_of`] answers.
///
/// Told by what the scan read the file as, never by its name, so the reader
/// the web opens on the library's word (`browse::books_among`), this route
/// and the feed agree about one file: a reflowable EPUB named `.zip` is a
/// book, and a comic archive named `.epub` is not.
pub(crate) fn is_book(key: &str, edition: &taiyaku_core::Edition, books: &HashSet<String>) -> bool {
    edition.members.len() == 1 && books.contains(key)
}

/// The keys of `work`'s editions the library's scan read as books
/// (`browse::book_keys`), none for a work the library holds no row for.
pub(crate) fn books_of(
    conn: &rusqlite::Connection,
    work: &crate::Work,
) -> Result<HashSet<String>, String> {
    match crate::works::find_row(conn, work)? {
        Some(work_id) => taiyaku_library::browse::book_keys(conn, work_id),
        None => Ok(HashSet::new()),
    }
}

/// [`books_of`], asked on the blocking pool, since the library's lock is a
/// blocking one; `Err` is the 500 to send where the library cannot answer.
#[expect(
    clippy::result_large_err,
    reason = "the `Err` carries a `Response`, for `routes::on_the_grant`'s reason: a \
              routing handler's refusal path, called once per request"
)]
pub(crate) async fn read_books(
    state: &Arc<ServeState>,
    work: &Arc<crate::Work>,
) -> Result<HashSet<String>, Response> {
    let state = Arc::clone(state);
    let work = Arc::clone(work);
    let id = work.id.clone();
    match tokio::task::spawn_blocking(move || books_of(&state.library.conn(), &work)).await {
        Ok(Ok(books)) => Ok(books),
        Ok(Err(why)) => Err(crate::routes::answered_and_logged(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("work `{id}`'s books could not be read"),
            &why,
        )),
        Err(why) => Err(crate::routes::answered_and_logged(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("work `{id}`'s books could not be read"),
            &why.to_string(),
        )),
    }
}

/// `GET /api/works/{work}/book`: each book edition the caller sees, as text
/// blocks, and how the hub's blocks align with each other edition's, through
/// the links a person pinned ([`put_pin`]).
///
/// Refused as `get_sidecar` refuses: a work the grant hides answers 404, and
/// an edition it hides is left out. Reading a book is reading its pages, so an
/// account that may not stream is refused too.
pub(crate) async fn get_book(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    extensions: axum::http::Extensions,
) -> Response {
    let view = match crate::view::of_work(&state, &extensions, &id).await {
        Ok(view) => view,
        Err(refusal) => return refusal,
    };
    let Some(work) = state.works.get(&id) else {
        return (StatusCode::NOT_FOUND, format!("no work `{id}`")).into_response();
    };
    if work.is_missing() {
        return crate::routes::missing_work(&id);
    }
    if let Some(refusal) = crate::routes::refuse_if_cannot_stream(&extensions) {
        return refusal;
    }
    let sidecar = Arc::clone(view.sidecar());
    let read = tokio::task::spawn_blocking(move || {
        let (pins, books) = {
            let conn = state.library.conn();
            (pins_of(&conn, &work)?, books_of(&conn, &work)?)
        };
        document(&work, &sidecar, &work.sidecar().hub, &pins, &books)
    })
    .await;
    match read {
        Ok(Ok(document)) => axum::Json(document).into_response(),
        Ok(Err(why)) => crate::routes::unreadable(&format!("work `{id}`'s book"), &why),
        Err(why) => crate::routes::answered_and_logged(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("work `{id}`'s book could not be read"),
            &why.to_string(),
        ),
    }
}

/// The document [`get_book`] sends: a book of none, `hub` null, for a work
/// with no book edition, which a reader can then say rather than guess.
///
/// `sidecar` is the caller's view of the work, whose hub is another edition
/// where the grant hides the work's own; `pins` count the blocks of the work's
/// own hub, `pinned_to`, so they are aligned through only where that is the
/// hub sent. `book_keys` are [`books_of`]'s. `Err` where a book's file is
/// there and does not read, or where no book reads.
fn document(
    work: &crate::Work,
    sidecar: &Sidecar,
    pinned_to: &str,
    pins: &[book_pins::Pin],
    book_keys: &HashSet<String>,
) -> Result<serde_json::Value, String> {
    let mut books = Vec::new();
    let mut unread = None;
    for (key, edition) in sidecar
        .editions
        .iter()
        .filter(|(key, e)| is_book(key, e, book_keys))
    {
        let paths = crate::download::member_paths(work, edition);
        let [path] = paths.as_slice() else {
            continue;
        };
        // A file no longer there since the last scan leaves its edition out,
        // logged, rather than closing the book: the hub falls to the first
        // edition read, as it does once a scan counts the file gone, and a
        // pin on a left-out edition, or on a left-out hub, has nothing to
        // hold on. A file still there that does not read closes it, as a
        // retry may read it; a hub left out would send another hub.
        match read(&edition.members[0].sha256, path) {
            Ok(book) => books.push((key, edition, book)),
            Err(why) if !matches!(path.try_exists(), Ok(false)) => return Err(why),
            Err(why) => {
                crate::routes::log_line(
                    &mut std::io::stderr(),
                    &format!("work `{}`'s book edition `{key}` was left out", work.id),
                    &why,
                );
                unread.get_or_insert(why);
            }
        }
    }
    if books.is_empty()
        && let Some(why) = unread
    {
        return Err(why);
    }
    let hub = books
        .iter()
        .position(|(key, ..)| **key == sidecar.hub)
        .unwrap_or(0);
    let Some((hub_key, hub_edition, hub_book)) = books.get(hub) else {
        return Ok(serde_json::json!({
            "title": sidecar.work.title,
            "hub": null,
            "editions": {},
            "alignments": [],
        }));
    };

    let mut alignments = Vec::new();
    for (key, edition, book) in &books {
        if key == hub_key {
            continue;
        }
        let pinned: Vec<(usize, usize)> = pins
            .iter()
            .filter(|pin| **hub_key == pinned_to && pin.edition == **key)
            .map(|pin| (pin.hub_block as usize, pin.block as usize))
            .collect();
        let found = aligned(
            (&hub_edition.members[0].sha256, &hub_book.book.blocks),
            (&edition.members[0].sha256, &book.book.blocks),
            &pinned,
        );
        let links = &found.blocks;
        // A pin the aligner passed over, out of order or past a book's end,
        // holds nothing, so it is not sent as holding.
        let held: Vec<&(usize, usize)> = pinned
            .iter()
            .filter(|(hub, block)| {
                links
                    .iter()
                    .any(|l| l.a.start == *hub && l.b.start == *block)
            })
            .collect();
        let sent = |links: &[BlockLink]| -> Vec<[usize; 4]> {
            links
                .iter()
                .map(|l| [l.a.start, l.a.end, l.b.start, l.b.end])
                .collect()
        };
        alignments.push(serde_json::json!({
            "to": key,
            "links": sent(links),
            "sentences": sent(&found.sentences),
            "pins": held,
        }));
    }

    let editions: serde_json::Map<String, serde_json::Value> = books
        .iter()
        .map(|(key, edition, read)| {
            let blocks: Vec<serde_json::Value> = read
                .book
                .blocks
                .iter()
                .zip(&read.sentences)
                .map(|(b, sentences)| {
                    serde_json::json!({
                        "text": b.text,
                        "heading": b.kind == taiyaku_read::BlockKind::Heading,
                        "line": b.line,
                        "sentences": sentences,
                    })
                })
                .collect();
            (
                (*key).clone(),
                serde_json::json!({
                    "label": edition.label,
                    "language": read.book.language,
                    "blocks": blocks,
                }),
            )
        })
        .collect();
    Ok(serde_json::json!({
        "title": sidecar.work.title,
        "hub": hub_key,
        "editions": editions,
        "alignments": alignments,
    }))
}

/// How many alignments, and how many books read, each cache keeps: past this
/// it starts again rather than grow with every book ever opened. Chosen, not
/// measured: room for every edition and pair of the few works being read at
/// once, while a full cache of books holds no more than that many books.
const KEPT: usize = 32;

/// An edition's book as [`taiyaku_read::read_book`] reads it, with each
/// block's sentences in UTF-16 units ([`utf16_ranges`]), as [`document`]
/// sends them.
struct ReadBook {
    book: taiyaku_read::Book,
    sentences: Vec<Vec<[usize; 2]>>,
}

/// The book at `path`, read and split into sentences once for the `sha256`
/// the scan recorded of its file, as [`aligned`] is cached: a reader asks for
/// the whole book again after every pin, and reading one unzips and parses
/// every chapter, while only a new file, which the next scan hashes anew,
/// changes what it holds.
fn read(sha256: &str, path: &std::path::Path) -> Result<Arc<ReadBook>, String> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<ReadBook>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some(read) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(sha256) {
        return Ok(Arc::clone(read));
    }
    let book = taiyaku_read::read_book(path)?;
    let sentences = book
        .blocks
        .iter()
        .map(|b| utf16_ranges(&b.text, &taiyaku_core::sentences(&b.text)))
        .collect();
    let read = Arc::new(ReadBook { book, sentences });
    let mut held = cache.lock().unwrap_or_else(|e| e.into_inner());
    if held.len() >= KEPT && !kept_whole() {
        held.clear();
    }
    held.insert(sha256.to_string(), Arc::clone(&read));
    Ok(read)
}

/// How many tests hold [`read`]'s cache whole (`tests::KeptWhole`).
#[cfg(test)]
static KEEPING_WHOLE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Whether [`read`] keeps its cache past [`KEPT`]: only while a test holds it
/// whole, since the tests of one `cargo test` process read books into the one
/// cache, and a test that asks whether a book was read once would see it read
/// again whenever another test emptied the cache between its requests.
#[cfg(test)]
fn kept_whole() -> bool {
    KEEPING_WHOLE.load(std::sync::atomic::Ordering::SeqCst) > 0
}

/// [`read`] empties its cache at [`KEPT`]; only a test holds it whole.
#[cfg(not(test))]
const fn kept_whole() -> bool {
    false
}

/// Two books' links, block by block and, inside those, sentence by sentence,
/// each sentence numbered through its book.
struct Found {
    blocks: Vec<BlockLink>,
    sentences: Vec<BlockLink>,
}

/// Alignments by the `sha256`s of the two files aligned and the pins between
/// them.
type Aligned = HashMap<(String, String, Vec<(usize, usize)>), Arc<Found>>;

/// The alignment of two books through `pins`, cached by their files'
/// `sha256`s and the pins, since aligning a pair is slow (`docs/notes.md`,
/// "Books: Harry Potter read in the browser, end to end") and its answer
/// changes only with them.
fn aligned(
    (a_sha, a): (&str, &[taiyaku_read::Block]),
    (b_sha, b): (&str, &[taiyaku_read::Block]),
    pins: &[(usize, usize)],
) -> Arc<Found> {
    static CACHE: OnceLock<Mutex<Aligned>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let key = (a_sha.to_string(), b_sha.to_string(), pins.to_vec());
    if let Some(links) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return Arc::clone(links);
    }
    let texts = |blocks: &[taiyaku_read::Block]| -> Vec<String> {
        blocks.iter().map(|b| b.text.clone()).collect()
    };
    let (a, b) = (texts(a), texts(b));
    let a: Vec<&str> = a.iter().map(String::as_str).collect();
    let b: Vec<&str> = b.iter().map(String::as_str).collect();
    let blocks = taiyaku_core::align_texts_pinned(&a, &b, pins);
    let split = |texts: &[&str]| -> Vec<Vec<String>> {
        texts
            .iter()
            .map(|t| {
                taiyaku_core::sentences(t)
                    .into_iter()
                    .map(|r| t[r].to_string())
                    .collect()
            })
            .collect()
    };
    let sentences = taiyaku_core::align_sentences(&split(&a), &split(&b), &blocks);
    let links = Arc::new(Found { blocks, sentences });
    let mut held = cache.lock().unwrap_or_else(|e| e.into_inner());
    if held.len() >= KEPT {
        held.clear();
    }
    held.insert(key, Arc::clone(&links));
    links
}

/// `ranges` of `text`'s bytes as the browser counts the same text: in UTF-16
/// code units, which is how a note's run of text is counted too.
fn utf16_ranges(text: &str, ranges: &[std::ops::Range<usize>]) -> Vec<[usize; 2]> {
    let mut units = Vec::with_capacity(text.len() + 1);
    let mut n = 0;
    for (at, c) in text.char_indices() {
        units.resize(at + 1, n);
        n += c.len_utf16();
    }
    units.resize(text.len() + 1, n);
    ranges
        .iter()
        .map(|r| [units[r.start], units[r.end]])
        .collect()
}

/// The pins on `work`, none for a work the library holds no row for.
fn pins_of(conn: &rusqlite::Connection, work: &crate::Work) -> Result<Vec<book_pins::Pin>, String> {
    match crate::works::find_row(conn, work)? {
        Some(work_id) => book_pins::pins(conn, work_id),
        None => Ok(Vec::new()),
    }
}

/// What `PUT /api/works/{work}/book/pins` asks: that the hub's block
/// `hub_block` and `edition`'s block `block` start one link, or, with `block`
/// null, that the pin at `hub_block` go. `hub` is the hub of the book the
/// blocks were counted in, as [`get_book`] sent it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PinRequest {
    edition: String,
    hub: String,
    hub_block: u32,
    block: Option<u32>,
}

/// `PUT /api/works/{work}/book/pins`: pin a link of a book's alignment, or
/// unpin one, answering 204. An administrator's, as correcting a pairing is
/// (O58): an alignment is every reader's. `edition` is a book edition other
/// than the hub, and each block one its book has, by its member's count,
/// counted in a book sent with the work's own hub, or 409.
pub(crate) async fn put_pin(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    extensions: axum::http::Extensions,
    body: axum::body::Bytes,
) -> Response {
    let Some(principal) = extensions.get::<crate::auth::Principal>() else {
        return crate::routes::NoPrincipal.into_response();
    };
    if let Some(refusal) = crate::session::administering_or_forbidden(principal, &extensions) {
        return refusal;
    }
    let request: PinRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(why) => return (StatusCode::BAD_REQUEST, format!("not a pin: {why}")).into_response(),
    };
    let Some(work) = state.works.get(&id) else {
        return (StatusCode::NOT_FOUND, format!("no work `{id}`")).into_response();
    };
    let books = match read_books(&state, &work).await {
        Ok(books) => books,
        Err(refusal) => return refusal,
    };
    let sidecar = work.sidecar();
    let blocks = |key: &str| {
        sidecar
            .editions
            .get(key)
            .filter(|edition| is_book(key, edition, &books))
            .map(|edition| edition.members[0].pages)
    };
    let (Some(hub_blocks), Some(edition_blocks)) = (blocks(&sidecar.hub), blocks(&request.edition))
    else {
        return (
            StatusCode::NOT_FOUND,
            format!(
                "work `{id}` has no book edition `{}` beside a book hub to pin",
                request.edition
            ),
        )
            .into_response();
    };
    // Pins are kept in the blocks of the work's own hub. `get_book` sends
    // another hub while that hub's file is gone with no scan since, and a pin
    // counted in its blocks would join other blocks here and take away the
    // pins it would seem to cross.
    if request.hub != sidecar.hub {
        return (
            StatusCode::CONFLICT,
            format!(
                "this pin counts the blocks of `{}`, and work `{id}`'s pins count those of its \
                 hub `{}`",
                request.hub, sidecar.hub
            ),
        )
            .into_response();
    }
    let past = request.hub_block >= hub_blocks
        || request.block.is_some_and(|block| block >= edition_blocks)
        || request.edition == sidecar.hub;
    if past {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "a pin joins a block of the hub, of {hub_blocks}, to a block of another edition, \
                 of {edition_blocks}; this one does not"
            ),
        )
            .into_response();
    }
    let written = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let Some(work_id) = crate::works::find_row(&conn, &work)? else {
            return Err(format!(
                "work `{}` has no row in the library to pin on",
                work.id
            ));
        };
        match request.block {
            Some(block) => book_pins::pin(
                &conn,
                work_id,
                &book_pins::Pin {
                    edition: request.edition,
                    hub_block: request.hub_block,
                    block,
                },
            ),
            None => {
                book_pins::unpin(&conn, work_id, &request.edition, request.hub_block).map(|_| ())
            }
        }
    })
    .await;
    match written {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(why)) => crate::routes::answered_and_logged(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("work `{id}`'s pin could not be written"),
            &why,
        ),
        Err(why) => crate::routes::answered_and_logged(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("work `{id}`'s pin could not be written"),
            &why.to_string(),
        ),
    }
}

#[cfg(test)]
mod tests;
