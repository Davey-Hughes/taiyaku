//! `GET /api/about`: which build is running, where its source is, and the
//! version of each format taiyaku writes.
//!
//! Every value is read from the build or from the constant the format's own
//! writer uses, so the About page cannot drift from the code.

use axum::response::{IntoResponse, Response};

/// The commit `raw` names, or `None` when it names none.
///
/// Only a full 40-character lowercase hex hash counts, because the label shows
/// its first seven characters and CI tags the image with the same seven: a
/// shorter value, a value in another case or one that is not hex cannot be
/// matched against the image, and reporting no commit is better than reporting
/// a wrong one.
pub(crate) fn full_hash(raw: Option<&'static str>) -> Option<&'static str> {
    raw.filter(|commit| {
        commit.len() == 40
            && commit
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    })
}

/// What `/api/about` says about the build: the commit it was built from, and
/// whether its tracked files differed from that commit.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BuildLabel {
    pub(crate) commit: Option<&'static str>,
    pub(crate) dirty: bool,
}

/// `TAIYAKU_COMMIT` when it is a full hash, never marked; otherwise git's HEAD
/// as the build script read it, marked when the script saw changes; otherwise
/// no commit.
pub(crate) fn build_label(
    commit: Option<&'static str>,
    head: Option<&'static str>,
    dirty: Option<&'static str>,
) -> BuildLabel {
    if let Some(commit) = full_hash(commit) {
        return BuildLabel {
            commit: Some(commit),
            dirty: false,
        };
    }
    match full_hash(head) {
        Some(head) => BuildLabel {
            commit: Some(head),
            dirty: dirty.is_some(),
        },
        None => BuildLabel {
            commit: None,
            dirty: false,
        },
    }
}

/// `GET /api/about`.
///
/// **Behind the guard**, in `routes::guarded_routes`: nothing here is secret,
/// but an open route needs a reason a guarded one cannot serve it
/// (`routes::open_routes`), and this has none.
///
/// The label is read at compile time, from the three values [`build_label`]
/// weighs. `TAIYAKU_COMMIT` is passed in; the other two come from this crate's
/// build script, which asks git for HEAD and whether tracked files differ from
/// it. The image's build context holds no `.git`, so there the script finds no
/// repository and the Dockerfile's argument is the only source.
///
/// Cargo rebuilds this crate when `TAIYAKU_COMMIT` changes or the script runs
/// again. The script runs again when HEAD moves or git's index changes (a
/// commit, a checkout, a stage), or a crate's `src` or `Cargo.toml` or the
/// workspace's `Cargo.toml` or `Cargo.lock` does, not on every build, which
/// would recompile this crate every time. So an edit anywhere else, a crate's
/// `tests/` included, can go unseen until then, and the mark can be stale.
/// Where git is missing, refuses the repository, or there was no repository
/// yet, the script watches none of git's state, so once git can answer, the
/// label reads "unknown build" until one of those Rust files changes.
pub(crate) async fn about() -> Response {
    (axum::Json(about_body(&this_label())),).into_response()
}

/// This binary's label, from the three values its build was given.
fn this_label() -> BuildLabel {
    build_label(
        option_env!("TAIYAKU_COMMIT"),
        option_env!("TAIYAKU_GIT_HEAD"),
        option_env!("TAIYAKU_GIT_DIRTY"),
    )
}

/// The build a stored fingerprint names: the label's commit, with `-dirty`
/// when it is marked, or `None` for a build that names no commit.
///
/// The mark is part of it because a marked build of a commit need not read
/// pages as that commit's clean build does.
pub(crate) fn build_stamp(label: &BuildLabel) -> Option<String> {
    label.commit.map(|commit| {
        if label.dirty {
            format!("{commit}-dirty")
        } else {
            commit.to_owned()
        }
    })
}

/// This binary's [`build_stamp`].
pub(crate) fn this_build() -> Option<String> {
    build_stamp(&this_label())
}

/// What `GET /api/about` answers for a build `label` describes.
///
/// It takes the label rather than reading the build itself so that a test can
/// hand it a changed build: CI builds with `TAIYAKU_COMMIT`, which is never
/// marked, so a body that answered a fixed `false` would pass there.
///
/// Every version is a string because the OPDS one is not an integer, and a
/// format with no version is `null` rather than missing, so the client reads
/// one shape for every row.
///
/// Each row's `description` is the line About draws under its name, for a
/// person who runs taiyaku rather than one who works on it. It sits beside
/// the name here so a row and what it says about itself cannot part. About
/// draws the rows in this order, so the five `<stem>` files sit next to each
/// other here for a reader to find them together.
pub(crate) fn about_body(label: &BuildLabel) -> serde_json::Value {
    serde_json::json!({
        "commit": label.commit,
        "dirty": label.dirty,
        "repository": env!("CARGO_PKG_REPOSITORY"),
        "catalog": crate::opds::catalog_path(),
        "formats": [
            {
                "name": "taiyaku.json",
                "version": taiyaku_core::VERSION.to_string(),
                "description": "Sits beside a work’s archives and records how its editions correspond to each other.",
            },
            {
                "name": "<stem>.library.json",
                "version": taiyaku_library::librarydoc::VERSION.to_string(),
                "description": "A work’s metadata, kept in a file so fixes made by hand outlive the database.",
            },
            {
                "name": "<stem>.progress.json",
                "version": taiyaku_library::progressdoc::VERSION.to_string(),
                "description": "One reader’s place in a work, edition by edition.",
            },
            {
                "name": "<stem>.truth.json",
                "version": taiyaku_core::TRUTH_VERSION.to_string(),
                "description": "Boxes a person drew around the text on pages, to score text detection against.",
            },
            {
                "name": "<stem>.regions.json",
                "version": taiyaku_core::REGIONS_VERSION.to_string(),
                "description": "Where the text is on each page, as the regions command found it.",
            },
            {
                "name": "<stem>.notes.json",
                "version": taiyaku_library::NOTES_VERSION.to_string(),
                "description": "Notes one reader left on a work’s pages.",
            },
            {
                "name": "Database schema",
                "version": taiyaku_library::schema::SCHEMA_VERSION.to_string(),
                "description": "The layout of the SQLite database taiyaku keeps its library in.",
            },
            {
                "name": "OPDS, with the Page Streaming Extension",
                "version": crate::opds::VERSION,
                "description": "The catalog that reading apps browse and stream pages from.",
            },
            {
                "name": "HTTP API",
                "version": null,
                "description": "The /api routes this web app uses to talk to the server.",
            },
            {
                "name": "wasm module",
                "version": null,
                "description": "taiyaku’s core built for the browser, which the reader uses to match pages between editions.",
            },
        ],
    })
}

#[cfg(test)]
mod tests;
