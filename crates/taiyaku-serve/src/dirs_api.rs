//! `GET /api/library/dirs`: the directory browser behind "add a directory", §6.1
//! of `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **It answers only beneath the prefixes the server was started with**
//! (`--dirs`, `TAIYAKU_DIRS`), and names directories only: no file, no
//! size, no dot-entry. With no prefix it lists nothing, and the screen falls
//! back to a typed path.
//!
//! **A path is walked, not resolved.** The route never asks the file system
//! about a path as a whole: it checks that the path begins, by whole
//! components, with a prefix (so `/media2` is not beneath `/media`), and then
//! takes it one name at a time from that prefix, looking a name up only in a
//! directory it already knows to be inside. `..` moves up only while the
//! parent is still inside a prefix, and a symbolic link is followed one hop at
//! a time, each hop through the same walk. On a file system nobody is changing,
//! the walk never rests on ground outside the prefixes, and what is out there
//! is never touched, so it cannot be told from what is not: a path that leaves
//! and comes back, a link that points out, and one that points at nothing all
//! get the same refusal.
//!
//! **That holds only while nothing is renamed inside a prefix during the
//! walk.** Each name is checked with `symlink_metadata` and then handed to the
//! kernel again by path, and a directory is listed with `read_dir` by path. If
//! another process swaps a directory inside a prefix for a link that points out
//! between the check and the use, the kernel follows the link, and the listing
//! is of the directory outside. That takes an administrator's session, since
//! the route is administrator-only, and a second process renaming inside the
//! share at that moment; the listing goes only to that administrator, who may
//! already point a library at any path on the server. It is accepted for that
//! reason. Walking by descriptor (each name opened relative to the directory
//! already held) would close it, and needs a dependency this crate does not
//! have.
//!
//! **Two accepted costs.** A link inside a prefix that dangles or loops is
//! refused as outside, not answered as missing; and a spelling that reaches a
//! prefix by a route the walk does not recognise, `/proc/self/root` followed by
//! the prefix, is refused though it would resolve inside.
//!
//! **Every path the route answers is UTF-8**: a name that is not is neither
//! offered nor entered, by whatever way it is reached.
//!
//! **A path outside every prefix is refused once, whether or not it exists.**
//! The answer is one status and one sentence built from the path as it was
//! sent: it never says that the path exists, never names where a link leads,
//! and carries no operating-system error. Only a path that is inside a prefix
//! gets the system's words about why it cannot be listed.
//!
//! **Inside a prefix the route is not coy.** A directory is listed whether or
//! not its name starts with a dot, when it is asked for by name or through a
//! link, and a file inside a prefix is answered "Not a directory". Dot-entries
//! are left out of a listing, not made unreachable.
//!
//! **It is a way to find a path, not a way to add one.** Adding is the
//! libraries routes' work.
//!
//! **A refusal is `{"error": "<sentence>"}`**, including the ones axum would
//! answer in plain text (a query it cannot read). The exception is the
//! administrator check: a reader's refusal is the plain text every
//! administrator-only route gives, and a device credential's is the one that
//! names the API key.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Query, State, rejection::QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::{ServeState, auth::Principal};

/// The subdirectories of one path, or the prefixes themselves.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Listing {
    /// The path listed, canonical. `None` where the prefixes were listed.
    pub path: Option<PathBuf>,
    /// Where "up" goes. `None` at a prefix no other prefix holds, and where
    /// the prefixes were listed.
    pub parent: Option<PathBuf>,
    /// Each directory's name and path. A directory's entries are in byte
    /// order of their names, so a capital sorts before a lower-case letter. A
    /// prefix is named by its whole path, and the prefixes are in the order
    /// their paths sort.
    pub dirs: Vec<(String, PathBuf)>,
}

/// Why a path was not listed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// Beneath no prefix: 403.
    Outside(String),
    /// It is not absolute, does not exist, or this process cannot list it:
    /// 400.
    Unreadable(String),
}

/// Where the directories the server may browse are, once read: each as the
/// operator wrote it and in canonical form.
struct Prefixes {
    /// Every spelling a request may begin with, and the canonical prefix it
    /// stands for: the canonical form itself, and the form as written.
    spellings: Vec<(PathBuf, PathBuf)>,
    /// The canonical prefixes, by name, each once.
    canonical: Vec<PathBuf>,
}

/// The most symbolic links one walk follows. A chain, a loop or a tree of
/// links that needs more is refused as outside, so a share made to be slow or
/// endless costs a bounded number of reads.
const MOST_LINKS: usize = 40;

/// Why a walk stopped.
enum Stop {
    /// It went, or was sent, somewhere not known to be inside: the outside
    /// refusal, which says nothing of what is there.
    Outside,
    /// A name was looked up in a directory known to be inside, and that failed:
    /// the operating system's words are about a path inside.
    Inside(String),
}

