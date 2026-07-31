//! `taiyaku serve` — the sidecar, its pages, and where you were, over HTTP.
//!
//! Everything above `PageSource` is indifferent to where pages come from, which
//! is what lets taiyaku run beside Stump now and replace it later without a
//! redesign. See `docs/superpowers/specs/2026-07-30-reader-design.md`.

#[cfg(test)]
mod corpus;
mod progress;
mod routes;
mod source;
mod works;

use std::sync::Arc;

pub use progress::Progress;
pub use routes::build_router;
pub use source::{LocalLibrary, PageSource, ServedPage};
pub use works::{Work, load_works, work_id};

/// Everything a request needs: which works exist, and where pages come from.
#[derive(Debug)]
pub struct ServeState {
    pub works: Vec<Work>,
    pub source: Arc<dyn PageSource>,
}

impl ServeState {
    pub fn work(&self, id: &str) -> Option<&Work> {
        self.works.iter().find(|w| w.id == id)
    }
}

/// Bind and serve until interrupted.
///
/// Separate from `build_router` so every test drives the router directly and
/// none of them opens a socket.
pub async fn serve(works: Vec<Work>, addr: &str) -> Result<(), String> {
    let state = Arc::new(ServeState {
        works,
        source: Arc::new(LocalLibrary),
    });
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("{addr}: {e}"))?;
    eprintln!("taiyaku serve listening on http://{addr}");
    // Named the same way the bind failure is. Both are reported by a process
    // that may be serving several addresses over its life and prints nothing
    // else about them; "Address already in use" without one is a message that
    // has to be guessed at.
    axum::serve(listener, build_router(state))
        .await
        .map_err(|e| format!("{addr}: {e}"))
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::io::Write;

    use crate::{ServeState, Work};

    /// A work of two cbz members, four pages, on disk under a named temp dir.
    ///
    /// Two members deliberately: the offset arithmetic that picks which member
    /// holds page N is what most of these tests are about, and one member
    /// cannot exercise it. Each entry's bytes are its own name, so a test can
    /// tell which page it got.
    ///
    /// Shared rather than local to one test module because Task 5's route
    /// tests need the same work.
    pub(crate) fn work(dir: &str) -> Work {
        let root = std::env::temp_dir().join(dir);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        for (name, entries) in [
            ("c1.cbz", ["a1.jpg", "a2.jpg"]),
            ("c2.cbz", ["b1.jpg", "b2.jpg"]),
        ] {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join(name)).unwrap());
            for entry in entries {
                zip.start_file(entry, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(entry.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }

        let json = r#"{
          "taiyaku": 1,
          "work": { "title": "T", "reading": "rtl" },
          "editions": {
            "raw": { "label": "Raw", "members": [
              { "path": "c1.cbz", "sha256": "x", "pages": 2 },
              { "path": "c2.cbz", "sha256": "y", "pages": 2 }
            ], "spreads": [] }
          },
          "hub": "raw",
          "alignments": []
        }"#;
        let path = root.join("taiyaku.json");
        std::fs::write(&path, json).unwrap();

        Work {
            id: "t".into(),
            path,
            sidecar: taiyaku_core::Sidecar::from_json(json).unwrap(),
        }
    }

    /// The fixture work, wired to a `LocalLibrary`, ready for `build_router`.
    pub(crate) fn state(dir: &str) -> ServeState {
        ServeState {
            works: vec![work(dir)],
            source: std::sync::Arc::new(crate::LocalLibrary),
        }
    }

    pub(crate) fn cleanup(dir: &str) {
        let root = std::env::temp_dir().join(dir);
        let _ = std::fs::remove_dir_all(&root);
    }
}
