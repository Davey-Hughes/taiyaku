//! `taiyaku serve` — the sidecar, its pages, and where you were, over HTTP.
//!
//! Everything above `PageSource` is indifferent to where pages come from, which
//! is what lets taiyaku run beside Stump now and replace it later without a
//! redesign. See `docs/superpowers/specs/2026-07-30-reader-design.md`.

mod composite;
#[cfg(test)]
mod corpus;
mod cover;
mod opds;
mod progress;
mod reqlog;
mod routes;
mod source;
mod stump;
mod works;

use std::sync::Arc;

pub use composite::{CompositeSource, StumpSide};
pub use progress::Progress;
pub use reqlog::{Entry, Log};
pub use routes::build_router;
pub use source::{LocalLibrary, PageSource, ServedPage};
pub use works::{Work, load_works, work_id};

/// Everything a request needs: which works exist, and where pages come from.
#[derive(Debug)]
pub struct ServeState {
    pub works: Vec<Work>,
    pub source: Arc<dyn PageSource>,
    /// Where generated covers are kept between runs.
    pub cache_dir: std::path::PathBuf,
    /// Set when `--log-requests` was given; otherwise nothing is recorded.
    pub log: Option<Arc<crate::reqlog::Log>>,
}

impl ServeState {
    pub fn work(&self, id: &str) -> Option<&Work> {
        self.works.iter().find(|w| w.id == id)
    }

    /// The work a route names, or the message to send back when it does not
    /// exist.
    ///
    /// Four call sites — the `/api/…` page route, the OPDS work feed, its
    /// page stream and its cover — each typed their own copy of `no work
    /// `{id}`` before this existed, which is exactly the drift the doc
    /// comment on `routes.rs`'s own `work()` helper already warned about:
    /// one place decides what a request for a missing work gets back, so
    /// four routes cannot drift into four different answers to the same
    /// question.
    pub fn work_or_missing(&self, id: &str) -> Result<&Work, String> {
        self.work(id).ok_or_else(|| format!("no work `{id}`"))
    }

    /// One page of an edition, by its index in that edition's flat sequence.
    ///
    /// This is where a flat index becomes a member and an offset, once, for
    /// every source and every route — the API, the OPDS stream and the cover
    /// all arrive here. The failure messages name what was missing, because a
    /// reader shown a blank page with no explanation is the failure this
    /// project exists to avoid.
    pub fn page(&self, work: &Work, edition: &str, index: u32) -> Result<ServedPage, String> {
        let found = work
            .sidecar
            .editions
            .get(edition)
            .ok_or_else(|| format!("no edition `{edition}` in {}", work.path.display()))?;
        let (position, within) = found.locate(index).ok_or_else(|| {
            format!(
                "`{edition}` has {} pages; there is no page {index}",
                found.total_pages()
            )
        })?;
        let root = work.path.parent().unwrap_or(std::path::Path::new("."));
        self.source
            .page(root, &found.members[position], within)
            .map_err(|e| format!("{e} ({})", work.path.display()))
    }
}

/// How to reach a Stump instance, if one is configured.
#[derive(Debug, Clone)]
pub struct StumpConfig {
    pub base: String,
    pub key: String,
}

impl StumpConfig {
    /// `base` is normalised once, here — a trailing slash trimmed — rather
    /// than leaving each consumer to do it separately.
    ///
    /// Before this existed, `UreqTransport::new` trimmed its own copy of
    /// `base` for the transport (`stump/http.rs`), while `resolve_work`'s
    /// cache key (`lib.rs`'s call into `resolve::resolve_work`) used whatever
    /// was typed. `https://nas:10801/` and `https://nas:10801` were
    /// therefore one instance to every request and two to the resolution
    /// cache — so typing the slash once silently discarded every prior
    /// resolution, forever, with nothing to say so.
    pub fn new(base: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_string(),
            key: key.into(),
        }
    }
}