impl Prefixes {
    /// Reads the prefixes once. One that cannot be canonicalised, is not a
    /// directory (an unmounted share, say), or whose path is not UTF-8 (it
    /// could be neither sent as JSON nor named in a query) is left out and is
    /// beneath nothing.
    fn read(written: &[PathBuf]) -> Self {
        let mut spellings = Vec::new();
        for raw in written {
            let Ok(canonical) = raw.canonicalize() else {
                continue;
            };
            if !canonical.is_dir() || canonical.to_str().is_none() {
                continue;
            }
            spellings.push((canonical.clone(), canonical));
            spellings.push((raw.clone(), spellings[spellings.len() - 1].1.clone()));
        }
        let mut canonical: Vec<PathBuf> = spellings.iter().map(|(_, c)| c.clone()).collect();
        canonical.sort();
        canonical.dedup();
        Self {
            spellings,
            canonical,
        }
    }

    fn within(&self, candidate: &Path) -> bool {
        self.canonical
            .iter()
            .any(|prefix| candidate.starts_with(prefix))
    }

    /// Takes `path` one name at a time, and answers where it comes to, in
    /// canonical form, and whether that is a directory. `links` is what is left
    /// of the budget of links this request may follow.
    ///
    /// Nothing here asks the file system about a place that is not already
    /// known to be inside a prefix, except the one-hop read of a link that sits
    /// in such a place.
    fn walk(&self, path: &Path, links: &std::cell::Cell<usize>) -> Result<(PathBuf, bool), Stop> {
        // 1. Before the file system is touched at all, the path must begin, by
        //    whole components, with a prefix, in its canonical form or as the
        //    operator wrote it. Of those that do, the one that leaves least of
        //    the path to walk. Anything else is outside, and nothing is asked
        //    about it.
        let (mut current, rest) = self
            .spellings
            .iter()
            .filter_map(|(spelling, canonical)| {
                path.strip_prefix(spelling)
                    .ok()
                    .map(|rest| (canonical, rest))
            })
            .min_by_key(|(_, rest)| rest.components().count())
            .map(|(canonical, rest)| (canonical.clone(), rest))
            .ok_or(Stop::Outside)?;
        // A canonical prefix is a directory.
        let mut is_dir = true;

        for component in rest.components() {
            match component {
                std::path::Component::CurDir => {}
                // 2. `..` goes to the parent only while that parent is inside a
                //    prefix, so the walk is never anywhere outside one, however
                //    the path goes out and comes back.
                std::path::Component::ParentDir => {
                    if !is_dir {
                        // `..` beneath a file is the system's own error for a
                        // file used as a directory, ENOTDIR, which is 20 on
                        // Linux, where this server runs. It is built here, not
                        // asked of the file system, so nothing is looked up
                        // for it and the words are the ones every other answer
                        // about a path inside has.
                        let why = std::io::Error::from_raw_os_error(20);
                        return Err(Stop::Inside(why.to_string()));
                    }
                    let parent = current.parent().ok_or(Stop::Outside)?;
                    if !self.within(parent) {
                        return Err(Stop::Outside);
                    }
                    current = parent.to_path_buf();
                }
                std::path::Component::Normal(name) => {
                    // Every path answered is UTF-8, and one that is not is not
                    // entered.
                    if name.to_str().is_none() {
                        return Err(Stop::Outside);
                    }
                    let candidate = current.join(name);
                    // 3. The name is looked up beneath a directory known to be
                    //    inside, without following it: a failure of that
                    //    lookup is answered in the system's words.
                    let found = std::fs::symlink_metadata(&candidate)
                        .map_err(|e| Stop::Inside(e.to_string()))?;
                    if found.is_symlink() {
                        // 4. A link is followed only if it resolves inside a
                        //    prefix, one hop at a time, each hop walked from
                        //    step 1. Any other outcome, an error included, is
                        //    outside: what a link points at is never reported.
                        (current, is_dir) = self.follow(&candidate, &current, links)?;
                    } else {
                        is_dir = found.is_dir();
                        current = candidate;
                    }
                }
                // A path with a root or a drive was stripped by step 1.
                std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                    return Err(Stop::Outside);
                }
            }
        }
        Ok((current, is_dir))
    }

    /// One hop of the link at `link`, which sits in `holder`, a directory known
    /// to be inside: the link's target, made absolute against `holder` where it
    /// is relative, walked from step 1.
    fn follow(
        &self,
        link: &Path,
        holder: &Path,
        links: &std::cell::Cell<usize>,
    ) -> Result<(PathBuf, bool), Stop> {
        // 5. A hop is spent before it is taken, so a loop ends.
        let Some(left) = links.get().checked_sub(1) else {
            return Err(Stop::Outside);
        };
        links.set(left);
        let target = std::fs::read_link(link).map_err(|_| Stop::Outside)?;
        self.walk(&holder.join(target), links)
            .map_err(|_| Stop::Outside)
    }
}

