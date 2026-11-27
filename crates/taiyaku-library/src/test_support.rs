//! Fixtures for tests in this crate and, through the `test-support` feature,
//! in `taiyaku-serve`'s: one copy of each, so a fixture in either crate files
//! a planted directory the way the other's does.
//!
//! `taiyaku-serve` enables the feature only under `[dev-dependencies]`, so this
//! is compiled into test builds and `--all-features` builds and into no binary
//! this workspace ships. The file name is the one the coverage report and
//! `.cargo/mutants.toml` exclude.

use std::path::Path;
use std::sync::Mutex;

use haiyaku::log::{
    BookLog, BookRecord, Chapter, Document, Entity, EntityKind, Event, EventBody, Name, Position,
    RelationKind, Sex,
};
use haiyaku::store::{Order, Series};
use rusqlite::Connection;

use crate::Library;
use crate::progress::{Step, Watch};

/// Link directory `root_id` into library `library_id` as well, by SQL: for a
/// fixture whose directory need not exist, so cannot be linked by the add.
///
/// # Panics
///
/// If the link cannot be written.
pub fn link(conn: &Connection, library_id: i64, root_id: i64) {
    conn.execute(
        "INSERT INTO library_root (library_id, root_id) VALUES (?1, ?2)",
        [library_id, root_id],
    )
    .unwrap_or_else(|e| panic!("links directory {root_id} into library {library_id}: {e}"));
}

/// A `root` row with id `id` at `path`, which need not exist, filed under a
/// library of its own named as `scan::add_root` names one. Answers the
/// library's id.
///
/// # Panics
///
/// If the row cannot be written or filed.
pub fn plant_root(conn: &Connection, id: i64, path: &str) -> i64 {
    conn.execute(
        "INSERT INTO root (id, path) VALUES (?1, ?2)",
        rusqlite::params![id, path],
    )
    .unwrap_or_else(|e| panic!("plants root {id} at {path}: {e}"));
    crate::scan::file_under_its_own_library(conn, id)
        .unwrap_or_else(|e| panic!("files root {id} under a library of its own: {e}"))
}

/// The lowest library directory `root_id` is linked to: its one library,
/// while a directory is in one.
///
/// # Panics
///
/// If the directory is in no library.
#[must_use]
pub fn library_of_root(conn: &Connection, root_id: i64) -> i64 {
    conn.query_row(
        "SELECT min(library_id) FROM library_root WHERE root_id = ?1",
        [root_id],
        |r| r.get(0),
    )
    .unwrap_or_else(|e| panic!("directory {root_id} is in a library: {e}"))
}

/// One report a [`Recording`] was given: the step, the count done, the count
/// of.
pub type Report = (Step, u64, Option<u64>);

/// A [`Watch`] that keeps every report and every count of volumes written,
/// in order, and answers cancelled from the first report `stop` matches on,
/// as a job asked to stop stays asked.
pub struct Recording {
    reports: Mutex<Vec<Report>>,
    written: Mutex<Vec<u64>>,
    stop: Box<dyn Fn(Report) -> bool + Send + Sync>,
}

impl Recording {
    /// A recording that answers cancelled once a report matches `stop`.
    #[must_use]
    pub fn cancelling(stop: impl Fn(Report) -> bool + Send + Sync + 'static) -> Self {
        Self {
            reports: Mutex::new(Vec::new()),
            written: Mutex::new(Vec::new()),
            stop: Box::new(stop),
        }
    }

    /// Every report so far, in the order given.
    ///
    /// # Panics
    ///
    /// If a thread panicked while it held the reports.
    #[must_use]
    pub fn reports(&self) -> Vec<Report> {
        self.reports.lock().expect("the reports").clone()
    }

    /// Every count [`Watch::written`] was given so far, in the order given.
    ///
    /// # Panics
    ///
    /// If a thread panicked while it held the counts.
    #[must_use]
    pub fn written_so_far(&self) -> Vec<u64> {
        self.written.lock().expect("the counts").clone()
    }
}

/// Never cancelled.
impl Default for Recording {
    fn default() -> Self {
        Self::cancelling(|_| false)
    }
}

impl std::fmt::Debug for Recording {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recording")
            .field("reports", &self.reports)
            .field("written", &self.written)
            .finish_non_exhaustive()
    }
}

impl Watch for Recording {
    fn at(&self, step: Step, done: u64, of: Option<u64>) {
        self.reports
            .lock()
            .expect("the reports")
            .push((step, done, of));
    }

    fn cancelled(&self) -> bool {
        let reports = self.reports.lock().expect("the reports");
        reports.iter().any(|report| (self.stop)(*report))
    }

