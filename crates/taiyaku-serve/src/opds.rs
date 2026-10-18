//! OPDS 1.2 with the Page Streaming Extension.
//!
//! Stump already serves OPDS-PSE, so this feed is not here to let a reader
//! fetch pages — it is here for the one thing neither Stump nor Komga can
//! express: **several editions grouped under one work**. Measured on the
//! development instance, `Go-Toubun no Hanayome` and its Full Color Edition
//! are two unrelated series of 14 books each; with the raws and the fan
//! chapters one work would occupy four, with nothing relating them.
//!
//! XML is written by hand rather than through a crate. The element set is
//! small and fixed, and the only hard part is escaping — which has its own
//! test, because this corpus holds titles with `&` and `♡` in them.

// **Imported rather than written `std::collections::` inline, and that is
// load-bearing for the mutation harness — O88.** `cargo-mutants` replaces a
// function body with a literal it generates and emits the SHORT type name, so
// a function *returning* a fully-qualified collection has whole-body mutants
// that fail to compile and are reported `unviable` — never run, and invisible
// in the survivor list, because an unviable mutant is not a survivor.
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;

use axum::{
    extract::{Path as AxPath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use taiyaku_library::account::Access;

use crate::ServeState;

const ATOM: &str = "application/atom+xml;profile=opds-catalog";
const PSE_NS: &str = "http://vaemendis.net/opds-pse/ns";
const SIDECAR_REL: &str = "https://taiyaku.dev/rel/sidecar";

/// RFC 4287 requires `atom:author` on a feed unless every entry carries its
/// own, and none of taiyaku's do — so it goes on the feed instead, the same
/// as `catalog`'s already does.
const AUTHOR: &str = "<author><name>taiyaku</name></author>";

/// The five characters XML cannot carry literally.
///
/// Everything else passes through unchanged: the document is UTF-8 and this
/// library's titles are full of characters that are perfectly legal and would
/// be mangled by a stricter hand.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// Percent-encode `text` for use inside a URL path segment.
///
/// A different layer from `escape`: `escape` protects XML markup and belongs
/// on *displayed* text; this protects a URL path segment and belongs on
/// text that becomes part of an `href`. An edition key can be both — shown
/// in an `<id>` and addressed in a `pse:stream` link — and each use needs
/// its own layer; using this one for the display text would show a reader
/// `raw%20color` instead of `raw color`.
///
/// Everything in the RFC 3986 "unreserved" set (`A-Za-z0-9-._~`) passes
/// through unchanged; everything else becomes `%XX`, uppercase hex, encoded
/// byte-by-byte over the UTF-8 representation so a non-ASCII key encodes the
/// same way a browser's `encodeURIComponent` would. Written by hand rather
/// than added as a dependency, matching this module's existing choice for
/// `escape` and the crate's short dependency list.
pub fn percent_encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    out
}

fn feed(body: String) -> Response {
    (
        [(header::CONTENT_TYPE, format!("{ATOM};kind=navigation"))],
        body,
    )
        .into_response()
}

fn missing(message: String) -> Response {
    (StatusCode::NOT_FOUND, message).into_response()
}

async fn catalog() -> Response {
    feed(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>taiyaku:catalog</id>
  <title>taiyaku</title>
  <updated>1970-01-01T00:00:00Z</updated>
  <author><name>taiyaku</name></author>
  <link rel="self"  type="{ATOM};kind=navigation" href="/opds/v1.2/catalog"/>
  <link rel="start" type="{ATOM};kind=navigation" href="/opds/v1.2/catalog"/>
  <entry>
    <id>taiyaku:works</id>
    <title>Works</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">Every work this server holds, with its editions</content>
    <link rel="subsection" type="{ATOM};kind=navigation" href="/opds/v1.2/works"/>
  </entry>
  <entry>
    <id>taiyaku:libraries</id>
    <title>Libraries</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">Each library, with the works under its folders</content>
    <link rel="subsection" type="{ATOM};kind=navigation" href="/opds/v1.2/libraries"/>
  </entry>
  <entry>
    <id>taiyaku:libraries:by-work</id>
    <title>Libraries (by work)</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">Each library, with the works filed in it</content>
    <link rel="subsection" type="{ATOM};kind=navigation" href="/opds/v1.2/libraries/by-work"/>
  </entry>
</feed>
"#
    ))
}

