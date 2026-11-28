//! An account's view of a work: the one shared sidecar, cut to what the
//! account's grant reaches (spec §3.8 of
//! `docs/superpowers/specs/2026-11-02-overlapping-libraries-design.md`).
//!
//! Built per request from the in-memory sidecar every account shares, because
//! what an account sees depends on its grant: an edition with no visible
//! member is hidden, one with some is narrowed to them, and one with every
//! member visible is sent whole. `Access::Every` pays no query and sees the
//! sidecar as it is.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use taiyaku_core::{Edition, FromFull, Member, MemberPage, Narrowed, Sidecar, SlotOf};
use taiyaku_library::account::Access;
use taiyaku_library::browse::SeenMember;

use crate::routes::{filter_sidecar_editions, not_found, on_the_grant, work};
use crate::{ServeState, Work};

/// The work as one account is sent it, and the narrowed editions in it.
#[derive(Debug)]
pub(crate) struct View {
    sidecar: Arc<Sidecar>,
    /// The sidecar the flags were checked against, which the work's own can
    /// have been replaced since.
    full: Arc<Sidecar>,
    narrowed: HashMap<String, Narrowed>,
    /// The keys of the editions sent; `None` for `Access::Every`.
    visible: Option<HashSet<String>>,
}

impl View {
    /// `sidecar` as an account sees it: `flags` is `None` for `Access::Every`,
    /// which sees it as it is, and otherwise
    /// [`taiyaku_library::browse::visible_members`]'s answer. `None` when no
    /// edition is visible. An edition with no flags, or flags not read of its
    /// members in its order, is hidden. Alignments touching a hidden or
    /// narrowed edition are left out, their runs being in the full numbering,
    /// and `hub` moves as [`filter_sidecar_editions`] moves it.
    pub(crate) fn build(
        sidecar: Arc<Sidecar>,
        flags: Option<&HashMap<String, Vec<SeenMember>>>,
    ) -> Option<View> {
        let mut view = View {
            full: Arc::clone(&sidecar),
            sidecar,
            narrowed: HashMap::new(),
            visible: None,
        };
        let Some(flags) = flags else {
            return Some(view);
        };
        let mut visible = HashSet::new();
        for (key, edition) in &view.sidecar.editions {
            match flags.get(key).and_then(|seen| Seen::of(edition, seen)) {
                Some(Seen::Whole) => {
                    visible.insert(key.clone());
                }
                Some(Seen::Narrowed(narrowed)) => {
                    visible.insert(key.clone());
                    view.narrowed.insert(key.clone(), narrowed);
                }
                None => {}
            }
        }
        if visible.is_empty() {
            return None;
        }
        if view.narrowed.is_empty() && visible.len() == view.sidecar.editions.len() {
            view.visible = Some(visible);
            return Some(view);
        }
        let mut seen = filter_sidecar_editions(&view.sidecar, &visible);
        view.visible = Some(visible);
        for (key, narrowed) in &view.narrowed {
            seen.editions
                .insert(key.clone(), narrowed.edition().clone());
        }
        seen.alignments
            .retain(|a| !view.narrowed.contains_key(&a.from) && !view.narrowed.contains_key(&a.to));
        view.sidecar = Arc::new(seen);
        Some(view)
    }

    /// The document this account is sent.
    pub(crate) fn sidecar(&self) -> &Arc<Sidecar> {
        &self.sidecar
    }

    /// The whole sidecar this view was built from: the numbering a stored slot
    /// is in, so a position is resolved and anchored in it (O244), not in the
    /// store's, which can have moved on since.
    pub(crate) fn full(&self) -> &Arc<Sidecar> {
        &self.full
    }

    /// `edition` narrowed, or `None` when it is sent whole or not at all.
    pub(crate) fn narrowed(&self, edition: &str) -> Option<&Narrowed> {
        self.narrowed.get(edition)
    }