    fn written(&self, works: u64) {
        self.written.lock().expect("the counts").push(works);
    }
}

/// "The Lamp Keepers", the invented book A: each spine document's path and its
/// paragraphs. The front matter is in its log with no chapter, and the
/// afterword is a spine item its log does not list.
pub const LAMP_KEEPERS: &[(&str, &[&str])] = &[
    ("OEBPS/front.xhtml", &["The Lamp Keepers"]),
    (
        "OEBPS/c1.xhtml",
        &[
            "Ada Reed lit the lamp.",
            "Ned Reed and Tom Field kept it burning.",
            "Tom Field showed Ned Reed the wick.",
        ],
    ),
    (
        "OEBPS/c2.xhtml",
        &[
            "Mara Vell came at dusk.",
            "Ned Reed knew his cousin at once.",
            "Mrs. Vell said nothing.",
        ],
    ),
    (
        "OEBPS/c3.xhtml",
        &[
            "Mara Vell came back.",
            "Ned Reed saw that Tom Field had lied.",
        ],
    ),
    ("OEBPS/c4.xhtml", &["The Reeds put out the lamp."]),
    ("OEBPS/notes.xhtml", &["Afterword."]),
];

/// "The Second Lamp", the invented book B, which follows A in the series.
pub const SECOND_LAMP: &[(&str, &[&str])] = &[
    ("OEBPS/c1.xhtml", &["Wren Hollis found the lamp."]),
    ("OEBPS/c2.xhtml", &["Ned Reed wrote to Wren Hollis."]),
];

/// "A Lamp Alone", the invented book C, in no series.
pub const LAMP_ALONE: &[(&str, &[&str])] = &[
    ("OEBPS/c1.xhtml", &["Iris Moor waited."]),
    ("OEBPS/c2.xhtml", &["Iris Moor left."]),
];

/// "Names Abroad", the invented book D: a log with no EPUB, whose names take
/// more bytes than characters and, for 𠮷, more UTF-16 units than characters.
/// One of them, 다온 마루, is of two words.
pub(crate) const NAMES_ABROAD: &[(&str, &[&str])] = &[
    (
        "OEBPS/c1.xhtml",
        &[
            "에이다는 웃었다.",
            "𠮷田さんは来た。",
            "다온 마루는 늦었다.",
        ],
    ),
    ("OEBPS/c2.xhtml", &[]),
];

/// "The Ferry House", the invented book E: a log with no EPUB, in which one
/// character is shown under a name that others carry as another of theirs.
pub(crate) const FERRY_HOUSE: &[(&str, &[&str])] = &[
    (
        "OEBPS/c1.xhtml",
        &[
            "Mr. Dunn kept the ferry house.",
            "Ira Dunn, the ferryman, rowed at dawn.",
            "Hal Dunn mended the rope.",
            "Nell Carrow sold the tickets.",
        ],
    ),
    (
        "OEBPS/c2.xhtml",
        &["They called for Mr. Dunn, and Ira Dunn came."],
    ),
    (
        "OEBPS/c3.xhtml",
        &[
            "Hal Dunn was Mr. Dunn to the crew now.",
            "Hal Dunn was the ferryman that spring.",
        ],
    ),
    ("OEBPS/c4.xhtml", &["Nell Carrow became Mrs. Dunn."]),
    (
        "OEBPS/c5.xhtml",
        &[
            "Mr. Dunn watched the river.",
            "The Dunns kept the ferry.",
            "The ferryman slept.",
        ],
    ),
];

/// "The Weir Ledger", the invented book F: a log with no EPUB, in which one
/// character's name is the last two words of another's.
pub(crate) const WEIR_LEDGER: &[(&str, &[&str])] = &[
    (
        "OEBPS/c1.xhtml",
        &[
            "Dame Edda Sorrel kept the weir ledger.",
            "Edda Sorrel, her niece, kept the key.",
        ],
    ),
    ("OEBPS/c2.xhtml", &[]),
];

