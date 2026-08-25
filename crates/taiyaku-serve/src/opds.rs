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

use std::fmt::Write as _;
use std::sync::Arc;

use axum::{
    extract::{Path as AxPath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

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
</feed>
"#
    ))
}

/// The visible editions a restricted principal's grant admits, keyed by
/// work slug, or `None` for an unrestricted caller (no `Principal` at all —
/// this file's own tests build requests with none — or an empty grant,
/// which spec C1 reads as "every root").
///
/// `None` rather than "every edition of every work" so `works` can skip the
/// lookup entirely for the ordinary, unrestricted case instead of filtering
/// against a map built from every work — the same reason `refuse_if_hidden`
/// short-circuits on `roots.is_empty()` before ever reaching SQL.
fn visible_editions_by_work(
    state: &ServeState,
    extensions: &axum::http::Extensions,
) -> Result<Option<std::collections::HashMap<String, std::collections::HashSet<String>>>, String> {
    let Some(principal) = extensions.get::<crate::auth::Principal>() else {
        return Ok(None);
    };
    if principal.roots.is_empty() {
        return Ok(None);
    }
    let conn = state.library.conn();
    taiyaku_library::browse::visible_editions(&conn, &principal.roots).map(Some)
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
/// [`visible_editions_by_work`] above, and this loop only ever does a
/// `HashMap` lookup per work.
async fn works(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
) -> Response {
    let visible = match visible_editions_by_work(&state, &extensions) {
        Ok(visible) => visible,
        Err(why) => return (StatusCode::INTERNAL_SERVER_ERROR, why).into_response(),
    };
    let mut entries = String::new();
    for work in &state.works {
        // A work absent from `visible`'s keys has no edition inside the
        // grant at all — `visible_editions`'s own contract — so it is
        // skipped entirely rather than named with a count of zero.
        let editions = match &visible {
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
        let count = editions.map_or(sidecar.editions.len(), std::collections::HashSet::len);
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
  <id>taiyaku:works</id>
  <title>Works</title>
  <updated>1970-01-01T00:00:00Z</updated>
  {AUTHOR}
  <link rel="self"  type="{ATOM};kind=navigation" href="/opds/v1.2/works"/>
  <link rel="start" type="{ATOM};kind=navigation" href="/opds/v1.2/catalog"/>
{entries}</feed>
"#
    ))
}

/// One entry per edition of a work, each carrying the links OPDS 1.2
/// requires.
///
/// **The acquisition link still points at the edition's first page, not at
/// a download — a known remaining step, not a considered end state.** OPDS
/// 1.2 requires at least one `rel="http://opds-spec.org/acquisition"` link
/// on every entry in an acquisition feed, or a conformant client may render
/// the entry as unopenable, which is what this link has always guarded
/// against. That reasoning used to end there — this server had no download
/// a client could be pointed at, and PSE
/// (`rel="http://vaemendis.net/opds-pse/stream"`, below) was the only
/// acquisition mechanism a reader had. It no longer holds: spec C2 shipped
/// `GET /api/works/{work}/{edition}/download` (and a per-member route
/// beside it), so a whole publication a client could download now exists.
/// Repointing this link at it — spec C2 §9's own next step, an
/// OPDS-namespace mirror of that route so the guard's `WWW-Authenticate`
/// prompt still fires for an OPDS client — was deferred to a second plan
/// alongside §8's web UI control (`docs/status.md`'s `taiyaku-serve` entry
/// names both), not built on this branch. Until that lands, the link keeps
/// pointing at the first page — a real, fetchable image, not a dead link —
/// so a conformant client still sees a satisfiable entry rather than an
/// empty one; today, only a direct call to the `/api` route above reaches
/// the download — neither this feed nor the web UI (§8, also deferred)
/// offers it yet.
async fn work_feed(
    State(state): State<Arc<ServeState>>,
    AxPath(id): AxPath<String>,
    extensions: axum::http::Extensions,
) -> Response {
    let work = match state.work_or_missing(&id) {
        Ok(work) => work,
        Err(message) => return missing(message),
    };

    // A grant restricts which *editions* of a work may be named here, not
    // only whether the work itself may — the same distinction
    // `refuse_if_hidden` draws for the page and cover routes below, applied
    // to a feed rather than a single fetch. `browse::work` already answers
    // both halves in one round trip: `None` for a work with no edition
    // inside the grant (the same 404 `get_sidecar` sends for the identical
    // case), `Some` carrying only the editions that survive otherwise —
    // `WorkCard::editions`'s own contract. Reused rather than reimplemented,
    // and cheap for the one work this route addresses: `works_of`'s row plus
    // one `editions_of` call, not one query per edition.
    let visible_editions: Option<std::collections::HashSet<String>> = match extensions
        .get::<crate::auth::Principal>(
    ) {
        Some(principal) if !principal.roots.is_empty() => {
            let detail = {
                let conn = state.library.conn();
                taiyaku_library::browse::work(&conn, &principal.roots, &id)
            };
            match detail {
                Ok(None) => return missing(format!("no work `{id}`")),
                Err(why) => return (StatusCode::INTERNAL_SERVER_ERROR, why).into_response(),
                Ok(Some(detail)) => Some(detail.card.editions.into_iter().map(|e| e.key).collect()),
            }
        }
        _ => None,
    };

    let work_id = escape(&work.id);
    // See the note in `works`: `work.id` is already URL-safe by
    // construction, but every other `href` in this feed needs encoding, so
    // encoding this one too keeps the rule uniform rather than one route
    // relying on a guarantee made in another module.
    let work_href = escape(&percent_encode(&work.id));
    let sidecar = work.sidecar();
    let title = escape(&sidecar.work.title);

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
        let _ = writeln!(
            entries,
            r#"  <entry>
    <id>taiyaku:edition:{work_id}:{key_x}</id>
    <title>{title} — {label}</title>
    <updated>1970-01-01T00:00:00Z</updated>
    <content type="text">{pages} pages</content>
    <link rel="http://opds-spec.org/image/thumbnail" type="image/jpeg"
          href="/opds/v1.2/works/{work_href}/{key_href}/cover"/>
    <link rel="http://opds-spec.org/acquisition" type="image/jpeg"
          href="/opds/v1.2/works/{work_href}/{key_href}/pages/1"/>
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

async fn stream_page(
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
    if let Some(refusal) = crate::routes::refuse_if_hidden(&state, &extensions, &id, &edition) {
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
    let found = match state.work_or_missing(&id) {
        Ok(found) => found,
        Err(e) => return missing(e),
    };
    let (sha256, offset) = match crate::routes::locate_member(found, &edition, index) {
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
        let found = state.work_or_missing(&id)?;
        state.page(found, &edition, index)
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

async fn cover(
    State(state): State<Arc<ServeState>>,
    AxPath((id, edition)): AxPath<(String, String)>,
    extensions: axum::http::Extensions,
) -> Response {
    // Same grant `stream_page` refuses on, and deliberately no capability
    // check beside it: `refuse_if_cannot_stream`'s own doc says covers and
    // thumbnails follow browse, not `can_stream` — an account that may not
    // read pages here must still be able to see what it could download.
    if let Some(refusal) = crate::routes::refuse_if_hidden(&state, &extensions, &id, &edition) {
        return refusal;
    }
    let fetched = tokio::task::spawn_blocking(move || {
        let work = state.work_or_missing(&id)?;
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
            state.page(work, &edition, 0).map(|p| p.bytes)
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
/// hid these five paths from that walk, and OPDS is the half of the surface
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
    ]
}

#[cfg(test)]
mod tests;