/// The access of the request, which sees nothing where no principal was put
/// on it ([`crate::auth::access_of`]).
fn granted(extensions: &axum::http::Extensions) -> Access {
    crate::auth::access_of(extensions)
}

/// Which of the two library views a feed belongs to.
#[derive(Clone, Copy)]
enum View {
    /// A library's works are those with an edition under its folders.
    Folders,
    /// A library's works are those `work.library_id` files in it.
    Filed,
}

/// The navigation feed of `libraries`, each entry linking to that library's
/// works in `view`. A library's name is escaped wherever it is written.
fn libraries_body(libraries: &[taiyaku_library::browse::LibraryEntry], view: View) -> String {
    // The feed's id, its title, the words before a library's name in an
    // entry, and what follows the library's own id and link in that view.
    let (feed_id, title, content, suffix, href_suffix) = match view {
        View::Folders => ("taiyaku:libraries", "Libraries", "The works in", "", ""),
        View::Filed => (
            "taiyaku:libraries:by-work",
            "Libraries (by work)",
            "The works filed in",
            ":by-work",
            "/by-work",
        ),
    };
    let mut entries = String::new();
    for library in libraries {
        let id = library.id;
        let name = escape(&library.name);
        let _ = writeln!(
            entries,
            r#"  <entry>
    <id>taiyaku:library:{id}{suffix}</id>
    <title>{name}</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">{content} {name}</content>
    <link rel="subsection" type="{ATOM};kind=navigation" href="/opds/v1.2/libraries/{id}/works{href_suffix}"/>
  </entry>"#
        );
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>{feed_id}</id>
  <title>{title}</title>
  <updated>1970-01-01T00:00:00Z</updated>
  {AUTHOR}
  <link rel="self"  type="{ATOM};kind=navigation" href="/opds/v1.2/libraries{href_suffix}"/>
  <link rel="start" type="{ATOM};kind=navigation" href="/opds/v1.2/catalog"/>
{entries}</feed>
"#
    )
}

/// Every library this caller may read, each linking to its works.
pub(crate) async fn libraries(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
) -> Response {
    libraries_in(state, &extensions, View::Folders).await
}

/// Every library this caller may read, each linking to the works filed in it.
pub(crate) async fn libraries_by_work(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
) -> Response {
    libraries_in(state, &extensions, View::Filed).await
}

/// The feed of libraries both views write, asked of `browse::status` on the
/// blocking pool.
async fn libraries_in(
    state: Arc<ServeState>,
    extensions: &axum::http::Extensions,
    view: View,
) -> Response {
    let access = granted(extensions);
    crate::library_api::on_the_library(state, "the libraries", move |conn| {
        match taiyaku_library::browse::status(conn, &access) {
            Ok(status) => feed(libraries_body(&status.libraries, view)),
            Err(why) => crate::library_api::failed("the libraries", &why),
        }
    })
    .await
}

/// One library's works, as `works` lists every work: the works with an
/// edition under the library's folders, each counted by the editions there.
///
/// A library outside the grant answers 404 in the words one that does not
/// exist does; `library_scope` answers `None` for both.
pub(crate) async fn library_works(
    State(state): State<Arc<ServeState>>,
    AxPath(library): AxPath<i64>,
    extensions: axum::http::Extensions,
) -> Response {
    let access = granted(&extensions);
    let feed_state = Arc::clone(&state);
    crate::library_api::on_the_library(state, "the library's works", move |conn| {
        let scope = match taiyaku_library::browse::library_scope(conn, &access, library) {
            Ok(Some(scope)) => scope,
            Ok(None) => return missing(format!("no library {library}")),
            Err(why) => return crate::library_api::failed("the library's works", &why),
        };
        match taiyaku_library::browse::visible_editions(conn, &Access::Only(scope.folders)) {
            Ok(visible) => works_feed(
                &feed_state,
                Some(&visible),
                &format!("taiyaku:library:{library}:works"),
                &escape(&scope.name),
                &format!("/opds/v1.2/libraries/{library}/works"),
            ),
            Err(why) => crate::library_api::failed("the library's works", &why),
        }
    })
    .await
}