/// Store A's log under `sha256`.
///
/// # Panics
///
/// If haiyaku refuses the log.
pub fn store_lamp_keepers(conn: &mut Connection, sha256: &str) {
    let mara_as_mrs = Name {
        text: "Mrs. Vell".to_owned(),
        given: None,
        surname: Some("Vell".to_owned()),
        title: Some("Mrs.".to_owned()),
        title_sex: Some(Sex::Female),
    };
    Invented {
        title: "The Lamp Keepers",
        language: "en",
        spine: LAMP_KEEPERS,
        front: 1,
        chapters: &["The Lamp", "Dusk", "Return", "Out"],
        events: vec![
            (
                1,
                1,
                introduced("ada", "Ada Reed", Some("Ada"), Some("Reed")),
            ),
            (
                1,
                2,
                introduced("ned", "Ned Reed", Some("Ned"), Some("Reed")),
            ),
            (
                1,
                2,
                introduced("tom", "Tom Field", Some("Tom"), Some("Field")),
            ),
            (
                1,
                2,
                related(
                    "ned",
                    "tom",
                    RelationKind::Friend,
                    "they keep the lamp together",
                ),
            ),
            (
                1,
                3,
                related(
                    "tom",
                    "ned",
                    RelationKind::Mentor,
                    "Tom taught him the wick",
                ),
            ),
            (
                2,
                1,
                introduced("mara", "Mara Vell", Some("Mara"), Some("Vell")),
            ),
            (
                2,
                3,
                EventBody::Alias {
                    entity: "mara".to_owned(),
                    name: mara_as_mrs,
                },
            ),
            (
                2,
                2,
                related("ned", "mara", RelationKind::Family, "cousins"),
            ),
            (
                3,
                1,
                EventBody::Appeared {
                    entity: "mara".to_owned(),
                },
            ),
            // Said again a chapter later, so that a card's fact has a first
            // and a last that differ.
            (
                3,
                1,
                related("ned", "mara", RelationKind::Family, "cousins"),
            ),
            (
                3,
                2,
                EventBody::Overturned {
                    from: "ned".to_owned(),
                    to: "tom".to_owned(),
                    kind: RelationKind::Friend,
                    note: "Tom only pretended".to_owned(),
                },
            ),
        ],
    }
    .store(conn, sha256);
}

/// Store B's log under `sha256`.
///
/// # Panics
///
/// If haiyaku refuses the log.
pub fn store_second_lamp(conn: &mut Connection, sha256: &str) {
    Invented {
        title: "The Second Lamp",
        language: "en",
        spine: SECOND_LAMP,
        front: 0,
        chapters: &["Wren", "Letters"],
        events: vec![(
            1,
            1,
            introduced("wren", "Wren Hollis", Some("Wren"), Some("Hollis")),
        )],
    }
    .store(conn, sha256);
}

/// Store C's log under `sha256`.
///
/// # Panics
///
/// If haiyaku refuses the log.
pub fn store_lamp_alone(conn: &mut Connection, sha256: &str) {
    Invented {
        title: "A Lamp Alone",
        language: "en",
        spine: LAMP_ALONE,
        front: 0,
        chapters: &["Waiting", "Leaving"],
        events: vec![(
            1,
            1,
            introduced("iris", "Iris Moor", Some("Iris"), Some("Moor")),
        )],
    }
    .store(conn, sha256);
}

/// Store D's log under `sha256`.
///
/// # Panics
///
/// If haiyaku refuses the log.
pub fn store_names_abroad(conn: &mut Connection, sha256: &str) {
    Invented {
        title: "Names Abroad",
        language: "ko",
        spine: NAMES_ABROAD,
        front: 0,
        chapters: &["Arrivals", "End"],
        events: vec![
            (1, 1, introduced("eida", "에이다", Some("에이다"), None)),
            (1, 2, introduced("yoshida", "𠮷田", None, Some("𠮷田"))),
            (
                1,
                3,
                introduced("daon", "다온 마루", Some("다온"), Some("마루")),
            ),
        ],
    }
    .store(conn, sha256);
}

/// Store E's log under `sha256`. The character shown as "Mr. Dunn" is seen
/// in chapter 1 alone; Ira Dunn, Hal Dunn and Nell Carrow are last seen in
/// chapters 2, 3 and 4, each there given another name. Ira Dunn is "the
/// ferryman" before he is "Mr. Dunn", and Hal Dunn the other way round.
///
/// # Panics
///
/// If haiyaku refuses the log.
pub fn store_ferry_house(conn: &mut Connection, sha256: &str) {
    let titled = |text: &str, title: &str, sex| Name {
        text: text.to_owned(),
        given: None,
        surname: Some("Dunn".to_owned()),
        title: Some(title.to_owned()),
        title_sex: Some(sex),
    };
    let mr_dunn = || titled("Mr. Dunn", "Mr.", Sex::Male);
    let ferryman = || Name {
        text: "the ferryman".to_owned(),
        given: None,
        surname: None,
        title: None,
        title_sex: None,
    };
    Invented {
        title: "The Ferry House",
        language: "en",
        spine: FERRY_HOUSE,
        front: 0,
        chapters: &["The Ferry", "The Flood", "The Rope", "The Wedding", "After"],
        events: vec![
            (
                1,
                1,
                EventBody::Introduced {
                    entity: "elder".to_owned(),
                    name: mr_dunn(),
                },
            ),
            (
                1,
                2,
                introduced("ira", "Ira Dunn", Some("Ira"), Some("Dunn")),
            ),
            (1, 2, alias("ira", ferryman())),
            (
                1,
                3,
                introduced("hal", "Hal Dunn", Some("Hal"), Some("Dunn")),
            ),
            (
                1,
                4,
                introduced("nell", "Nell Carrow", Some("Nell"), Some("Carrow")),
            ),
            (2, 1, alias("ira", mr_dunn())),
            (3, 1, alias("hal", mr_dunn())),
            (3, 2, alias("hal", ferryman())),
            (
                4,
                1,
                alias("nell", titled("Mrs. Dunn", "Mrs.", Sex::Female)),
            ),
        ],
    }
    .store(conn, sha256);
}