    /// The keys of the editions this account is sent; `None` for
    /// `Access::Every`.
    pub(crate) fn visible(&self) -> Option<&HashSet<String>> {
        self.visible.as_ref()
    }

    /// The sidecar this view was built from, cut to the editions this
    /// account is sent, each in its full form, a narrowed one included: what
    /// a notes write is checked against, as its notes are in the stored
    /// numbering (spec §3.8). The whole sidecar for `Access::Every`.
    ///
    /// **So an edition outside the grant is refused in the words an edition
    /// the work does not have is refused in**, by the check that already
    /// refuses the second (O60), with no second question to ask.
    pub(crate) fn visible_editions_in_full(&self) -> Arc<Sidecar> {
        match &self.visible {
            Some(keys) => Arc::new(filter_sidecar_editions(&self.full, keys)),
            None => Arc::clone(&self.full),
        }
    }

    /// Whether `edition` is sent whole: every key for `Access::Every`, even
    /// one the work no longer has, and otherwise a visible edition that is
    /// not narrowed.
    pub(crate) fn sees_whole(&self, edition: &str) -> bool {
        self.visible
            .as_ref()
            .is_none_or(|keys| keys.contains(edition) && !self.narrowed.contains_key(edition))
    }

    /// Where a stored `slot` of `edition` is in the document sent: `None` for
    /// an edition not sent. `Access::Every` is told even of a key the work no
    /// longer has, and an edition sent whole keeps its slot as stored.
    pub(crate) fn slot_from_full(&self, edition: &str, slot: u32) -> Option<FromFull> {
        if self
            .visible
            .as_ref()
            .is_some_and(|keys| !keys.contains(edition))
        {
            return None;
        }
        Some(match self.narrowed(edition) {
            Some(narrowed) => narrowed.slot_from_full(slot, self.sidecar.work.reading),
            None => FromFull::Visible(slot),
        })
    }