/// One library's works as the store files them: `work.library_id`, with
/// every edition the grant allows, wherever its files are.
///
/// `visible_editions` with the caller's own access answers every edition of
/// every work for `Access::Every`, which is what an unrestricted caller is
/// owed here. A library outside the grant answers 404 as [`library_works`]
/// does.
pub(crate) async fn library_works_by_work(
    State(state): State<Arc<ServeState>>,
    AxPath(library): AxPath<i64>,
    extensions: axum::http::Extensions,
) -> Response {
    let access = granted(&extensions);
    let feed_state = Arc::clone(&state);
    crate::library_api::on_the_library(state, "the library's filed works", move |conn| {
        let scope = match taiyaku_library::browse::library_scope(conn, &access, library) {
            Ok(Some(scope)) => scope,
            Ok(None) => return missing(format!("no library {library}")),
            Err(why) => return crate::library_api::failed("the library's filed works", &why),
        };
        let filed = match taiyaku_library::browse::filed_in(conn, library) {
            Ok(filed) => filed,
            Err(why) => return crate::library_api::failed("the library's filed works", &why),
        };
        match taiyaku_library::browse::visible_editions(conn, &access) {
            Ok(mut visible) => {
                visible.retain(|slug, _| filed.contains(slug));
                works_feed(
                    &feed_state,
                    Some(&visible),
                    &format!("taiyaku:library:{library}:works:by-work"),
                    &format!("{} (by work)", escape(&scope.name)),
                    &format!("/opds/v1.2/libraries/{library}/works/by-work"),
                )
            }
            Err(why) => crate::library_api::failed("the library's filed works", &why),
        }
    })
    .await
}

/// The visible editions a restricted principal's grant admits, keyed by
/// work slug, or `None` for a caller whose access is `Access::Every`. `Err`
/// is the response to send instead: a 401 for a request with no principal
/// ([`crate::routes::on_the_grant`]), a 500 when the query fails.
///
/// `None` rather than "every edition of every work" so `works` can skip the
/// lookup entirely for the ordinary, unrestricted case instead of filtering
/// against a map built from every work — the same reason `on_the_grant`
/// answers `Every` before ever reaching SQL.
///
/// **Asked on the blocking pool, through [`crate::routes::on_the_grant`] —
/// O119.** This took `state.library.conn()` itself, on the async worker, so a
/// restricted reader's `works` feed waited on the library's mutex on a tokio
/// worker thread, stalling the whole worker: the failure O115 closed for the
/// eight handlers that ask through `routes`, in a ninth that asked through
/// code of its own.
#[expect(
    clippy::result_large_err,
    reason = "the `Err` carries a `Response`, for `routes::visible_editions_or_refuse`'s reason: \
              a handler's refusal path, called once per request"
)]
async fn visible_editions_by_work(
    state: &Arc<ServeState>,
    extensions: &axum::http::Extensions,
) -> Result<Option<HashMap<String, HashSet<String>>>, Response> {
    crate::routes::on_the_grant(state, extensions, Ok(None), |conn, principal| {
        taiyaku_library::browse::visible_editions(conn, &principal.access)
            .map(Some)
            .map_err(|why| (StatusCode::INTERNAL_SERVER_ERROR, why).into_response())
    })
    .await
    .and_then(|answer| answer)
}

/// Every work this caller may see, with its editions.
///
/// **A grant must not name a work it hides**, or the entry itself is the
/// leak `refuse_if_hidden` exists to close everywhere else on this server.
/// **Nor may the edition *count* say more than the entry does** — this feed
/// names no edition directly, only `{count} editions`, but a count computed
/// over `sidecar.editions` rather than the caller's visible subset still
/// discloses how many editions exist under a root the caller cannot see,
/// which is a smaller leak than a key or a path and a leak all the same.
/// Found beside the other four in this file, after they closed — a count is
/// easy to read as "not really naming anything."
///
/// **One query for the whole feed, not one per work.** A loop calling
/// `browse::edition_visible` per work here would be the same shape of N+1
/// the whole-branch review closed in `home` at 111 queries on the largest
/// series, reopened for a feed that walks every work the server holds
/// rather than one series — so `browse::visible_editions` (the same bulk
/// query `routes::list_works` already uses for the identical `/api/works`
/// question) answers every work's visible edition set in one round trip, in
/// [`visible_editions_by_work`] above, and [`works_feed`]'s loop only ever
/// does a `HashMap` lookup per work.
pub(crate) async fn works(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
) -> Response {
    let visible = match visible_editions_by_work(&state, &extensions).await {
        Ok(visible) => visible,
        Err(refusal) => return refusal,
    };
    works_feed(
        &state,
        visible.as_ref(),
        "taiyaku:works",
        "Works",
        "/opds/v1.2/works",
    )
}