/// Store F's log under `sha256`.
///
/// # Panics
///
/// If haiyaku refuses the log.
pub fn store_weir_ledger(conn: &mut Connection, sha256: &str) {
    Invented {
        title: "The Weir Ledger",
        language: "en",
        spine: WEIR_LEDGER,
        front: 0,
        chapters: &["The Ledger", "End"],
        events: vec![
            (
                1,
                1,
                EventBody::Introduced {
                    entity: "dame".to_owned(),
                    name: Name {
                        text: "Dame Edda Sorrel".to_owned(),
                        given: Some("Edda".to_owned()),
                        surname: Some("Sorrel".to_owned()),
                        title: Some("Dame".to_owned()),
                        title_sex: Some(Sex::Female),
                    },
                },
            ),
            (
                1,
                2,
                introduced("niece", "Edda Sorrel", Some("Edda"), Some("Sorrel")),
            ),
        ],
    }
    .store(conn, sha256);
}

/// Store the series "keepers", whose order `printed` reads `keepers` (A) then
/// `second` (B) and whose order `author` reads them the other way round.
/// `printed` ranks first though `author` sorts first by name.
///
/// # Panics
///
/// If haiyaku refuses the series, as it does while either book is unstored.
pub fn store_keepers_series(conn: &mut Connection, keepers: &str, second: &str) {
    let order = |name: &str, books: [&str; 2]| Order {
        name: name.to_owned(),
        books: books.map(str::to_owned).to_vec(),
    };
    let series = Series {
        name: "keepers".to_owned(),
        title: "The Keepers".to_owned(),
        orders: vec![
            order("printed", [keepers, second]),
            order("author", [second, keepers]),
        ],
    };
    if let Err(e) = haiyaku::store::put_series(conn, &series) {
        panic!("stores the series keepers: {e}");
    }
}

/// [`keepers`]' books, by their letters.
#[derive(Debug)]
pub struct Keepers {
    pub a: Kept,
    pub b: Kept,
    pub c: Kept,
}

/// One of [`Keepers`]' books.
#[derive(Debug)]
pub struct Kept {
    /// The digest of its file, which its log is stored under.
    pub sha256: String,
    /// The id of the directory it was added as, or `None` when it was not
    /// shelved.
    pub root: Option<i64>,
}

/// The invented books A, B and C on disk under `dir`, each at
/// `<dir>/<Title>/<Title> v01.epub` and titled there unlike its log ("Keepers
/// One", "Keepers Two", "Alone"). Each book `shelve` names by its letter is
/// added as a directory of its own and scanned, and the library is derived.
/// Every book's log is stored, A's whether shelved or not, and the series
/// `keepers`, so a library can lack a book the series reads.
///
/// # Panics
///
/// If `shelve` names a letter that is not A, B or C, or a book cannot be
/// written, read, added, scanned, derived or stored.
pub fn keepers(library: &Library, dir: &Path, shelve: &[&str]) -> Keepers {
    let books = [
        ("A", "Keepers One", LAMP_KEEPERS),
        ("B", "Keepers Two", SECOND_LAMP),
        ("C", "Alone", LAMP_ALONE),
    ];
    assert!(
        shelve
            .iter()
            .all(|letter| books.iter().any(|(book, ..)| book == letter)),
        "keepers shelves only A, B and C: {shelve:?}"
    );
    let mut conn = library.conn();
    let [a, b, c] = books.map(|(letter, title, spine)| {
        let folder = dir.join(title);
        std::fs::create_dir_all(&folder)
            .unwrap_or_else(|e| panic!("creates {}: {e}", folder.display()));
        let file = taiyaku_read::test_support::epub_documents(
            &folder,
            &format!("{title} v01.epub"),
            &format!("<dc:title>{title}</dc:title>"),
            spine,
        );
        let sha256 = haiyaku::store::sha256_file(&file)
            .unwrap_or_else(|e| panic!("reads the digest of {}: {e}", file.display()));
        let root = shelve.contains(&letter).then(|| {
            let root = crate::scan::add_root(&conn, &folder)
                .unwrap_or_else(|e| panic!("adds {}: {e}", folder.display()));
            crate::scan::scan_root(&conn, &root)
                .unwrap_or_else(|e| panic!("scans {}: {e}", folder.display()));
            root.id
        });
        Kept { sha256, root }
    });
    crate::derive::derive(&conn).unwrap_or_else(|e| panic!("derives the keepers library: {e}"));
    store_lamp_keepers(&mut conn, &a.sha256);
    store_second_lamp(&mut conn, &b.sha256);
    store_lamp_alone(&mut conn, &c.sha256);
    store_keepers_series(&mut conn, &a.sha256, &b.sha256);
    Keepers { a, b, c }
}