    /// The `ETag` of a stored notes or truth document, `stored` its one
    /// serialisation, as this account is sent it (spec §3.8, decision 11).
    /// For `Access::Every` it is `stored`'s, as `notes::etag_for` takes it.
    /// Otherwise it also names the numbering: a write read under another
    /// view (a grant changed, a scan moved or resized a member) then fails
    /// `If-Match`, where it would be applied in the new numbering, moving
    /// notes and deleting those the account had not been sent.
    pub(crate) fn etag_naming_numbering(&self, stored: &str) -> String {
        use sha2::{Digest, Sha256};
        let Some(visible) = &self.visible else {
            return crate::notes::etag_for(stored);
        };
        // Each field is length-prefixed and each list carries its count, 0
        // members for an edition sent whole, so two different views or
        // documents never feed the hasher the same bytes.
        let mut hasher = Sha256::new();
        let mut field = |bytes: &[u8]| {
            hasher.update((bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        };
        let reading = self.sidecar.work.reading;
        let mut keys: Vec<&String> = visible.iter().collect();
        keys.sort();
        for key in keys {
            field(key.as_bytes());
            let Some(narrowed) = self.narrowed(key) else {
                field(&0_u64.to_le_bytes());
                continue;
            };
            let members = &narrowed.edition().members;
            field(&(members.len() as u64).to_le_bytes());
            for member in members {
                field(member.sha256.as_bytes());
                field(member.path.as_bytes());
            }
            // Where each slot sent sits in the full edition, which notes are
            // stored in: a hidden member before it moves it without changing
            // what is sent.
            let slots = u32::try_from(narrowed.edition().slots(reading).len()).unwrap_or(u32::MAX);
            field(&u64::from(slots).to_le_bytes());
            for slot in 0..slots {
                let full = narrowed.slot_to_full(slot, reading);
                field(&full.map_or(u64::MAX, u64::from).to_le_bytes());
            }
        }
        // Keyed by the `sha256` of every member the account is not sent. All
        // else hashed is what it is sent, save each sent member's stored path
        // (a non-administrator is sent only its file name, O280), and small
        // numbers, the full slots and a stored document's entries on pages it
        // does not see, which an unkeyed tag would let it enumerate to a hidden
        // member's size.
        let mut editions: Vec<(&String, &Edition)> = self.full.editions.iter().collect();
        editions.sort_by_key(|(key, _)| *key);
        for (key, edition) in editions {
            let sent: HashSet<usize> = match self.narrowed(key) {
                Some(narrowed) => (0..narrowed.edition().members.len())
                    .filter_map(|member| narrowed.member_to_full(member))
                    .collect(),
                None if visible.contains(key) => continue,
                None => HashSet::new(),
            };
            field(key.as_bytes());
            let hidden: Vec<&Member> = edition
                .members
                .iter()
                .enumerate()
                .filter(|(at, _)| !sent.contains(at))
                .map(|(_, member)| member)
                .collect();
            field(&(hidden.len() as u64).to_le_bytes());
            for member in hidden {
                field(member.sha256.as_bytes());
            }
        }
        field(stored.as_bytes());
        crate::notes::quoted_hex(&hasher.finalize())
    }

    /// A sent `slot` of `edition` as it is stored: `None` past a narrowed
    /// edition's end, and any other slot as it is.
    pub(crate) fn slot_to_full(&self, edition: &str, slot: u32) -> Option<u32> {
        match self.narrowed(edition) {
            Some(narrowed) => narrowed.slot_to_full(slot, self.sidecar.work.reading),
            None => Some(slot),
        }
    }

    /// The page sent beside sent `slot` of `edition` (O240), `stored` the
    /// one its row names and `resolves` whether that is found. A row naming
    /// none is sent none, as the page its slot shows is not one it was written
    /// with. Narrowed, a found page is the one `slot` shows as sent, its copy
    /// counted among the members sent. Any other goes as stored, so its echo
    /// matches the row (spec §6 decision 5), unless [`Self::hides`] its digest.
    pub(crate) fn anchor_sent(
        &self,
        edition: &str,
        slot: u32,
        stored: Option<&MemberPage>,
        resolves: bool,
    ) -> Option<MemberPage> {
        let stored = stored?;
        if resolves && let Some(narrowed) = self.narrowed(edition) {
            return narrowed
                .edition()
                .member_page(slot, self.sidecar.work.reading);
        }
        (!self.hides(&stored.sha256)).then(|| stored.clone())
    }

    /// Whether `sha256` is a member's this account is not sent, in any
    /// edition, and no member's it is sent: the digests its tag is keyed by
    /// ([`Self::etag_naming_numbering`]), so that the tag cannot be enumerated
    /// (spec gap 8). Never for `Access::Every`, which is sent every member.
    fn hides(&self, sha256: &str) -> bool {
        let carries = |sidecar: &Sidecar| {
            sidecar
                .editions
                .values()
                .flat_map(|edition| &edition.members)
                .any(|member| member.sha256 == sha256)
        };
        self.visible.is_some() && carries(&self.full) && !carries(&self.sidecar)
    }

    /// Where `at`, a page in this account's numbering, stands in `edition` as
    /// it is sent: the slot a write's `at` places a note or a position on
    /// (O240). `None` for an edition not sent, or a page or copy it lacks.
    pub(crate) fn slot_of_anchor(&self, edition: &str, at: &MemberPage) -> Option<SlotOf> {
        self.sidecar
            .editions
            .get(edition)?
            .slot_of(at, self.sidecar.work.reading)
    }
}

/// One edition as one account sees it.
#[expect(
    clippy::large_enum_variant,
    reason = "consumed where it is made, one edition at a time; never held"
)]
enum Seen {
    Whole,
    Narrowed(Narrowed),
}