/// Bind and serve until interrupted.
///
/// Separate from `build_router` so every test drives the router directly and
/// none of them opens a socket.
pub async fn serve(
    works: Vec<Work>,
    addr: &str,
    stump: Option<StumpConfig>,
    log_requests: Option<std::path::PathBuf>,
) -> Result<(), String> {
    // Opened before anything else so a bad `--log-requests` path is a
    // startup failure that can be acted on, rather than an hour of reading
    // on an iPad producing a silently empty file.
    let log = match log_requests {
        Some(path) => Some(Arc::new(reqlog::Log::new(Some(path))?)),
        None => None,
    };

    let source: Arc<dyn PageSource> = match stump {
        None => Arc::new(CompositeSource::new(LocalLibrary, None)),
        Some(config) => {
            let transport: Arc<dyn stump::http::Transport> = Arc::new(
                stump::http::UreqTransport::new(config.base.clone(), config.key),
            );
            // Resolution happens once, at load, so no page turn costs a
            // GraphQL round trip. It is also where a misconfigured instance
            // announces itself, rather than on the first page a reader opens.
            let resolutions = resolve_all_works(transport.as_ref(), &works, &config.base);
            Arc::new(CompositeSource::new(
                LocalLibrary,
                Some(StumpSide::new(
                    transport,
                    resolutions,
                    composite::DOWN_COOLDOWN,
                )),
            ))
        }
    };

    let state = Arc::new(ServeState {
        works,
        source,
        cache_dir: std::path::PathBuf::from(".taiyaku-cache/covers"),
        log,
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

/// Resolve every work against Stump, in order, stopping as soon as one work
/// reports a transport-level failure.
///
/// Important 3 (whole-branch review): **the breaker spans works, not just
/// one work's members.** `resolve_work` already stops asking after the first
/// transport-level failure within a work; without hoisting it here too, an
/// unreachable instance would still pay one 30-second POST timeout *per
/// work* before the listener binds — twenty works is ten minutes, with
/// nothing printed until each one finishes. Once any work reports
/// `aborted_by_transport`, every later work is skipped without being asked
/// at all, and that is said once rather than once per skipped work.
///
/// Separate from `serve` so this loop can be driven by a `FakeTransport`
/// without a socket — `serve`'s only remaining job around this call is
/// turning the aggregated `Resolutions` into a `CompositeSource`.
fn resolve_all_works(
    transport: &dyn stump::http::Transport,
    works: &[Work],
    base: &str,
) -> stump::cache::Resolutions {
    let mut resolutions = stump::cache::Resolutions::new();
    for (index, work) in works.iter().enumerate() {
        let resolved = stump::resolve::resolve_work(transport, work, base);
        let known = resolved
            .resolutions
            .values()
            .filter(|v| v.is_some())
            .count();
        // The true member count, not `resolved.resolutions.len()`: the
        // circuit breaker in `resolve_work` can abandon a run partway
        // through, leaving members entirely absent from the map. A wholly
        // unreachable Stump would otherwise print "0 of 0 members from
        // stump", which reads as a work with no members rather than a run
        // that gave up.
        let total: usize = work
            .sidecar
            .editions
            .values()
            .map(|e| e.members.len())
            .sum();
        eprintln!("{}: {known} of {total} members from stump", work.id);
        let aborted = resolved.aborted_by_transport;
        resolutions.extend(resolved.resolutions);
        if aborted {
            let remaining = works.len() - index - 1;
            if remaining > 0 {
                eprintln!(
                    "taiyaku: stump appears unreachable — skipping stump resolution for the \
                     remaining {remaining} work(s); they will read from disk until the next \
                     start"
                );
            }
            break;
        }
    }
    resolutions
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
            cache_dir: std::env::temp_dir().join(dir).join("covers"),
            log: None,
        }
    }

    /// The fixture work, but with its `"raw"` edition renamed to a
    /// caller-chosen key — for tests about the key itself, such as OPDS
    /// percent-encoding one that is not URL-safe. The archives on disk are
    /// unchanged; only the in-memory sidecar's edition map is renamed, which
    /// is all a router test reads.
    pub(crate) fn work_with_edition_key(dir: &str, key: &str) -> Work {
        let mut renamed = work(dir);
        let edition = renamed
            .sidecar
            .editions
            .remove("raw")
            .expect("fixture defines the `raw` edition");
        renamed.sidecar.editions.insert(key.to_string(), edition);
        renamed.sidecar.hub = key.to_string();
        renamed
    }

    pub(crate) fn cleanup(dir: &str) {
        let root = std::env::temp_dir().join(dir);
        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod tests {
    use super::StumpConfig;

    /// Minor 4 (whole-branch review): `UreqTransport::new` already trims a
    /// trailing slash for the transport; the resolution cache key has to
    /// agree, or `https://nas:10801/` and `https://nas:10801` are one
    /// instance to every request and two to the cache — silently discarding
    /// every prior resolution the first time someone happens to type the
    /// slash.
    #[test]
    fn a_trailing_slash_is_trimmed_once_at_construction() {
        let with_slash = StumpConfig::new("https://nas:10801/", "key");
        let without = StumpConfig::new("https://nas:10801", "key");

        assert_eq!(with_slash.base, without.base);
        assert_eq!(with_slash.base, "https://nas:10801");
    }

    /// A one-member work whose archive name is distinct from every other
    /// fixture's, so a `FakeTransport`'s recorded request bodies can tell
    /// which work triggered them — `crate::test_support::work` reuses
    /// `c1.cbz`/`c2.cbz` for every caller, which would make two such works
    /// indistinguishable in a GraphQL request body.
    fn distinct_work(dir: &str, member_name: &str) -> crate::Work {
        use std::io::Write;

        let root = std::env::temp_dir().join(dir);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        let archive = format!("{member_name}.cbz");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(root.join(&archive)).unwrap());
        zip.start_file("p1.jpg", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"p1.jpg").unwrap();
        zip.finish().unwrap();

        let json = format!(
            r#"{{
              "taiyaku": 1,
              "work": {{ "title": "{member_name}", "reading": "rtl" }},
              "editions": {{
                "raw": {{ "label": "Raw", "members": [
                  {{ "path": "{archive}", "sha256": "{member_name}-sha", "pages": 1 }}
                ], "spreads": [] }}
              }},
              "hub": "raw",
              "alignments": []
            }}"#
        );
        let path = root.join("taiyaku.json");
        std::fs::write(&path, &json).unwrap();

        crate::Work {
            id: member_name.to_string(),
            path,
            sidecar: taiyaku_core::Sidecar::from_json(&json).unwrap(),
        }
    }

    /// Important 3 (whole-branch review): the circuit breaker must span
    /// works, not stop at the boundary of the first one. Against an
    /// unreachable instance, the second work's member must never be asked
    /// about at all — proven by asserting on the fake's recorded requests,
    /// not merely on the returned map.
    #[test]
    fn resolve_all_works_stops_asking_after_the_first_transport_failure() {
        use crate::stump::http::test_support::FakeTransport;

        let first = distinct_work("taiyaku-test-lib-resolve-all-first", "alpha");
        let second = distinct_work("taiyaku-test-lib-resolve-all-second", "bravo");
        // No handlers registered, so every POST is a transport-level error —
        // standing in for an unreachable instance, the convention every
        // other test in this crate uses.
        let unreachable = FakeTransport::new();

        let resolutions =
            super::resolve_all_works(&unreachable, &[first, second], "http://nas:10801");

        let requests = unreachable.requests();
        crate::test_support::cleanup("taiyaku-test-lib-resolve-all-first");
        crate::test_support::cleanup("taiyaku-test-lib-resolve-all-second");
        assert!(
            resolutions.is_empty(),
            "nothing was resolved: {resolutions:?}"
        );
        assert_eq!(
            requests.len(),
            1,
            "only the first work's first (failing) member should be attempted, not one \
             lookup per work: {requests:?}"
        );
        assert!(
            !requests.iter().any(|r| r.contains("bravo")),
            "the second work must never be asked about once the first aborted by \
             transport failure: {requests:?}"
        );
    }
}
