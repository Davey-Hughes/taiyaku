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
pub(crate) fn build_commit(raw: Option<&'static str>) -> Option<&'static str> {
    raw.filter(|commit| {
        commit.len() == 40
            && commit
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    })
}

/// `GET /api/about`.
///
/// **Behind the guard**, in `routes::guarded_routes`: nothing here is secret,
/// but an open route needs a reason a guarded one cannot serve it
/// (`routes::open_routes`), and this has none.
///
/// The commit is read at compile time because nothing runs git during a build
/// (`.git` is not in the image's build context); the Dockerfile passes it in.
/// Cargo rebuilds this crate when the variable changes, so a build never
/// reports a stale one.
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
pub(crate) async fn about() -> Response {
    (axum::Json(serde_json::json!({
        "commit": build_commit(option_env!("TAIYAKU_COMMIT")),
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
                "version": null,
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
    })),)
        .into_response()
}

#[cfg(test)]
mod tests;
