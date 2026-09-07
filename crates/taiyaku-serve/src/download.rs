//! Downloading a member's own bytes, and an edition as one streamed CBZ.
//!
//! Spec C2, `docs/superpowers/specs/2026-08-23-library-download-design.md`.
//! `cbz.rs` is the layout and `comicinfo.rs` is the metadata document; this
//! module is the HTTP part — which shape of body, what headers, and which of
//! the two guards refused.

/// What a downloaded file is called.
///
/// `Series - v01 (raw).cbz`. Series, because that is what groups a downloads
/// folder; the work title when there is no series, which is every
/// hand-imported work, since `import` never writes `series_id` and cannot —
/// `taiyaku.json` has no field for it. Both come from hand-edited JSON with no
/// validation, so either can sanitise to something with no usable character
/// in it at all — not only `"..."` (empty after `safe`), but `"/"` or
/// `"---"` too (non-empty after `safe`, but pure punctuation, which reads as
/// nothing to a person); when that happens the stem falls through to the
/// title, and if the title is equally unusable, to a constant, `download`,
/// rather than emit a name built from separators alone. Whatever stem wins
/// has its own leading punctuation and whitespace stripped afterward, so a
/// stem that starts with something other than a letter or digit — `"- Real
/// Title"` is usable (it has an `R` in it) but still starts with a `-` —
/// cannot leave the filename starting with `-`, `.`, or a space; a leading
/// `-` reads as a flag to ordinary shell tools. No volume drops the segment
/// rather than claiming volume 0; a non-finite volume does too, since it has
/// no meaningful two-digit rendering.
///
/// **`extension` is the caller's, not this function's**, because a
/// single-member edition served byte for byte may not be a comic archive at
/// all: `member.format` admits `pdf` and both epub kinds, and a `.cbz` on a PDF
/// would be a lie told in the filename as well as in the `Content-Type`.
/// Spec §7.
pub(crate) fn download_name(
    series: Option<&str>,
    volume: Option<f64>,
    title: &str,
    label: &str,
    extension: &str,
) -> String {
    // Usable, not merely non-empty: `/` sanitises to `-`, which is
    // non-empty but reads as nothing to a person, so a `.filter(|s|
    // !s.is_empty())` chain never fell through to the title beside it.
    let usable = |s: &str| s.chars().any(char::is_alphanumeric);
    let safe_series = series.map(safe).filter(|s| usable(s));
    let safe_title = || {
        let t = safe(title);
        usable(&t).then_some(t)
    };
    let stem = strip_leading_punctuation(
        &safe_series
            .or_else(safe_title)
            .unwrap_or_else(|| "download".to_string()),
    );
    // A non-finite volume (`INFINITY` can round-trip through a SQLite REAL
    // column; NaN cannot, since rusqlite converts a bound NaN to NULL) has
    // no sensible rendering, so it is treated the same as no volume at all
    // rather than risk `volume_text` printing something like `"vNaN"`.
    let volume_part = volume
        .filter(|v| v.is_finite())
        .map_or_else(String::new, |v| format!(" - v{}", volume_text(v)));
    format!("{stem}{volume_part} ({}).{extension}", safe(label))
}

/// Two digits at least, the fraction only when there is one.
///
/// `v01`, `v12`, `v105`, `v01.5`. Zero-padded so a downloads folder sorts, and
/// `work.volume` is REAL because omnibus and half-volumes exist — and, being
/// unconstrained, can hold any fraction, not only `.5`.
///
/// **The whole value is rounded once, before anything is split into a whole
/// part and a fraction.** An earlier version took `trunc()` for the whole
/// part and `volume - whole` for the fraction *first*, and rounded the
/// fraction to three decimals *second* — so a fraction that rounds up to
/// `1.000` (`1.9999`, or `49.99999999999999`) had nowhere to carry to: the
/// whole part was already formatted from the unrounded value, giving
/// `"011"` and `"491"` instead of `"02"` and `"50"`.
///
/// Rounding once means scaling to milli-units and rounding *there*, as a
/// single integer operation, then splitting that integer with `/` and `%` —
/// not `format!("{volume:.3}")` split on `'.'`. The two give different
/// answers for a value like `2.9995`: the nearest `f64` to that literal is
/// actually `2.99949999999999983302`, which correct decimal formatting
/// rounds *down* to `"2.999"`, while `2.9995 * 1000.0` happens to land on
/// the exactly-representable `2999.5`, which rounds *up* to `2999.5.round()
/// == 3000`. Milli-units matches how a person reads the number typed into
/// the sidecar; decimal formatting matches the `f64` underneath it. This
/// function does the former, since a card labelled `2.9995` is closer to
/// volume 3 than to volume 2 to the person who wrote that label.
///
/// Rounding to three decimal places and trimming trailing zeros (rather than
/// printing the raw `f64` remainder) matters on its own: most fractions
/// (`2.3 - 2.0`, for one) are not exactly representable in binary and would
/// otherwise spill their full imprecise expansion into the filename.
///
/// Caller's responsibility, not this function's: `volume` must be finite.
/// `download_name` filters `NAN`/`INFINITY` out before calling this, the same
/// as no volume at all, rather than let this print `"vNaN"` or `"vinf"`.
///
/// A sweep across very large volumes turned up a second way to get this
/// wrong: multiplying by 1000 to get milli-units overflows a `u64` past
/// roughly `1.84e16`, and a saturating cast turns every volume beyond that —
/// `1e300` and `f64::MAX` alike — into the identical wrong string
/// (`u64::MAX / 1000`), silently. `2^52` sidesteps it rather than papering
/// over it with a wider integer that only moves the ceiling: past `2^52` the
/// gap between adjacent `f64` values is already `>= 1.0`, so every such
/// volume is already a whole number with no fraction to round, and can be
/// formatted directly instead of multiplied.
const WHOLE_NUMBER_THRESHOLD: f64 = 4_503_599_627_370_496.0; // 2^52
fn volume_text(volume: f64) -> String {
    let magnitude = volume.abs();
    let (whole, fraction) = if magnitude >= WHOLE_NUMBER_THRESHOLD {
        (format!("{magnitude:.0}"), String::new())
    } else {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "magnitude < 2^52, so magnitude * 1000.0 < 2^52 * 1000 \
                      (~4.5e18), well inside u64's range: the cast neither \
                      truncates nor loses a sign"
        )]
        let milli = (magnitude * 1000.0).round() as u64;
        let whole = (milli / 1000).to_string();
        let fraction = milli % 1000;
        let fraction = if fraction == 0 {
            String::new()
        } else {
            format!("{fraction:03}").trim_end_matches('0').to_string()
        };
        (whole, fraction)
    };
    let padded_whole = format!("{whole:0>2}");
    let magnitude_text = if fraction.is_empty() {
        padded_whole
    } else {
        format!("{padded_whole}.{fraction}")
    };
    // `-0.0`, or any volume so close to zero it rounds to zero, must not
    // print a bare `-` in front of it — nothing about the sign survives
    // rounding to zero.
    let is_zero = magnitude_text.chars().all(|c| c == '0' || c == '.');
    if volume.is_sign_negative() && !is_zero {
        format!("-{magnitude_text}")
    } else {
        magnitude_text
    }
}