impl Seen {
    /// `edition` as `seen` flags it, or `None` when it is hidden: no member
    /// visible, or flags not read of its members in its order.
    fn of(edition: &Edition, seen: &[SeenMember]) -> Option<Seen> {
        // The sidecar can be the registry's copy from before a scan that
        // moved a member to another ordinal; pairing by position would then
        // send a member the grant hides, narrowed or whole.
        let read_of_these = seen.len() == edition.members.len()
            && seen
                .iter()
                .zip(&edition.members)
                .all(|(seen, member)| seen.sha256 == member.sha256 && seen.path == member.path);
        if !read_of_these {
            return None;
        }
        let seen: Vec<bool> = seen.iter().map(|seen| seen.visible).collect();
        if seen.iter().all(|seen| *seen) {
            return Some(Seen::Whole);
        }
        Narrowed::new(edition, &seen).map(Seen::Narrowed)
    }
}

/// One edition of a work as a page route's account sees it: the sidecar its
/// flags were checked against, to locate the page in, and the edition
/// narrowed when it is.
///
/// Built of the one edition a page names, so a page request clones nothing
/// of the sidecar's other editions.
#[derive(Debug)]
pub(crate) struct EditionView {
    work: Arc<Work>,
    /// Never the work's current sidecar read again: a refresh replaces that
    /// in place, and one read after the check can hold a member the flags
    /// never named, which would then be served (fail closed, spec §3.8).
    sidecar: Arc<Sidecar>,
    key: String,
    narrowed: Option<Narrowed>,
}

impl EditionView {
    /// `work`'s `edition` as an account sees it, `flags` as for
    /// [`View::build`]; `None` when it is hidden. For `Access::Every` it is
    /// the edition whole, present or not, which locating a page reports.
    pub(crate) fn build(
        work: Arc<Work>,
        edition: &str,
        flags: Option<&HashMap<String, Vec<SeenMember>>>,
    ) -> Option<EditionView> {
        let sidecar = work.sidecar();
        let narrowed = match flags {
            None => None,
            Some(flags) => match Seen::of(sidecar.editions.get(edition)?, flags.get(edition)?)? {
                Seen::Whole => None,
                Seen::Narrowed(narrowed) => Some(narrowed),
            },
        };
        Some(EditionView {
            work,
            sidecar,
            key: edition.to_string(),
            narrowed,
        })
    }

    /// The work, for its path and id; its pages are [`Self::locate`]'s.
    pub(crate) fn work(&self) -> &Arc<Work> {
        &self.work
    }

    /// The edition's key.
    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    /// The edition as the account is sent it: narrowed, or whole as `build`
    /// checked it. `None` when the work has no such edition.
    pub(crate) fn edition(&self) -> Option<&Edition> {
        match &self.narrowed {
            Some(narrowed) => Some(narrowed.edition()),
            None => self.sidecar.editions.get(&self.key),
        }
    }

    /// The member holding the full page `index`, and the page's offset in it,
    /// in the sidecar `build` checked.
    pub(crate) fn locate(&self, index: u32) -> Result<(&Member, u32), String> {
        let key = &self.key;
        let full = self
            .sidecar
            .editions
            .get(key)
            .ok_or_else(|| format!("no edition `{key}` in `{}`", self.work.id))?;
        let (position, within) = full.locate(index).ok_or_else(|| {
            format!(
                "`{key}` has {} pages; there is no page {index}",
                full.total_pages()
            )
        })?;
        Ok((&full.members[position], within))
    }

    /// The full index of this account's `page`: the page itself for an edition
    /// sent whole, and for a narrowed one its visible page's, or the refusal of
    /// a page past its end, counted as the account is sent it.
    pub(crate) fn page_to_full(&self, page: u32) -> Result<u32, String> {
        let Some(narrowed) = &self.narrowed else {
            return Ok(page);
        };
        narrowed.page_to_full(page).ok_or_else(|| {
            format!(
                "`{}` has {} pages; there is no page {page}",
                self.key,
                narrowed.edition().total_pages()
            )
        })
    }
}