/// The feed `/opds/v1.2/works` writes, over `visible` (`None`: every edition
/// of every work), under the feed id, title and self link given. `id` and
/// `title` are written as given, so a caller escapes them.
///
/// It reads the registry and each work's document, and no library, so a
/// caller already on the blocking pool with the library's connection held
/// may call it there.
fn works_feed(
    state: &ServeState,
    visible: Option<&HashMap<String, HashSet<String>>>,
    id: &str,
    title: &str,
    href: &str,
) -> Response {
    // Named apart from the `id` and `title` each entry below writes.
    let feed_id = id;
    let feed_title = title;
    let mut entries = String::new();
    for work in state.works.snapshot().iter() {
        // A work absent from `visible`'s keys has no edition inside the
        // grant at all — `visible_editions`'s own contract — so it is
        // skipped entirely rather than named with a count of zero.
        let editions = match visible {
            None => None,
            Some(map) => match map.get(&work.id) {
                Some(keys) => Some(keys),
                None => continue,
            },
        };
        let sidecar = work.sidecar();
        let title = escape(&sidecar.work.title);
        let id = escape(&work.id);
        // `work.id` is slugified by construction (see `work_id`) and needs
        // no encoding, but encoding it anyway costs nothing and does not
        // depend on a guarantee made in another module.
        let href_id = escape(&percent_encode(&work.id));
        let count = editions.map_or(sidecar.editions.len(), HashSet::len);
        let _ = writeln!(
            entries,
            r#"  <entry>
    <id>taiyaku:work:{id}</id>
    <title>{title}</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">{count} editions</content>
    <link rel="subsection" type="{ATOM};kind=acquisition" href="/opds/v1.2/works/{href_id}"/>
  </entry>"#
        );
    }
    feed(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>{feed_id}</id>
  <title>{feed_title}</title>
  <updated>1970-01-01T00:00:00Z</updated>
  {AUTHOR}
  <link rel="self"  type="{ATOM};kind=navigation" href="{href}"/>
  <link rel="start" type="{ATOM};kind=navigation" href="/opds/v1.2/catalog"/>
{entries}</feed>
"#
    ))
}