/// A path component nothing can escape from.
///
/// Separators and control characters become `-`; leading dots and the
/// whitespace around them are stripped, repeatedly, until nothing more comes
/// off — so no interleaving of the two (`. .hidden`, `.. . x`) leaves a dot in
/// front, and a title cannot make the download a hidden file. This can strip
/// a title down to the empty string (`"..."` has nothing else in it); that is
/// `download_name`'s problem to catch, not this function's. A title that
/// arrived from a filename can hold anything a filename can, which is why the
/// replacement is not optional.
fn safe(text: &str) -> String {
    let replaced: String = text
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c == ':' || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let mut stripped = replaced.trim().to_string();
    loop {
        let next = stripped.trim_start_matches('.').trim().to_string();
        if next == stripped {
            break;
        }
        stripped = next;
    }
    stripped
}

/// Strips characters from the front of a stem until it starts with something
/// alphanumeric.
///
/// `safe` already keeps a leading dot off (so a title cannot make the
/// download a hidden file), but a stem can still pass `download_name`'s
/// "has at least one alphanumeric character" check and begin with something
/// else entirely — `"- Real Title"` has an `R` in it, so it is usable, but
/// the `-` is still first. A leading `-` is parsed as a flag by ordinary
/// shell tools (`rm`, `mv`, and friends), so no filename this module returns
/// may begin with `-`, `.`, or whitespace. Unlike `safe`'s dot-stripping,
/// this needs no repeated pass: one predicate already covers every
/// character that isn't alphanumeric, so nothing a first pass removes can
/// expose another character the same pass would also have removed.
fn strip_leading_punctuation(stem: &str) -> String {
    stem.trim_start_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

/// The `Content-Disposition` for a download, in both forms RFC 6266 allows.
///
/// **Both, not one.** This library's titles are routinely non-ASCII —
/// `五等分の花嫁` is the search case that forced migration 8's trigram
/// tokeniser — and a client that understands only the quoted `filename=` must
/// still get something usable, while one that understands `filename*` must get
/// the real name. The quoted form is transliterated to ASCII by dropping what
/// will not fit, which can leave it nearly empty; that is why it is the
/// fallback and not the answer.
pub(crate) fn content_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| {
            if c.is_ascii() && c != '"' && c != '\\' && !c.is_control() {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut encoded = String::with_capacity(name.len() * 3);
    for byte in name.as_bytes() {
        let c = *byte as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~') {
            encoded.push(c);
        } else {
            let _ = std::fmt::Write::write_fmt(&mut encoded, format_args!("%{byte:02X}"));
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

use std::sync::Arc;

use axum::{
    extract::{Path as AxPath, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;

use crate::{ServeState, cbz};

/// The registered type for a comic archive, and what an OPDS client matches
/// its acquisition link against.
pub(crate) const CBZ_TYPE: &str = "application/vnd.comicbook+zip";

/// What a passthrough download says it is.
///
/// **The type follows the bytes, not the route.** A single-member edition may
/// not be a comic archive at all — `member.format` admits `pdf` and both epub
/// kinds — and calling a PDF `application/vnd.comicbook+zip` would be a lie
/// told in the header as well as in the filename. Spec §7. Only the *generated*
/// path is unconditionally [`CBZ_TYPE`], because there the container is this
/// module's own.
/// The extension a **passthrough** download of these members would serve, or
/// `None` where the download is generated.
///
/// One member that is a file is served byte for byte, and §7's rule is that
/// its type and its extension follow the bytes rather than the route: a
/// single-member edition can be a PDF or an EPUB, and serving one as
/// [`CBZ_TYPE`] under a `.cbz` name would be a lie told twice. Everything
/// else is generated, and generated output is always a `STORE` zip.
///
/// Extracted so [`prepare`] and [`advertised_type`] cannot disagree. A feed
/// advertising one type while the route sends another is that same lie in a
/// third place, which is why `refuse_if_hidden` is reused rather than
/// restated two modules away.
/// Every member of an edition, resolved against the document's own directory.
///
/// The other half of [`passthrough_extension`]'s job: extracting the *test*
/// while leaving both callers to build its input themselves left the root
/// rule — and the scanned-work case where these paths are already absolute
/// and the join is the identity — written out twice, which is the drift the
/// extraction was for.
fn member_paths(work: &crate::Work, edition: &taiyaku_core::Edition) -> Vec<std::path::PathBuf> {
    let root = work
        .path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();
    edition.members.iter().map(|m| root.join(&m.path)).collect()
}

fn passthrough_extension(paths: &[std::path::PathBuf]) -> Option<String> {
    (paths.len() == 1 && paths[0].is_file()).then(|| {
        paths[0]
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("cbz")
            .to_ascii_lowercase()
    })
}

/// What a download of this edition would answer with as its `Content-Type`.
///
/// For [`crate::opds::work_feed`]'s acquisition link, which has to declare
/// the type the route will actually send rather than a constant.
///
/// **Falls back to [`CBZ_TYPE`] rather than failing.** A member missing from
/// disk must not turn one bad edition into a 500 for a whole work's feed,
/// and an entry advertising the wrong type for material that is not there is
/// no worse than the page link it replaced, which for a *local* member would
/// also 404. (Not for a Stump-only member, where the page link resolves
/// through `PageSource` and this route does not — see `opds::work_feed`,
/// which states that asymmetry where the link is built.)
///
/// The cost is one `metadata` call per edition, in a feed that otherwise
/// touches no disk. Bounded — one work, its editions, metadata only, no
/// archive opened — and it is what buys the feed and the route being unable
/// to drift apart.
pub(crate) fn advertised_type(work: &crate::Work, edition: &taiyaku_core::Edition) -> &'static str {
    match passthrough_extension(&member_paths(work, edition)) {
        Some(extension) => type_for(&extension),
        None => CBZ_TYPE,
    }
}

fn type_for(extension: &str) -> &'static str {
    match extension {
        "cbz" | "zip" => CBZ_TYPE,
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "cbr" | "rar" => "application/vnd.comicbook-rar",
        _ => "application/octet-stream",
    }
}

/// How much unsent output the producer may run ahead by, in chunks.
///
/// The channel is the backpressure: a bounded sender blocks the blocking
/// thread when the client is slower than the disk, which is what keeps a
/// 1.49 GB volume out of memory. Eight is enough that a fast client never
/// waits on a page read.
///
/// **A chunk, not a page.** `passthrough` has always read and sent its file
/// in [`CHUNK_SIZE`] pieces; `generated` did not — it queued one whole
/// *page* per slot, taken straight from `for_each_member_page`'s in-memory
/// `Vec` and copied again into the `Bytes` the channel carries. This
/// corpus's pages run 100 KB–3 MB, with ~430 MB outliers in the Color Walk
/// art books (`taiyaku-read`'s `MAX_ENTRY_CAPACITY_HINT`) — eight of those
/// queued at once is 3.4 GB, around 4.3 GB counting the page `Vec` still
/// being read and the copy of it in flight, which is larger than the
/// 1.49 GB volume this channel exists to keep out of memory, not "eight
/// pages rather than a volume" as an earlier version of this comment
/// concluded from the very figure that contradicts it. `generated` now
/// chunks its payload the same [`CHUNK_SIZE`] way `passthrough` always has,
/// so a slot is at most one chunk regardless of which shape produced it,
/// and the high-water mark this constant actually bounds is CHANNEL_DEPTH ×
/// CHUNK_SIZE — about 512 KB — plus the one page still being read, not
/// CHANNEL_DEPTH pages.
const CHANNEL_DEPTH: usize = 8;

/// The size of one piece of output, for both download shapes.
///
/// `passthrough` has always read its file this way; `generated` is chunked
/// to match, rather than the whole-page slot [`CHANNEL_DEPTH`]'s doc comment
/// describes fixing. Below this a fast local disk pays more syscalls for no
/// benefit a bounded channel does not already provide; above it a slow
/// client's queue holds more per slot than [`CHANNEL_DEPTH`] was chosen to
/// bound.
const CHUNK_SIZE: usize = 64 * 1024;

/// How long the producer waits for one send to be accepted before treating
/// the client as gone rather than merely slow.
///
/// **Idle, not total.** [`send_with_timeout`] is called once per
/// [`CHUNK_SIZE`] piece, and the clock restarts on every one it accepts, so
/// this can never cap how long a whole download takes — a legitimate
/// 1.49 GB volume over a slow link must be free to run for hours. It only
/// fires on *silence*.
///
/// **The number, derived rather than picked.** The producer sends
/// [`CHUNK_SIZE`] (64 KiB) pieces. A client that accepts one chunk every `T`
/// seconds is reading at `65536 / T` bytes/sec; at `T` = 600 s (ten minutes)
/// that is about 109 bytes/sec. Ten minutes with *no* chunk accepted
/// therefore means the connection has sustained under ~110 bytes/sec for the
/// whole of that window — not a slow reader on mobile data, a dead one:
/// hyper has stopped polling under TCP backpressure because nothing is
/// reading the socket at all (`DOWNLOAD_LIMIT`'s doc comment below has the
/// mechanism). Ten minutes clears any plausible legitimate stall — a phone
/// locking, a handoff between cell towers — by a wide margin, while still
/// being short enough that a handful of stalled clients cannot wedge the
/// server for an unbounded time.
const IDLE_SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// How many downloads — each an open archive and, for a generated download,
/// one page fully in memory while it streams — may run at once.
///
/// **`cover.rs`'s `GENERATION_LIMIT` is the precedent, not the mechanism.**
/// That one is a hand-rolled `Mutex`+`Condvar` semaphore because its callers
/// run inside `spawn_blocking`, with no runtime to `.await` on. `edition`
/// and `member` below are already `async fn`s, so [`DOWNLOAD_LIMIT`] is a
/// `tokio::sync::Semaphore` instead — already a dependency (`sync` feature),
/// no `Cargo.lock` change — awaited right where the permit is needed:
/// before the producer thread is even spawned, rather than inside it. That
/// bounds the thread count itself, not merely what an already-spawned
/// thread holds, and costs no idle OS thread parked on a permit that is not
/// free yet.
///
/// **The number.** After the `CHANNEL_DEPTH` fix above, a generated
/// download's peak is dominated by one page `Vec` — this corpus's measured
/// worst case is the Color Walk art books at ~430 MB (`docs/notes.md`,
/// `MAX_ENTRY_CAPACITY_HINT`'s own doc comment) — the same order of
/// magnitude as the decoded buffer `GENERATION_LIMIT` was sized against
/// (~546 MiB for this corpus's worst page, inside a 3 GiB ceiling neither
/// route lets one request exceed). Four permits bounds the aggregate at
/// roughly four such pages — under 2 GB in the worst realistic case — while
/// still letting more than one reader download at once without queuing
/// behind a single slow transfer, which two permits would risk on a server
/// with more than one occasional reader.
///
/// **This is a reasoned starting point, not a measured one, and that
/// distinction matters.** `GENERATION_LIMIT` cites an actual measurement —
/// four concurrent decodes peaking at 3.0 GB. Nothing on this branch
/// measured concurrent *downloads*: no deployment's available memory is
/// recorded anywhere in this repository, and no session log shows how many
/// readers download at once. Four is reached by extending
/// `GENERATION_LIMIT`'s own method to the one number this branch does have
/// — the corpus's largest page — rather than invented outright; what would
/// settle it properly is either the target deployment's memory budget or an
/// observed concurrent-download count, and neither exists yet.
///
/// **A send timeout was considered beside this, rejected once, and has since
/// been added — [`IDLE_SEND_TIMEOUT`], applied to every `blocking_send` this
/// module used to do unconditionally, via [`send_with_timeout`].** The
/// reasoning that rejected it the first time is kept below rather than
/// deleted, because it was sound and most of it still is: a client that
/// stops reading — not merely a slow one — used to leave a producer thread
/// parked in `blocking_send` indefinitely, holding whichever permit it
/// acquired here. This semaphore alone bounds how many threads can be stuck
/// that way at once and what each one holds meanwhile; it does not by itself
/// reclaim a thread from a connection that never comes back. Before this cap
/// existed, such a client cost only its own thread and memory; after it,
/// four such clients hold every permit and every other reader's download —
/// not merely that client's — blocks behind them, so the cap turned a
/// per-connection leak into a whole-server outage at exactly the threshold
/// meant to bound memory.
///
/// **What changed is not the judgement that a guessed number is dangerous —
/// that is still true — but that [`IDLE_SEND_TIMEOUT`] is not guessed.**
/// Panels, the OPDS client this feed exists for, can still stall for reasons
/// indistinguishable from a hostile client inside any one send — a phone
/// locking, a handoff between towers — and nothing in this repository
/// measures how long that legitimate case runs; a measurement of it (a
/// Panels session log with timing, the same standard spec C2 §5 already
/// applies to the true-offset-seeking decision it separately defers) still
/// does not exist. What makes a timeout safe without that measurement is
/// arithmetic instead: [`IDLE_SEND_TIMEOUT`]'s own doc comment derives ten
/// minutes from this module's chunk size, not from any observed stall, and
/// ten minutes of zero progress on a 64 KiB chunk is under 110 bytes/sec
/// sustained — no plausible legitimate reader, and nowhere near what a
/// tower handoff or a locked phone costs. The timeout is idle, reset by
/// every accepted send, so it still cannot sever a real reader mid-volume
/// the way a total-duration cap would; it only reclaims the thread and the
/// permit from a connection that is provably gone.
const MAX_CONCURRENT_DOWNLOADS: usize = 4;

/// The semaphore [`MAX_CONCURRENT_DOWNLOADS`] describes. `const_new` rather
/// than `Arc` + a lazily-initialised heap allocation: a `'static` semaphore
/// hands out `SemaphorePermit<'static>`, which is `Send` and moves into the
/// producer thread's closure exactly as an owned permit would.
static DOWNLOAD_LIMIT: tokio::sync::Semaphore =
    tokio::sync::Semaphore::const_new(MAX_CONCURRENT_DOWNLOADS);

/// An `mpsc` receiver as the `Stream` `Body::from_stream` wants.
///
/// Hand-written rather than pulling in `tokio-stream`: the workspace's
/// dependency set is deliberately short enough to quote in `docs/status.md`,
/// and this is the whole of what that crate would be used for here.
struct ChannelStream(tokio::sync::mpsc::Receiver<Result<Bytes, std::io::Error>>);

impl futures_core::Stream for ChannelStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}

/// What the route decided to send, before it sends any of it.
enum Shape {
    /// One member's own bytes, from this absolute path.
    Passthrough {
        path: std::path::PathBuf,
        length: u64,
        content_type: &'static str,
    },
    /// A `STORE` zip built from these members, in this order.
    Generated {
        plan: cbz::Plan,
        /// One `(absolute member path, how many of the plan's entries it
        /// contributes)`, in order. The metadata document is entry 0 and is
        /// not a member.
        members: Vec<std::path::PathBuf>,
        comic_info: Vec<u8>,
    },
}

/// `GET /api/works/{slug}/{edition}/download`.
///
/// Spec C2 §2, §3, §5. The order of the two guards is the design: the root
/// grant answers 404 because an edition outside the grant does not admit that
/// it exists, and `can_download` answers 403 because "you may see this and may
/// not have it" is a different fact that a 404 would lie about.
///
/// `#[expect(clippy::result_large_err)]` for the reason `put_notes` and
/// `patch_alignments` in `routes.rs` already carry it (there as `#[allow]`,
/// predating this crate's `#[expect]`-only rule): the `spawn_blocking`
/// closure below resolves `prepare`'s `Result<(Shape, String), Response>`,
/// and clippy checks each closure as its own item, so the attribute has to
/// sit on this enclosing fn rather than on `prepare` alone.
#[expect(
    clippy::result_large_err,
    reason = "the spawn_blocking closure below resolves prepare's \
              Result<(Shape, String), Response>, and a Response is 128 \
              bytes; every refusal prepare can produce already is one, for \
              the same reason NoSuchWork and NotAPageIndex exist elsewhere \
              in this crate"
)]
pub(crate) async fn edition(
    State(state): State<Arc<ServeState>>,
    AxPath((slug, edition_key)): AxPath<(String, String)>,
    headers: HeaderMap,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) = crate::routes::refuse_if_hidden(&state, &extensions, &slug, &edition_key)
    {
        return refusal;
    }
    if let Some(refusal) = crate::routes::refuse_if_cannot_download(&extensions) {
        return refusal;
    }

    // Everything that can fail happens here, on a blocking thread, before the
    // body exists — because once the body has begun a failure cannot become a
    // status code. Spec §5.
    let prepared = {
        let state = state.clone();
        let slug = slug.clone();
        let edition_key = edition_key.clone();
        tokio::task::spawn_blocking(move || prepare(&state, &slug, &edition_key)).await
    };
    let (shape, name) = match prepared {
        Ok(Ok(pair)) => pair,
        Ok(Err(refusal)) => return refusal,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("preparing the download panicked: {e}"),
            )
                .into_response();
        }
    };

    let total = match &shape {
        Shape::Passthrough { length, .. } => *length,
        Shape::Generated { plan, .. } => plan.len(),
    };
    let content_type = match &shape {
        Shape::Passthrough { content_type, .. } => *content_type,
        Shape::Generated { .. } => CBZ_TYPE,
    };

    let (from, upto, partial) = match requested_range(&headers, total) {
        Ok(triple) => triple,
        Err(()) => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{total}"))],
                "that range is not inside this file",
            )
                .into_response();
        }
    };

    // Awaited here, before the producer thread exists at all — see
    // `DOWNLOAD_LIMIT`'s own doc comment for why this bounds the thread
    // count itself rather than merely what an already-spawned thread holds.
    let permit = DOWNLOAD_LIMIT
        .acquire()
        .await
        .expect("DOWNLOAD_LIMIT is never closed");

    // Captured here, inside the async handler, and moved into the producer
    // thread below — not obtained from inside that thread, which is never a
    // tokio worker and would panic if it tried `Handle::current()` itself.
    // `send_with_timeout`'s doc comment has the reasoning.
    let handle = tokio::runtime::Handle::current();

    let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_DEPTH);
    std::thread::Builder::new()
        .name("taiyaku-download".to_string())
        .spawn(move || {
            let _permit = permit;
            produce(shape, from, upto, &tx, &handle);
        })
        .expect("spawning the download producer");

    streamed(
        rx,
        Sent {
            from,
            upto,
            total,
            partial,
            content_type,
            name,
        },
    )
}