/// An invented book's log, described by chapter and paragraph.
struct Invented {
    title: &'static str,
    language: &'static str,
    spine: &'static [(&'static str, &'static [&'static str])],
    /// How many spine documents come before the first chapter's. Each chapter
    /// has the one document after them, in order, and the spine documents
    /// past the last chapter's are not in the log.
    front: usize,
    /// The chapters' titles, chapter 1 first.
    chapters: &'static [&'static str],
    /// Each event's chapter, the paragraph of that chapter's document it
    /// quotes (from 1), and what it says. Every event sits at block 0.
    events: Vec<(u32, usize, EventBody)>,
}

impl Invented {
    fn store(self, conn: &mut Connection, sha256: &str) {
        let log = self.log(sha256);
        if let Err(e) = haiyaku::store::write_book(conn, &log) {
            panic!("stores the log of {}: {e}", log.book.title);
        }
    }

    fn log(self, sha256: &str) -> BookLog {
        let ordinal = |index: usize| u32::try_from(index).expect("a spine of few documents");
        let listed = &self.spine[..self.front + self.chapters.len()];
        let documents = listed
            .iter()
            .enumerate()
            .map(|(index, (path, _))| Document {
                ordinal: ordinal(index),
                path: (*path).to_owned(),
                chapter: index
                    .checked_sub(self.front)
                    .map(|before| ordinal(before) + 1),
            })
            .collect();
        let chapters = self
            .chapters
            .iter()
            .zip(1..)
            .map(|(title, number)| Chapter {
                number,
                toc_entry: number,
                title: (*title).to_owned(),
                seconds: 0.0,
                cost_usd: None,
                kept: 0,
                rejected: 0,
            })
            .collect();
        let entities = self
            .events
            .iter()
            .filter_map(|(_, _, body)| match body {
                EventBody::Introduced { entity, .. } => Some(Entity {
                    id: entity.clone(),
                    kind: EntityKind::Character,
                }),
                _ => None,
            })
            .collect();
        let events = self
            .events
            .into_iter()
            .zip(1..)
            .map(|((chapter, paragraph, body), seq)| {
                let index = self.front + usize::try_from(chapter).expect("a small chapter") - 1;
                Event {
                    seq,
                    chapter,
                    at: Position {
                        document: ordinal(index),
                        block: 0,
                    },
                    quote: self.spine[index].1[paragraph - 1].to_owned(),
                    body,
                }
            })
            .collect();
        BookLog {
            book: BookRecord {
                sha256: sha256.to_owned(),
                title: self.title.to_owned(),
                language: self.language.to_owned(),
                backend: "invented".to_owned(),
                model: "none".to_owned(),
                prompt_version: 1,
                extracted_at: "2026-11-24T00:00:00Z".to_owned(),
            },
            chapters,
            documents,
            entities,
            events,
        }
    }
}

fn introduced(entity: &str, text: &str, given: Option<&str>, surname: Option<&str>) -> EventBody {
    EventBody::Introduced {
        entity: entity.to_owned(),
        name: Name {
            text: text.to_owned(),
            given: given.map(str::to_owned),
            surname: surname.map(str::to_owned),
            title: None,
            title_sex: None,
        },
    }
}

fn alias(entity: &str, name: Name) -> EventBody {
    EventBody::Alias {
        entity: entity.to_owned(),
        name,
    }
}

fn related(from: &str, to: &str, kind: RelationKind, note: &str) -> EventBody {
    EventBody::Relation {
        from: from.to_owned(),
        to: to.to_owned(),
        kind,
        note: note.to_owned(),
    }
}