/// One entry per edition of a work, each carrying the links OPDS 1.2
/// requires.
///
/// **The acquisition link points at a download.** OPDS 1.2 requires at
/// least one `rel="http://opds-spec.org/acquisition"` link on every entry in
/// an acquisition feed, or a conformant client may render the entry as
/// unopenable. This link used to point at the edition's first page because
/// this server had no whole publication to point at, and PSE
/// (`rel="http://vaemendis.net/opds-pse/stream"`, below) was the only
/// acquisition mechanism a reader had. Spec C2 built the download and its
/// second half repointed this link at the OPDS-namespace mirror of that
/// route, so the guard's `WWW-Authenticate` prompt still fires for a client
/// following it — which is the whole reason that mirror exists.
///
/// Its `type` is what the download route will actually answer with rather
/// than a constant: [`crate::download::advertised_type`] decides, once, for
/// the feed and the route alike, because a single-member edition can be a
/// PDF and advertising it as a comic archive would be a claim the response's
/// own header then contradicts. It is asked on the blocking pool, through
/// [`advertised_types`], because for an edition of one member it asks the
/// filesystem (O120).
///
/// **A reader whose `can_download` is withheld keeps the page link.** The
/// conformance requirement above admits no third option: an entry with no
/// acquisition link is the unopenable one, and an entry naming a download
/// that answers 403 on every attempt is worse than the page that has always
/// been here. The web UI's own control is *absent* in that case rather than
/// disabled (spec C2 §8), and the two are not inconsistent — a feed has a
/// conformance floor a screen does not.
///
/// **This link does not reach Stump, deliberately.** The one it replaced did:
/// `stream_page` takes bytes from `ServeState::page` and therefore through
/// `PageSource`, including `CompositeSource`'s Stump branch.
/// `download::edition` reads the local filesystem and nothing else, which is
/// spec C2's own decision and the library store design's standing rule —
/// "the Stump page source becomes a migration path rather than a feature …
/// nothing removes it, and nothing should build on it either".
///
/// The consequence is named rather than left to be discovered: on a server
/// run with `--stump-url`, a member whose bytes live *only* on Stump has an
/// acquisition link that answers 404 where the page link would have served.
/// That is the changeover being incomplete rather than something to repair
/// here — the fix is finishing the migration, not teaching a new route to
/// depend on the thing being replaced.
///
/// It does leave one asymmetry inside a single feed, worth knowing before
/// somebody reads it as a bug: the PSE link still resolves through
/// `PageSource` and can serve from Stump, while acquisition cannot. The two
/// disagree for exactly as long as any member is Stump-only.
///
/// The PSE link is untouched. Streaming stays the in-app read path and
/// acquisition became the offline one; they are two mechanisms for two
/// different intentions and always were, and the documented deviation
/// existed only because one of them had no implementation.
pub(crate) async fn work_feed(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    extensions: axum::http::Extensions,
) -> Response {
    let work = match state.find(&id).await {
        Ok(work) => work,
        Err(message) => return missing(message),
    };

    // A grant restricts which *editions* of a work may be named here, not
    // only whether the work itself may — the same distinction
    // `refuse_if_hidden` draws for the page and cover routes below, applied
    // to a feed rather than a single fetch. `routes::visible_editions_or_refuse`
    // answers both halves in one round trip through `browse::work`: a 404 for
    // a work with no edition inside the grant (the one `get_sidecar` sends for
    // the identical case, from the same function), `Some` carrying only the
    // editions that survive otherwise — `WorkCard::editions`'s own contract.
    // It asks on the blocking pool; until O119 this handler called
    // `browse::work` itself, on the async worker. Reused rather than reimplemented,
    // and cheap for the one work this route addresses: `works_of`'s row plus
    // one `editions_by_work` call, not one query per edition — and since O85
    // that call answers the whole set rather than one work at a time.
    // **Absent means "may download"**, as `routes::refuse_if_cannot_download`
    // reads it. `can_download` is a capability, not a grant: it narrows what a
    // reader may do with what they can see, so reading it permissively widens
    // nothing. What a request with no principal sees is the grant's to
    // decide, and `visible_editions_or_refuse` below answers it 401.
    let may_download = extensions
        .get::<crate::auth::Principal>()
        .is_none_or(|principal| principal.can_download);

    let visible_editions: Option<HashSet<String>> =
        match crate::routes::visible_editions_or_refuse(&state, &extensions, &id).await {
            Ok(visible) => visible,
            Err(refusal) => return refusal,
        };

    let work_id = escape(&work.id);
    // See the note in `works`: `work.id` is already URL-safe by
    // construction, but every other `href` in this feed needs encoding, so
    // encoding this one too keeps the rule uniform rather than one route
    // relying on a guarantee made in another module.
    let work_href = escape(&percent_encode(&work.id));
    let sidecar = work.sidecar();
    let title = escape(&sidecar.work.title);
    // Only a reader who may download is sent to the blocking pool: the page
    // link the others get names no type this server has to find out.
    let types = if may_download {
        match advertised_types(&state, &id, &sidecar, visible_editions.as_ref()).await {
            Ok(types) => types,
            Err(refusal) => return refusal,
        }
    } else {
        HashMap::new()
    };

    let mut entries = String::new();
    for (key, edition) in &sidecar.editions {
        if visible_editions.as_ref().is_some_and(|v| !v.contains(key)) {
            continue;
        }
        // Two layers, two variables: `key_x` is XML-escaped display text —
        // the atom `<id>` a reader never sees rendered as a link — and
        // `key_href` is percent-encoded for the `href`s below. An edition
        // key is not sanitised on the way in (a sidecar this server did not
        // write may hold one `align` would now refuse), so both have to
        // handle whatever a key actually contains, including a space or a
        // `/`.
        let key_x = escape(key);
        let key_href = escape(&percent_encode(key));
        let label = escape(&edition.label);
        // The flat page count across every member, which is exactly what PSE
        // streams. An edition of seven chapter files is one publication of 235
        // pages here, which only works because pages are addressed by flat
        // index rather than by member and entry.
        let pages = edition.total_pages();
        // **OPDS 1.2 requires at least one acquisition link on every entry**,
        // so a reader who may not download gets the page link rather than
        // none: removing it would produce exactly the unopenable entry that
        // requirement exists to prevent, and a download link answering 403 on
        // every attempt is worse than the page that has always been here.
        //
        // The type is what the route will actually send rather than a
        // constant — a single-member PDF edition is served `application/pdf`,
        // and a feed advertising it as a comic archive would tell §7's "lie
        // told twice" a third time.
        let acquisition = if may_download {
            format!(
                r#"<link rel="http://opds-spec.org/acquisition" type="{}"
          href="/opds/v1.2/works/{work_href}/{key_href}/download"/>"#,
                // Every edition this loop names was asked about, so the
                // fallback is `advertised_type`'s own answer for a file it
                // cannot find rather than a case this reaches.
                types.get(key).copied().unwrap_or(crate::download::CBZ_TYPE)
            )
        } else {
            // **`pages/0`, not `pages/1`.** PSE counts from 0 and
            // `stream_page` converts nothing, so the `pages/1` this line
            // carried until now was the *second* page — while the test that
            // covered it called it "the edition's first page". Inherited
            // rather than introduced here, and corrected on the way past
            // because this branch rewrites the line: a one-page edition made
            // it a 404 outright.
            format!(
                r#"<link rel="http://opds-spec.org/acquisition" type="image/jpeg"
          href="/opds/v1.2/works/{work_href}/{key_href}/pages/0"/>"#
            )
        };
        let _ = writeln!(
            entries,
            r#"  <entry>
    <id>taiyaku:edition:{work_id}:{key_x}</id>
    <title>{title} — {label}</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">{pages} pages</content>
    <link rel="http://opds-spec.org/image/thumbnail" type="image/jpeg"
          href="/opds/v1.2/works/{work_href}/{key_href}/cover"/>
    {acquisition}
    <link rel="http://vaemendis.net/opds-pse/stream" type="image/jpeg"
          href="/opds/v1.2/works/{work_href}/{key_href}/pages/{{pageNumber}}"
          pse:count="{pages}"/>
  </entry>"#
        );
    }

    (
        [(header::CONTENT_TYPE, format!("{ATOM};kind=acquisition"))],
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opds="http://opds-spec.org/2010/catalog" xmlns:pse="{PSE_NS}">
  <id>taiyaku:work:{work_id}</id>
  <title>{title}</title>
  <updated>1970-01-01T00:00:00Z</updated>
  {AUTHOR}
  <link rel="self"  type="{ATOM};kind=acquisition" href="/opds/v1.2/works/{work_href}"/>
  <link rel="start" type="{ATOM};kind=navigation"  href="/opds/v1.2/catalog"/>
  <link rel="{SIDECAR_REL}" type="application/json" href="/api/works/{work_href}"/>
{entries}</feed>
"#
        ),
    )
        .into_response()
}