/// What a download's headers state, gathered so the two handlers say it once.
struct Sent {
    from: u64,
    upto: u64,
    total: u64,
    partial: bool,
    content_type: &'static str,
    name: String,
}

/// The response both handlers return: a streamed body and the headers that
/// describe it.
///
/// One function rather than one per handler. The edition download and the
/// member download differ in *what they produce* and in nothing about how they
/// announce it, and two copies of this block would be two places for a
/// `Content-Range` off-by-one to be fixed in one of.
fn streamed(
    rx: tokio::sync::mpsc::Receiver<Result<Bytes, std::io::Error>>,
    sent: Sent,
) -> Response {
    let mut response = axum::body::Body::from_stream(ChannelStream(rx)).into_response();
    *response.status_mut() = if sent.partial {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };
    let h = response.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        sent.content_type.parse().expect("static type"),
    );
    h.insert(
        header::CONTENT_LENGTH,
        (sent.upto - sent.from)
            .to_string()
            .parse()
            .expect("a number"),
    );
    h.insert(header::ACCEPT_RANGES, "bytes".parse().expect("static"));
    h.insert(
        header::CONTENT_DISPOSITION,
        content_disposition(&sent.name)
            .parse()
            .unwrap_or_else(|_| "attachment".parse().expect("static")),
    );
    if sent.partial {
        h.insert(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", sent.from, sent.upto - 1, sent.total)
                .parse()
                .expect("a range"),
        );
    }
    // A download is one file, not a per-reader document — but it is still
    // per-*grant*, so `private` stays and only `no-store` goes: a client may
    // keep the volume it just fetched.
    h.insert(
        header::CACHE_CONTROL,
        "private, no-cache".parse().expect("static"),
    );
    response
}