/// List `path`'s subdirectories, or the prefixes where `path` is `None`.
///
/// A prefix is read once, here, in canonical form, and is listed once however
/// often or however it was written. A relative prefix is made absolute against
/// the working directory by that, which is the operator's own word; a relative
/// `path` is refused, since a request must not be resolved against where the
/// server happens to run.
pub(crate) fn list(prefixes: &[PathBuf], path: Option<&Path>) -> Result<Listing, Refusal> {
    let prefixes = Prefixes::read(prefixes);

    let Some(path) = path else {
        return Ok(Listing {
            path: None,
            parent: None,
            dirs: prefixes
                .canonical
                .iter()
                .map(|prefix| (prefix.to_string_lossy().into_owned(), prefix.clone()))
                .collect(),
        });
    };

    if !path.is_absolute() {
        return Err(Refusal::Unreadable(format!(
            "`{}` is not an absolute path",
            path.display()
        )));
    }
    let (canonical, _) = prefixes
        .walk(path, &std::cell::Cell::new(MOST_LINKS))
        .map_err(|stop| match stop {
            // Said of the path as it was sent and of nothing the file system
            // holds, so it is the same however the path came to be outside.
            Stop::Outside => Refusal::Outside(format!(
                "{} is not inside a directory this server may browse",
                path.display()
            )),
            Stop::Inside(why) => Refusal::Unreadable(format!("{}: {why}", path.display())),
        })?;
    let entries = std::fs::read_dir(&canonical).map_err(|e| {
        Refusal::Unreadable(format!("{}: cannot be read: {e}", canonical.display()))
    })?;

    let mut dirs = Vec::new();
    for entry in entries {
        // An error from the iterator itself means the listing so far is not the
        // directory, and a short list must not be presented as a whole one.
        let entry = entry.map_err(|e| {
            Refusal::Unreadable(format!("{}: cannot be read: {e}", canonical.display()))
        })?;
        // A name that is not UTF-8 cannot be sent as JSON without being
        // changed into a path that is not the directory's.
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        // An entry whose type cannot be read is left out, and the rest are
        // still listed: one unreadable entry must not hide its neighbours.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let child = entry.path();
        // A link is offered only if the walk would enter it and it comes to a
        // directory, so what is offered and what can be entered cannot
        // disagree. One it cannot follow, for any reason, is left out like a
        // dangling one.
        let is_dir = if kind.is_symlink() {
            prefixes
                .follow(&child, &canonical, &std::cell::Cell::new(MOST_LINKS))
                .is_ok_and(|(_, is_dir)| is_dir)
        } else {
            kind.is_dir()
        };
        if is_dir {
            dirs.push((name, child));
        }
    }
    dirs.sort();

    // Up goes to the directory above while that directory is inside a prefix: from
    // a prefix that sits inside another, to the outer one's directory, and from
    // any other prefix nowhere.
    let parent = canonical
        .parent()
        .filter(|parent| prefixes.within(parent))
        .map(Path::to_path_buf);
    Ok(Listing {
        path: Some(canonical),
        parent,
        dirs,
    })
}

/// What `serve` prints at startup for each directory that lists nothing, so a
/// deployer whose share is not mapped sees why the browser is empty. It names
/// the directory and not the flag, which may have come from `TAIYAKU_DIRS`.
pub(crate) fn notices(prefixes: &[PathBuf]) -> Vec<String> {
    prefixes
        .iter()
        .filter(|prefix| {
            !prefix
                .canonicalize()
                .is_ok_and(|p| p.is_dir() && p.to_str().is_some())
        })
        .map(|prefix| {
            format!(
                "directory browser: {} is not a directory here, or its name is not UTF-8, so it lists nothing",
                prefix.display()
            )
        })
        .collect()
}

/// The one query parameter [`list_dirs`] reads. `deny_unknown_fields`, so
/// a misspelt `path` is refused and not answered as a list of the prefixes.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DirsQuery {
    #[serde(default)]
    path: Option<String>,
}

/// `GET /api/library/dirs[?path=…]`. The read runs on the blocking pool: a
/// sleeping disk can take seconds to list a directory.
pub(crate) async fn list_dirs(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    query: Result<Query<DirsQuery>, QueryRejection>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let refused = |status: StatusCode, why: String| {
        (status, Json(serde_json::json!({ "error": why }))).into_response()
    };
    let query = match query {
        Ok(Query(query)) => query,
        Err(rejection) => {
            return refused(
                StatusCode::BAD_REQUEST,
                format!("the query could not be read: {}", rejection.body_text()),
            );
        }
    };

    let listed = tokio::task::spawn_blocking(move || {
        list(&state.dirs, query.path.as_deref().map(Path::new))
    })
    .await;

    match listed {
        Ok(Ok(listing)) => Json(serde_json::json!({
            "path": listing.path.map(|p| p.to_string_lossy().into_owned()),
            "parent": listing.parent.map(|p| p.to_string_lossy().into_owned()),
            "dirs": listing
                .dirs
                .iter()
                .map(|(name, path)| serde_json::json!({
                    "name": name,
                    "path": path.to_string_lossy(),
                }))
                .collect::<Vec<_>>(),
        }))
        .into_response(),
        Ok(Err(Refusal::Outside(why))) => refused(StatusCode::FORBIDDEN, why),
        Ok(Err(Refusal::Unreadable(why))) => refused(StatusCode::BAD_REQUEST, why),
        Err(why) => refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("listing directories panicked: {why}"),
        ),
    }
}

#[cfg(test)]
mod tests;