/// The type each edition [`work_feed`] lists will declare on its acquisition
/// link, keyed by edition, asked on the blocking pool.
///
/// **Off the async worker, because the answer is a `stat` — O120.**
/// [`crate::download::advertised_type`] asks the filesystem whether an edition
/// of one member is a file, and until O120 closed `work_feed` asked it in the
/// handler, once for each such edition. On the async worker a `stat` holds a
/// runtime worker thread for as long as the filesystem takes to answer. This
/// server's host bind-mounts its library from a FUSE share, where nothing
/// promises that is quick; it has not been timed there.
///
/// One trip for the whole feed. Only the editions `visible` admits are asked
/// about, which are the ones the feed names, and `sidecar` is the snapshot the
/// feed builds its entries from, so a type and its entry describe one
/// document.
///
/// `Err` is the refusal to send: a 404 if `id` names no work, which the handler
/// has already refused before asking, and a 500 if the task panicked.
#[expect(
    clippy::result_large_err,
    reason = "the `Err` carries a `Response`, for `routes::visible_editions_or_refuse`'s \
              reason: a routing handler's refusal path, called once per request"
)]
async fn advertised_types(
    state: &Arc<ServeState>,
    id: &str,
    sidecar: &Arc<taiyaku_core::Sidecar>,
    visible: Option<&HashSet<String>>,
) -> Result<HashMap<String, &'static str>, Response> {
    let state = Arc::clone(state);
    let id = id.to_string();
    let sidecar = Arc::clone(sidecar);
    let visible = visible.cloned();
    let typed = tokio::task::spawn_blocking(move || {
        let work = state.find_blocking(&id)?;
        Ok::<_, String>(
            sidecar
                .editions
                .iter()
                .filter(|(key, _)| visible.as_ref().is_none_or(|v| v.contains(*key)))
                .map(|(key, edition)| {
                    (
                        key.clone(),
                        crate::download::advertised_type(&work, edition),
                    )
                })
                .collect(),
        )
    })
    .await;
    match typed {
        Ok(Ok(types)) => Ok(types),
        Ok(Err(message)) => Err(missing(message)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("typing the editions panicked: {e}"),
        )
            .into_response()),
    }
}