/// Decide the shape and the name, opening whatever has to be opened.
///
/// Runs on a blocking thread. Every refusal this can produce is a complete
/// `Response`, because all of them must be sent instead of a body rather than
/// in the middle of one.
///
/// A bare `Response` in the `Err` rather than a named type like `NoSuchWork`:
/// this function's refusals are worded five different ways, which a shared
/// wrapper type would not shorten. `#[expect(clippy::result_large_err)]` for
/// the same reason `one_slot` in `routes.rs` carries it.
#[expect(
    clippy::result_large_err,
    reason = "a Response is 128 bytes and every refusal this function can \
              produce already is one; a named error type here would only \
              wrap one Response per call site, which one_slot's own doc \
              comment says buys nothing a comment does not already say"
)]
fn prepare(
    state: &Arc<ServeState>,
    slug: &str,
    edition_key: &str,
) -> Result<(Shape, String), Response> {
    let work = state
        .work_or_missing(slug)
        .map_err(crate::routes::not_found)?;
    let sidecar = work.sidecar();
    let found = sidecar.editions.get(edition_key).ok_or_else(|| {
        crate::routes::not_found(format!("no edition `{edition_key}` in `{slug}`"))
    })?;
    // A scanned work has no document, so its member paths are absolute and
    // this join is the identity — `derive.rs` says so where it writes them.
    let paths = member_paths(work, found);
    if paths.is_empty() {
        return Err(crate::routes::not_found(format!(
            "`{edition_key}` of `{slug}` has no members"
        )));
    }

    // The library rows the name and the metadata document come from. A work
    // this server holds only as a document has no rows, and the fallbacks are
    // what that case gets.
    let (series, volume, title) = {
        let conn = state.library.conn();
        conn.query_row(
            "SELECT
               (SELECT COALESCE(
                  (SELECT title FROM series_title
                    WHERE series_id = w.series_id ORDER BY is_primary DESC, title LIMIT 1),
                  (SELECT sort_title FROM series WHERE id = w.series_id))),
               w.volume, w.title
             FROM work w WHERE w.slug = ?1",
            [slug],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .unwrap_or((None, None, sidecar.work.title.clone()))
    };

    if let Some(extension) = passthrough_extension(&paths) {
        let length = std::fs::metadata(&paths[0])
            .map_err(|e| crate::routes::not_found(format!("{}: {e}", paths[0].display())))?
            .len();
        let content_type = type_for(&extension);
        let name = download_name(series.as_deref(), volume, &title, &found.label, &extension);
        return Ok((
            Shape::Passthrough {
                path: paths[0].clone(),
                length,
                content_type,
            },
            name,
        ));
    }

    // Generated. Describe every page first — for an archive member this reads
    // only central directories.
    let mut described: Vec<taiyaku_read::PageEntry> = Vec::new();
    for path in &paths {
        let entries = taiyaku_read::member_page_entries(path).map_err(crate::routes::not_found)?;
        described.extend(entries);
    }
    if described.is_empty() {
        return Err(crate::routes::not_found(format!(
            "`{edition_key}` of `{slug}` holds no pages"
        )));
    }

    let page_count = u32::try_from(described.len()).unwrap_or(u32::MAX);
    let comic_info = {
        let conn = state.library.conn();
        taiyaku_library::comicinfo::read(&conn, slug, edition_key, page_count)
            .map(|info| taiyaku_library::comicinfo::xml(&info).into_bytes())
            .unwrap_or_else(|_| {
                // No rows for this work — a document-only library. The
                // document still states what the sidecar knows.
                taiyaku_library::comicinfo::xml(&taiyaku_library::comicinfo::ComicInfo {
                    series: None,
                    number: None,
                    title: Some(title.clone()),
                    page_count,
                    language: None,
                    credits: Vec::new(),
                    notes: Some(found.label.clone()),
                })
                .into_bytes()
            })
    };

    // Zero-padded and sequential across the whole edition, width from the
    // total, extension from the source entry. Name order is then reading
    // order, which is the trap `docs/sidecar.md`'s rule 1 exists to document.
    let width = page_count.to_string().len().max(4);
    let mut planned = vec![cbz::PlannedEntry {
        name: "ComicInfo.xml".to_string(),
        size: comic_info.len() as u64,
        crc32: crc32fast::hash(&comic_info),
    }];
    for (index, entry) in described.iter().enumerate() {
        let extension = std::path::Path::new(&entry.name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("jpg")
            .to_ascii_lowercase();
        planned.push(cbz::PlannedEntry {
            name: format!("{:0width$}.{extension}", index + 1, width = width),
            size: entry.size,
            crc32: entry.crc32,
        });
    }

    let name = download_name(series.as_deref(), volume, &title, &found.label, "cbz");
    Ok((
        Shape::Generated {
            plan: cbz::Plan::of(planned),
            members: paths,
            comic_info,
        },
        name,
    ))
}

/// `(from, upto, is a 206)`, or `Err(())` for a range outside the file.
///
/// One range only. A multi-range request is answered with the whole file,
/// which RFC 9110 explicitly permits and which is the right trade here: the
/// `multipart/byteranges` body a real answer needs is not something any OPDS
/// client has been observed asking for, and inventing it unmeasured is the
/// mistake spec §5 avoids elsewhere.
fn requested_range(headers: &HeaderMap, total: u64) -> Result<(u64, u64, bool), ()> {
    let Some(value) = headers.get(header::RANGE).and_then(|v| v.to_str().ok()) else {
        return Ok((0, total, false));
    };
    let Some(spec) = value.strip_prefix("bytes=") else {
        return Ok((0, total, false));
    };
    if spec.contains(',') {
        return Ok((0, total, false));
    }
    let (start, end) = spec.split_once('-').ok_or(())?;
    let (from, upto) = match (start.trim(), end.trim()) {
        ("", "") => return Err(()),
        // `bytes=-500`: the last 500 bytes.
        ("", tail) => {
            let n: u64 = tail.parse().map_err(|_| ())?;
            (total.saturating_sub(n), total)
        }
        (head, "") => (head.parse().map_err(|_| ())?, total),
        (head, tail) => {
            let from: u64 = head.parse().map_err(|_| ())?;
            let last: u64 = tail.parse().map_err(|_| ())?;
            (from, last.saturating_add(1).min(total))
        }
    };
    if from >= total || from >= upto {
        return Err(());
    }
    Ok((from, upto, true))
}

/// What happened when the producer tried to hand one item to the channel.
#[derive(Debug)]
enum SendOutcome {
    /// Accepted. The idle clock restarts on the caller's next send.
    Sent,
    /// The receiver is gone — the client hung up. Not an error: the caller
    /// stops producing, exactly as it always has.
    Gone,
    /// Nothing was accepted inside the timeout. The client is stalled, not
    /// merely slow; the caller abandons the download.
    TimedOut,
}

/// Send one item down `tx`, giving up after `timeout` of silence.
///
/// `tokio::sync::mpsc::Sender::blocking_send` has no timeout variant, so this
/// drives the async `Sender::send` itself, through `handle`, from inside the
/// caller's own thread. **That is legal only because the producer runs on a
/// dedicated `std::thread`, never a tokio worker** — `Handle::block_on`
/// panics if the calling thread is already inside a runtime, which is
/// exactly the thread kind this function must never be called from.
///
/// Takes `timeout` as a parameter rather than reading [`IDLE_SEND_TIMEOUT`]
/// directly so a test can drive it with milliseconds instead of minutes;
/// every real call site passes that constant.
///
/// **`tokio::time::timeout(timeout, tx.send(item))` must be built *inside*
/// the future handed to `block_on`, not as its argument.** Building it first
/// and passing the already-constructed value was the first version of this
/// function, and it panicked — `Timeout::new_with_delay` registers with the
/// runtime's timer driver at construction, not at first poll, and as a plain
/// function argument it is constructed before `block_on` has entered any
/// runtime context on this thread: "there is no reactor running, must be
/// called from the context of a Tokio 1.x runtime". Wrapping it in an
/// `async` block defers that construction to the block's first poll, which
/// happens only after `block_on` has entered the context.
fn send_with_timeout(
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    item: Result<Bytes, std::io::Error>,
    handle: &tokio::runtime::Handle,
    timeout: std::time::Duration,
) -> SendOutcome {
    match handle.block_on(async { tokio::time::timeout(timeout, tx.send(item)).await }) {
        Ok(Ok(())) => SendOutcome::Sent,
        Ok(Err(_)) => SendOutcome::Gone,
        Err(_) => SendOutcome::TimedOut,
    }
}

/// Send `[from, upto)` of the output down `tx`, then stop.
///
/// Runs on its own thread, not a `spawn_blocking` worker: this holds the
/// thread for the whole download, which for a 1.49 GB volume over a slow
/// connection is minutes, and tokio's blocking pool is sized for short waits.
///
/// `handle` is a `Handle` to the runtime that spawned this thread, captured
/// in the async route handler before the thread existed — see `edition` and
/// `member` below — so [`send_with_timeout`] can drive an async `send` from
/// here without this thread ever being a tokio worker itself.
fn produce(
    shape: Shape,
    from: u64,
    upto: u64,
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    handle: &tokio::runtime::Handle,
) {
    let sent = match shape {
        Shape::Passthrough { path, .. } => passthrough(&path, from, upto, tx, handle),
        Shape::Generated {
            plan,
            members,
            comic_info,
        } => generated(&plan, &members, &comic_info, from, upto, tx, handle),
    };
    if let Err(message) = sent {
        // The body has already begun, so this cannot be a status code. Closing
        // the stream short of `Content-Length` is the only signal available;
        // the log line is what makes it diagnosable. Spec §5.
        eprintln!("taiyaku: download failed mid-stream: {message}");
        // Also under a timeout: if `sent` failed because the client stalled,
        // a plain `blocking_send` here would re-park this thread on the same
        // dead channel `send_with_timeout` above just gave up on. The
        // outcome is discarded either way — this is the last thing `produce`
        // does regardless of whether it lands.
        let _ = send_with_timeout(
            tx,
            Err(std::io::Error::other(message)),
            handle,
            IDLE_SEND_TIMEOUT,
        );
    }
}

/// One member's own bytes, seeking to `from`.
fn passthrough(
    path: &std::path::Path,
    from: u64,
    upto: u64,
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    handle: &tokio::runtime::Handle,
) -> Result<(), String> {
    use std::io::{Read as _, Seek as _};
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    file.seek(std::io::SeekFrom::Start(from))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut left = upto - from;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    while left > 0 {
        let want = usize::try_from(left)
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        let read = file
            .read(&mut buffer[..want])
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if read == 0 {
            return Err(format!("{}: ended early", path.display()));
        }
        match send_with_timeout(
            tx,
            Ok(Bytes::copy_from_slice(&buffer[..read])),
            handle,
            IDLE_SEND_TIMEOUT,
        ) {
            SendOutcome::Sent => {}
            // The client hung up. Not an error; stop reading.
            SendOutcome::Gone => return Ok(()),
            SendOutcome::TimedOut => {
                return Err(format!(
                    "{}: no send accepted in {IDLE_SEND_TIMEOUT:?}; the client \
                     appears to have stalled",
                    path.display()
                ));
            }
        }
        left -= read as u64;
    }
    Ok(())
}

/// The generated zip, produced from zero and clipped to `[from, upto)`.
fn generated(
    plan: &cbz::Plan,
    members: &[std::path::PathBuf],
    comic_info: &[u8],
    from: u64,
    upto: u64,
    tx: &tokio::sync::mpsc::Sender<Result<Bytes, std::io::Error>>,
    handle: &tokio::runtime::Handle,
) -> Result<(), String> {
    let mut at = 0u64;
    let mut index = 0usize;
    // `false` once the client has gone. Not `mut`: every capture here (`tx`,
    // `from`, `upto`, `handle`) is `Copy` or already a shared reference, so
    // the closure is `Fn` and calling it never needs a unique borrow of the
    // binding — `let mut send` triggered `unused_mut` under `-D warnings`.
    //
    // **Chunked at `CHUNK_SIZE`, the way `passthrough` already chunks its
    // file** — `CHANNEL_DEPTH`'s doc comment has the reasoning. `clipped`
    // returns a subslice of `bytes` with no copy of its own, so `chunks`
    // below walks that view and each `Bytes::copy_from_slice` below only
    // ever copies one `CHUNK_SIZE` piece, never the whole page or local
    // header at once. This changes how many `Bytes` frames a page becomes
    // in the stream, never their bytes or their order, so it cannot change
    // the zip this produces: the chunking is beneath the entry level
    // `cbz::Plan` reasons about, `Content-Length` is unaffected, and a
    // client sees one continuous body of the same bytes either way.
    //
    // Each chunk goes through `send_with_timeout` rather than a bare
    // `blocking_send`, so a client that stops reading mid-entry is abandoned
    // after `IDLE_SEND_TIMEOUT` of silence instead of parking this thread
    // (and the permit it holds) forever — see that constant's doc comment.
    let send = |bytes: &[u8], at: u64| -> Result<bool, String> {
        let part = cbz::clipped(bytes, at, from, upto);
        for chunk in part.chunks(CHUNK_SIZE) {
            match send_with_timeout(
                tx,
                Ok(Bytes::copy_from_slice(chunk)),
                handle,
                IDLE_SEND_TIMEOUT,
            ) {
                SendOutcome::Sent => {}
                SendOutcome::Gone => return Ok(false),
                SendOutcome::TimedOut => {
                    return Err(format!(
                        "no send accepted in {IDLE_SEND_TIMEOUT:?}; the \
                         client appears to have stalled"
                    ));
                }
            }
        }
        Ok(true)
    };

    // Entry 0 is the metadata document.
    let header = plan.local_header(index);
    if !send(&header, at)? {
        return Ok(());
    }
    at += header.len() as u64;
    if !send(comic_info, at)? {
        return Ok(());
    }
    at += comic_info.len() as u64;
    index += 1;

    for path in members {
        // The plan is authoritative about names and lengths; the member is
        // asked only for bytes. A mismatch means the archive changed since
        // `prepare` read its central directory, and continuing would produce
        // a file whose own directory lies.
        let mut failed: Option<String> = None;
        let mut gone = false;
        let visited = taiyaku_read::for_each_member_page(path, &mut |entry, bytes| {
            let planned = plan
                .entries()
                .get(index)
                .ok_or_else(|| format!("{}: more pages than the plan named", path.display()))?;
            if planned.size != entry.size || planned.crc32 != entry.crc32 {
                // **Two causes, one refusal, and the message must not pick
                // one.** The plan's size is what the archive's central
                // directory *declared*; `entry.size` is what
                // `for_each_member_page` actually read. They differ either
                // because the archive was replaced since `prepare` read it,
                // or because it declares a size it never had — `zip`
                // validates an entry's crc32 against its bytes and never
                // validates its declared length, so a size-lying archive
                // reaches here with a perfectly good crc. Either way the
                // local header already sent states a length these bytes do
                // not have, so the only safe move is to stop.
                return Err(format!(
                    "{}: `{}` is not the {} bytes this download was planned around",
                    path.display(),
                    entry.name,
                    planned.size
                ));
            }
            let header = plan.local_header(index);
            match send(&header, at) {
                Ok(true) => {}
                Ok(false) => {
                    gone = true;
                    return Err(String::new());
                }
                Err(e) => return Err(e),
            }
            at += header.len() as u64;
            match send(bytes, at) {
                Ok(true) => {}
                Ok(false) => {
                    gone = true;
                    return Err(String::new());
                }
                Err(e) => return Err(e),
            }
            at += bytes.len() as u64;
            index += 1;
            // Nothing after `upto` needs producing.
            if at >= upto {
                gone = true;
                return Err(String::new());
            }
            Ok(())
        });
        if let Err(message) = visited {
            if gone {
                return Ok(());
            }
            failed = Some(message);
        }
        if let Some(message) = failed {
            return Err(message);
        }
    }

    // **The third direction the per-page check above cannot catch.** That
    // check runs once per page actually visited, so it sees a member with
    // more pages than planned (the `.get(index)` lookup above fails) and a
    // page whose declared size or crc32 changed. It cannot see a member that
    // lost pages: `for_each_member_page` simply stops early, `visited` comes
    // back `Ok(())`, and nothing above notices `index` never reached
    // `plan.entries().len()`. Uncaught, the trailer below still gets written
    // in full — every central-directory record `plan.trailer()` was built
    // for — at an absolute offset that is short by exactly the missing
    // pages' bytes, so the EOCD's directory offset points into the payload
    // region instead of the directory that follows it. Checked here, once,
    // rather than by threading a page-count comparison through every loop
    // iteration above.
    if index != plan.entries().len() {
        return Err(format!(
            "the plan named {} pages but only {index} were found while \
             streaming; the edition changed during download",
            plan.entries().len()
        ));
    }

    let trailer = plan.trailer();
    let _ = send(&trailer, at)?;
    Ok(())
}

/// `GET /api/works/{slug}/{edition}/members/{ordinal}/download`.
///
/// One member's original bytes, unchanged. `{ordinal}` is the position in the
/// document counting from 0 — `member.ordinal`, which `import.rs` writes and
/// `Edition::locate` returns. **Not the path**: a path is long, holds
/// separators, and is the field `import` reconciles on, so a URL built from one
/// would move under a rescan that followed the content identity rule as
/// designed.
///
/// There is no OPDS mirror of this route. OPDS asks for a publication, and a
/// chapter file inside one is not something a feed has an entry for.
///
/// `#[expect(clippy::result_large_err)]` for the same reason `edition` above
/// carries it: the `spawn_blocking` closure below resolves `prepare_member`'s
/// `Result<_, Response>`, and clippy checks each closure as its own item, so
/// the attribute has to sit on this enclosing fn rather than on
/// `prepare_member` alone.
#[expect(
    clippy::result_large_err,
    reason = "the spawn_blocking closure below resolves prepare_member's \
              Result<(PathBuf, u64, &'static str, String), Response>, and a \
              Response is 128 bytes; every refusal prepare_member can produce \
              already is one, for the same reason NoSuchWork and \
              NotAPageIndex exist elsewhere in this crate"
)]
pub(crate) async fn member(
    State(state): State<Arc<ServeState>>,
    AxPath((slug, edition_key, raw)): AxPath<(String, String, String)>,
    headers: HeaderMap,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) = crate::routes::refuse_if_hidden(&state, &extensions, &slug, &edition_key)
    {
        return refusal;
    }
    if let Some(refusal) = crate::routes::refuse_if_cannot_download(&extensions) {
        return refusal;
    }
    // Refused here in this server's own words rather than by axum's `Path`
    // rejection in terms of a Rust type — the same reason `page_index` exists
    // (O5).
    let Ok(ordinal) = raw.parse::<usize>() else {
        return crate::routes::not_found(format!(
            "`{raw}` is not a member number; they count from 0"
        ));
    };

    let prepared = {
        let state = state.clone();
        let slug = slug.clone();
        let edition_key = edition_key.clone();
        tokio::task::spawn_blocking(move || prepare_member(&state, &slug, &edition_key, ordinal))
            .await
    };
    let (path, length, content_type, name) = match prepared {
        Ok(Ok(four)) => four,
        Ok(Err(refusal)) => return refusal,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("preparing the member download panicked: {e}"),
            )
                .into_response();
        }
    };

    let (from, upto, partial) = match requested_range(&headers, length) {
        Ok(triple) => triple,
        Err(()) => {
            return (
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{length}"))],
                "that range is not inside this file",
            )
                .into_response();
        }
    };

    // See the identical wait in `edition` above, and `DOWNLOAD_LIMIT`'s own
    // doc comment for why it is one shared budget rather than one per route.
    let permit = DOWNLOAD_LIMIT
        .acquire()
        .await
        .expect("DOWNLOAD_LIMIT is never closed");

    // See the identical capture in `edition` above and `send_with_timeout`'s
    // doc comment for why it must happen here, not inside the thread below.
    let handle = tokio::runtime::Handle::current();

    let (tx, rx) = tokio::sync::mpsc::channel(CHANNEL_DEPTH);
    std::thread::Builder::new()
        .name("taiyaku-download-member".to_string())
        .spawn(move || {
            let _permit = permit;
            if let Err(message) = passthrough(&path, from, upto, &tx, &handle) {
                eprintln!("taiyaku: member download failed mid-stream: {message}");
                let _ = send_with_timeout(
                    &tx,
                    Err(std::io::Error::other(message)),
                    &handle,
                    IDLE_SEND_TIMEOUT,
                );
            }
        })
        .expect("spawning the member download producer");

    streamed(
        rx,
        Sent {
            from,
            upto,
            total: length,
            partial,
            content_type,
            name,
        },
    )
}

