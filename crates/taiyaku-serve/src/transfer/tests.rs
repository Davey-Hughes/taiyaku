use super::*;

use std::collections::{BTreeMap, VecDeque};

/// A stream that yields `frames` in order, then ends.
struct Frames(VecDeque<Result<Bytes, std::io::Error>>);

impl futures_core::Stream for Frames {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.0.pop_front())
    }
}

fn streamed(frames: Vec<Result<&'static [u8], std::io::Error>>) -> Body {
    Body::from_stream(Frames(
        frames
            .into_iter()
            .map(|frame| frame.map(Bytes::from_static))
            .collect(),
    ))
}

/// `inner`, counted for a GET of `/t` that promised `promised` bytes.
fn counted(log: &Arc<Log>, inner: Body, promised: Option<u64>, sends_body: bool) -> Counted {
    let head = Head {
        method: "GET".into(),
        path: "/t".into(),
        status: 200,
        micros: 0,
        promised,
        sends_body,
    };
    Counted::new(inner, Arc::clone(log), log.begin(BTreeMap::new()), head)
}

/// Polls `body` for one frame.
async fn next_frame(body: &mut Counted) -> Option<Result<Frame<Bytes>, axum::Error>> {
    std::future::poll_fn(|cx| Pin::new(&mut *body).poll_frame(cx)).await
}

/// Polls `body` to its end or its first error.
async fn drain(mut body: Counted) {
    while let Some(Ok(_)) = next_frame(&mut body).await {}
}

/// The one line the test's log holds.
fn the_line(log: &Log, path: &std::path::Path) -> serde_json::Value {
    let mut lines = crate::test_support::logged_lines(log, path);
    assert_eq!(lines.len(), 1, "one body, one line: {lines:?}");
    lines.remove(0)
}

/// A download whose producer gave up with its error frame lost ends the
/// stream cleanly, short of its `Content-Length`: not complete.
#[tokio::test]
async fn a_body_that_ends_below_its_promised_length_logs_short() {
    let tmp = crate::test_support::Fixture::new("transfer-short");
    let (log, path) = crate::test_support::log(&tmp);

    drain(counted(&log, streamed(vec![Ok(b"four")]), Some(10), true)).await;

    let line = the_line(&log, &path);
    assert_eq!(line["outcome"], "short");
    assert_eq!(line["bytes"], 4);
}

#[tokio::test]
async fn a_body_that_yields_an_error_logs_error_with_what_came_before_it() {
    let tmp = crate::test_support::Fixture::new("transfer-error");
    let (log, path) = crate::test_support::log(&tmp);

    let frames = vec![Ok(&b"four"[..]), Err(std::io::Error::other("gone"))];
    drain(counted(&log, streamed(frames), Some(10), true)).await;

    let line = the_line(&log, &path);
    assert_eq!(line["outcome"], "error");
    assert_eq!(line["bytes"], 4);
}

/// Hyper stops polling a body that says it is at its end, so a buffered
/// body is dropped after its one frame without ever yielding `None`. That is
/// a complete transfer even where the head promised no length to compare.
#[tokio::test]
async fn a_body_dropped_once_it_says_it_has_ended_logs_complete() {
    let tmp = crate::test_support::Fixture::new("transfer-full");
    let (log, path) = crate::test_support::log(&tmp);

    let mut body = counted(&log, Body::from("abcd"), None, true);
    assert!(next_frame(&mut body).await.is_some());
    assert!(body.is_end_stream());
    drop(body);

    let line = the_line(&log, &path);
    assert_eq!(line["outcome"], "complete");
    assert_eq!(line["bytes"], 4);
}

/// Hyper also stops polling a body once it has written the length the head
/// promised, so a download read whole is dropped before its stream ends.
#[tokio::test]
async fn a_stream_dropped_at_its_promised_length_logs_complete() {
    let tmp = crate::test_support::Fixture::new("transfer-promised");
    let (log, path) = crate::test_support::log(&tmp);

    let mut body = counted(&log, streamed(vec![Ok(b"one"), Ok(b"two")]), Some(6), true);
    assert!(next_frame(&mut body).await.is_some());
    assert!(next_frame(&mut body).await.is_some());
    assert!(!body.is_end_stream(), "a stream does not know it has ended");
    drop(body);

    let line = the_line(&log, &path);
    assert_eq!(line["outcome"], "complete");
    assert_eq!(line["bytes"], 6);
}

/// A file that grew between its head and its read hands over more than its
/// `Content-Length`, which hyper does not send.
#[tokio::test]
async fn a_stream_longer_than_its_promised_length_counts_only_what_it_promised() {
    let tmp = crate::test_support::Fixture::new("transfer-longer");
    let (log, path) = crate::test_support::log(&tmp);

    let mut body = counted(&log, streamed(vec![Ok(b"one"), Ok(b"two")]), Some(4), true);
    assert!(next_frame(&mut body).await.is_some());
    assert!(next_frame(&mut body).await.is_some());
    drop(body);

    let line = the_line(&log, &path);
    assert_eq!(line["outcome"], "complete");
    assert_eq!(line["bytes"], 4);
}

/// `micros` is the head's, as the layer read it, and `transfer_micros` runs
/// from the request's arrival to the line.
#[tokio::test]
async fn a_line_times_the_head_and_the_transfer_apart() {
    let tmp = crate::test_support::Fixture::new("transfer-times");
    let (log, path) = crate::test_support::log(&tmp);

    let mut body = counted(&log, streamed(vec![Ok(b"one"), Ok(b"two")]), None, true);
    body.head.micros = 1234;
    assert!(next_frame(&mut body).await.is_some());
    std::thread::sleep(std::time::Duration::from_millis(20));
    drop(body);

    let line = the_line(&log, &path);
    assert_eq!(line["micros"], 1234);
    assert!(
        line["transfer_micros"].as_u64().unwrap() >= 20_000,
        "{line}"
    );
}

#[tokio::test]
async fn a_stream_dropped_between_its_frames_logs_cut() {
    let tmp = crate::test_support::Fixture::new("transfer-cut");
    let (log, path) = crate::test_support::log(&tmp);

    let mut body = counted(&log, streamed(vec![Ok(b"one"), Ok(b"two")]), Some(6), true);
    assert!(next_frame(&mut body).await.is_some());
    drop(body);

    let line = the_line(&log, &path);
    assert_eq!(line["outcome"], "cut");
    assert_eq!(line["bytes"], 3);
}

/// A HEAD's body is never sent, whatever is inside it, so its transfer is
/// complete however it is dropped.
#[tokio::test]
async fn a_body_that_is_never_sent_logs_complete_when_dropped_unread() {
    let tmp = crate::test_support::Fixture::new("transfer-head");
    let (log, path) = crate::test_support::log(&tmp);

    drop(counted(&log, streamed(vec![Ok(b"one")]), None, false));

    let line = the_line(&log, &path);
    assert_eq!(line["outcome"], "complete");
    assert_eq!(line["bytes"], 0);
}