pub(crate) async fn stream_page(
    State(state): State<Arc<ServeState>>,
    // `String` rather than `u32` — see `routes::page_index`. A malformed
    // number on the PSE stream is refused in this server's own words, and in
    // the same words the `/api/…` route uses, rather than by axum's `Path`
    // rejection.
    AxPath((id, edition, number)): AxPath<(String, String, String)>,
    headers: HeaderMap,
    extensions: axum::http::Extensions,
) -> Response {
    // Grant first (404, it does not exist for you), capability second (403,
    // you may see it and may not have it) — the order every other refusal
    // pair on this server follows. `refuse_if_hidden` is the exact check
    // `get_page` sends for the `/api/…` page route; reused rather than
    // reimplemented, because a second spelling of this rule is exactly what
    // spec C1's whole-branch review warned about.
    if let Some(refusal) = crate::routes::refuse_if_hidden(&state, &extensions, &id, &edition).await
    {
        return refusal;
    }
    if let Some(refusal) = crate::routes::refuse_if_cannot_stream(&extensions) {
        return refusal;
    }
    let number = match crate::routes::page_index(&number, &edition) {
        Ok(number) => number,
        Err(refusal) => return refusal.into_response(),
    };
    // **PSE counts from 0**, so this is taiyaku's own index unchanged. There
    // is no conversion here and there must not be one.
    //
    // This shipped as 1-based and was wrong. Measured against Panels on
    // 2026-07-31: it opens a book by requesting `pages/0`, retried that up to
    // nine times per edition against the 404 this used to return, showed an
    // error screen, and only advanced once tapped — at which point every page
    // it displayed was the one before the page it believed it was on, and the
    // true first page could not be reached at all. Twenty of the session's
    // thirty-nine requests were that 404.
    //
    // Stump settles the convention rather than the spec being read twice: its
    // REST API is 1-based, and its OPDS-PSE template is
    // `…/pages/{pageNumber}?zero_based=true` — it converts *for* PSE. So a
    // server whose PSE stream is 1-based disagrees with both the client and
    // the other implementation of the same feed.
    let index = number;

    // O1: the same validator the `/api/…` page route sends, checked the same
    // way and for the same reason — Panels reaches pages through *this*
    // route, not the `/api/…` one, and O5 already recorded once what fixing
    // only the other route is worth. `crate::routes::locate_member` reads
    // nothing but the in-memory sidecar, so a match is answered before the
    // decompression or Stump round trip below ever runs.
    let found = match state.find(&id).await {
        Ok(found) => found,
        Err(e) => return missing(e),
    };
    let (sha256, offset) = match crate::routes::locate_member(&found, &edition, index) {
        Ok(pair) => pair,
        Err(e) => return missing(e),
    };
    // `None`: the OPDS stream serves the member's own bytes at full size and
    // takes no `?w=`. A reading client fetching through this feed is not the
    // browser reader and has no viewport this server knows about, so there is
    // nothing to fit to — and the tag it validates against stays the one it
    // has always been.
    let tag = crate::routes::page_etag(&sha256, offset, None);
    if crate::routes::if_none_match_hits(&headers, &tag) {
        return crate::routes::not_modified(&tag, Some(crate::routes::PAGE_CACHE_CONTROL));
    }

    let fetched = tokio::task::spawn_blocking(move || {
        let found = state.find_blocking(&id)?;
        state.page(&found, &edition, index)
    })
    .await;

    match fetched {
        Ok(Ok(page)) => (
            [
                (header::CONTENT_TYPE, page.content_type),
                (header::ETAG, tag),
                (
                    header::CACHE_CONTROL,
                    crate::routes::PAGE_CACHE_CONTROL.to_string(),
                ),
            ],
            page.bytes,
        )
            .into_response(),
        Ok(Err(e)) => missing(e),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("page read panicked: {e}"),
        )
            .into_response(),
    }
}