/// The flags of `slug`'s members (of `edition`'s alone when named) for an
/// `access` that is not `Every`, or the refusal to send: a 404 naming the work
/// when none of them is visible, the answer for a work this server does not
/// hold, so a refusal tells no one which slugs are real.
#[expect(
    clippy::result_large_err,
    reason = "the `Err` carries a `Response`, for `routes::on_the_grant`'s reason"
)]
pub(crate) fn flags_or_refuse(
    conn: &rusqlite::Connection,
    access: &Access,
    slug: &str,
    edition: Option<&str>,
) -> Result<HashMap<String, Vec<SeenMember>>, Response> {
    match taiyaku_library::browse::visible_members(conn, access, slug, edition) {
        Ok(flags) if flags.values().flatten().any(|seen| seen.visible) => Ok(flags),
        Ok(_) => Err(not_found(format!("no work `{slug}`"))),
        // The store's text, which can carry the statement, in the log alone
        // (O387).
        Err(why) => Err(crate::routes::answered_and_logged(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("work `{slug}` could not be read"),
            &why,
        )),
    }
}

/// [`flags_or_refuse`] on the blocking pool, through [`on_the_grant`]: `None`,
/// without a query, for `Access::Every`.
#[expect(
    clippy::result_large_err,
    reason = "the `Err` carries a `Response`, for `routes::on_the_grant`'s reason"
)]
async fn flags(
    state: &Arc<ServeState>,
    extensions: &axum::http::Extensions,
    slug: &str,
    edition: Option<&str>,
) -> Result<Option<HashMap<String, Vec<SeenMember>>>, Response> {
    let (slug, edition) = (slug.to_string(), edition.map(str::to_string));
    on_the_grant(state, extensions, Ok(None), move |conn, principal| {
        flags_or_refuse(conn, &principal.access, &slug, edition.as_deref()).map(Some)
    })
    .await
    .and_then(|answer| answer)
}

/// The work routes' view of `slug`: every edition the account sees. A work
/// it sees none of is refused as a slug no work holds is (O60).
#[expect(
    clippy::result_large_err,
    reason = "the `Err` carries a `Response`, for `routes::on_the_grant`'s reason"
)]
pub(crate) async fn of_work(
    state: &Arc<ServeState>,
    extensions: &axum::http::Extensions,
    slug: &str,
) -> Result<View, Response> {
    let flags = flags(state, extensions, slug, None).await?;
    built(state, slug, flags.as_ref()).await
}

/// The page and download routes' view of `slug`'s `edition`, in one query,
/// and their grant check: without it spec C1's per-root sharing would be a
/// browse filter rather than a boundary, and a reader who knew a slug could
/// read every page of every root. An edition the account cannot see, or whose
/// flags were not read of its members, is refused as a work this server does
/// not hold is, 404 rather than 403. `Access::Every` asks nothing.
#[expect(
    clippy::result_large_err,
    reason = "the `Err` carries a `Response`, for `routes::on_the_grant`'s reason"
)]
pub(crate) async fn of_edition(
    state: &Arc<ServeState>,
    extensions: &axum::http::Extensions,
    slug: &str,
    edition: &str,
) -> Result<EditionView, Response> {
    let flags = flags(state, extensions, slug, Some(edition)).await?;
    let found = work(state, slug)
        .await
        .map_err(IntoResponse::into_response)?;
    EditionView::build(found, edition, flags.as_ref())
        .ok_or_else(|| not_found(format!("no work `{slug}`")))
}

#[expect(
    clippy::result_large_err,
    reason = "the `Err` carries a `Response`, for `routes::on_the_grant`'s reason"
)]
async fn built(
    state: &Arc<ServeState>,
    slug: &str,
    flags: Option<&HashMap<String, Vec<SeenMember>>>,
) -> Result<View, Response> {
    let found = work(state, slug)
        .await
        .map_err(IntoResponse::into_response)?;
    View::build(found.sidecar(), flags).ok_or_else(|| not_found(format!("no work `{slug}`")))
}