/// `(path, length, content type, filename)` for one member.
///
/// `#[expect(clippy::result_large_err)]` for the same reason `prepare` above
/// carries it: a `Response` is 128 bytes and every refusal this function can
/// produce already is one.
#[expect(
    clippy::result_large_err,
    reason = "a Response is 128 bytes and every refusal this function can \
              produce already is one; a named error type here would only \
              wrap one Response per call site, which one_slot's own doc \
              comment says buys nothing a comment does not already say"
)]
fn prepare_member(
    state: &Arc<ServeState>,
    slug: &str,
    edition_key: &str,
    ordinal: usize,
) -> Result<(std::path::PathBuf, u64, &'static str, String), Response> {
    let work = state
        .work_or_missing(slug)
        .map_err(crate::routes::not_found)?;
    let sidecar = work.sidecar();
    let found = sidecar.editions.get(edition_key).ok_or_else(|| {
        crate::routes::not_found(format!("no edition `{edition_key}` in `{slug}`"))
    })?;
    let held = found.members.get(ordinal).ok_or_else(|| {
        crate::routes::not_found(format!(
            "`{edition_key}` of `{slug}` has {} members; there is no member {ordinal}",
            found.members.len()
        ))
    })?;
    let root = work.path.parent().unwrap_or(std::path::Path::new("."));
    let path = root.join(&held.path);
    if path.is_dir() {
        return Err(crate::routes::not_found(format!(
            "member {ordinal} of `{edition_key}` is a directory of pages, not a file; download the edition instead"
        )));
    }
    let length = std::fs::metadata(&path)
        .map_err(|e| crate::routes::not_found(format!("{}: {e}", path.display())))?
        .len();
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("cbz")
        .to_ascii_lowercase();
    let content_type = type_for(&extension);
    // **A member keeps its own filename.** The edition download gets a name
    // built from the library's rows because it is a file that did not exist
    // before; a member already had one, and it is the name the person who
    // packed it chose.
    let name = path.file_name().map_or_else(
        || format!("member-{ordinal}.{extension}"),
        |n| n.to_string_lossy().to_string(),
    );
    Ok((path, length, content_type, name))
}

#[cfg(test)]
mod tests;