pub(crate) async fn cover(
    State(state): State<Arc<ServeState>>,
    AxPath((id, edition)): AxPath<(String, String)>,
    extensions: axum::http::Extensions,
) -> Response {
    // Same grant `stream_page` refuses on, and deliberately no capability
    // check beside it: `refuse_if_cannot_stream`'s own doc says covers and
    // thumbnails follow browse, not `can_stream` — an account that may not
    // read pages here must still be able to see what it could download.
    if let Some(refusal) = crate::routes::refuse_if_hidden(&state, &extensions, &id, &edition).await
    {
        return refusal;
    }
    let fetched = tokio::task::spawn_blocking(move || {
        let work = state.find_blocking(&id)?;
        let sidecar = work.sidecar();
        let found = sidecar
            .editions
            .get(&edition)
            .ok_or_else(|| format!("no edition `{edition}` in work `{id}`"))?;
        // Minor 5 (whole-branch review): keyed on whichever member
        // `Edition::locate(0)` actually resolves page 0 to, not
        // `members.first()` unconditionally. `locate` steps over a member
        // recorded with zero pages, so a first member with none would make
        // `members.first()` name a different member than the one
        // `state.page(work, &edition, 0)` below actually reads bytes from —
        // caching the generated cover under the wrong member's hash.
        let (position, _within) = found
            .locate(0)
            .ok_or_else(|| format!("`{edition}` has no pages"))?;
        let cover_member = &found.members[position];
        crate::cover::cached_or_generated(&state.cache_dir, &cover_member.sha256, || {
            state.page(&work, &edition, 0).map(|p| p.bytes)
        })
    })
    .await;

    match fetched {
        Ok(Ok(bytes)) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, crate::routes::PAGE_CACHE_CONTROL),
            ],
            bytes,
        )
            .into_response(),
        Ok(Err(e)) => missing(e),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("cover generation panicked: {e}"),
        )
            .into_response(),
    }
}

/// Every OPDS route, ready to be merged into the main router.
/// This module's paths and their handlers, as a list rather than as a built
/// `Router`.
///
/// **A list because `routes::build_router` has to be able to walk it.** The
/// guard's enumeration test asserts that every path this server answers refuses
/// an anonymous request, and it drives that off the same list `build_router`
/// folds — so a route added here arrives guarded and covered on the commit that
/// adds it, which a reviewer's checklist cannot promise. Returning a `Router`
/// hid these paths from that walk, and OPDS is the half of the surface
/// reachable from outside the house.
pub fn routes() -> Vec<(&'static str, axum::routing::MethodRouter<Arc<ServeState>>)> {
    vec![
        ("/opds/v1.2/catalog", get(catalog)),
        ("/opds/v1.2/works", get(works)),
        ("/opds/v1.2/works/{work}", get(work_feed)),
        (
            "/opds/v1.2/works/{work}/{edition}/pages/{number}",
            get(stream_page),
        ),
        ("/opds/v1.2/works/{work}/{edition}/cover", get(cover)),
        // **The same handler as `/api/works/{work}/{edition}/download`**,
        // registered a second time rather than reimplemented: `edition` binds
        // its path parameters positionally, so `{work}`/`{edition}` here are
        // the `{slug}`/`{edition}` it expects.
        //
        // It exists in this namespace because of `auth::refuse`, not for
        // symmetry. A 401 on `/opds` carries `WWW-Authenticate` and a 401 on
        // `/api` deliberately does not, so a client following an acquisition
        // link into `/api` cold would receive a bare 401 and never prompt for
        // credentials. `stream_page` above duplicates the page route for
        // exactly this reason and the download follows it.
        //
        // There is no member download here: OPDS asks for a publication, and
        // a chapter file inside one is not something a feed has an entry for.
        (
            "/opds/v1.2/works/{work}/{edition}/download",
            get(crate::download::edition),
        ),
        ("/opds/v1.2/libraries", get(libraries)),
        ("/opds/v1.2/libraries/{library}/works", get(library_works)),
        ("/opds/v1.2/libraries/by-work", get(libraries_by_work)),
        (
            "/opds/v1.2/libraries/{library}/works/by-work",
            get(library_works_by_work),
        ),
    ]
}

#[cfg(test)]
mod tests;
