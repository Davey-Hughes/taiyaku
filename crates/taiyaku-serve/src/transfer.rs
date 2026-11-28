//! A response body that counts the bytes it hands over and writes its
//! request's log line when it ends, so a line describes the transfer and not
//! only the response head (O68). The request log layer in
//! `routes::build_router_serving` wraps every body in one.

use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use axum::body::{Body, Bytes, HttpBody};
use http_body::{Frame, SizeHint};

use crate::reqlog::{InFlight, Log, Outcome, Sent};

/// What the log layer knew when the response head was ready.
#[derive(Debug)]
pub(crate) struct Head {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) status: u16,
    /// [`crate::Entry::micros`].
    pub(crate) micros: u64,
    /// The length the head promised, if it did. A body that ends below it is
    /// `short`.
    pub(crate) promised: Option<u64>,
    /// False for a HEAD, a 204 and a 304, which hyper never sends a body
    /// for, however the body inside says it ends.
    pub(crate) sends_body: bool,
}

/// `inner`, counted. Its `Drop` writes the line, once, whatever ended it.
///
/// **`Drop`, not the end of the stream, writes the line.** Hyper drops a body
/// without polling it to its end when the client goes away and when the
/// server stops, and also once it has taken every byte the head promised, or
/// the body says it is at its end, as a `Full` does after its one frame and
/// an empty 304 does at once; axum drops a HEAD's itself. `Drop` is the one
/// place all of those pass, as `InFlight`'s is for the in-flight count.
#[derive(Debug)]
pub(crate) struct Counted {
    inner: Body,
    log: Arc<Log>,
    /// `None` once the line is written.
    request: Option<InFlight>,
    head: Head,
    bytes: u64,
    /// Set when the stream ended or failed while being polled.
    ended: Option<Outcome>,
}

impl Counted {
    pub(crate) fn new(inner: Body, log: Arc<Log>, request: InFlight, head: Head) -> Self {
        Self {
            inner,
            log,
            request: Some(request),
            head,
            bytes: 0,
            ended: None,
        }
    }

    /// The outcome of a body that reached its end cleanly.
    fn at_end(&self) -> Outcome {
        match self.head.promised {
            Some(promised) if self.bytes < promised => Outcome::Short,
            _ => Outcome::Complete,
        }
    }
}

impl HttpBody for Counted {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, axum::Error>>> {
        let this = self.get_mut();
        let polled = Pin::new(&mut this.inner).poll_frame(cx);
        match &polled {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    this.bytes += data.len() as u64;
                }
            }
            Poll::Ready(Some(Err(_))) => {
                this.ended.get_or_insert(Outcome::Error);
            }
            Poll::Ready(None) => {
                let at_end = this.at_end();
                this.ended.get_or_insert(at_end);
            }
            Poll::Pending => {}
        }
        polled
    }

    /// Forwarded unchanged: hyper reads these to set `Content-Length` and to
    /// send an empty body without polling it.
    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        let Some(request) = self.request.take() else {
            return;
        };
        let outcome = self.ended.unwrap_or_else(|| {
            let all_promised = self.head.promised.is_some_and(|n| self.bytes >= n);
            if !self.head.sends_body || all_promised || self.inner.is_end_stream() {
                self.at_end()
            } else {
                Outcome::Cut
            }
        });
        let sent = Sent {
            // Hyper sends no byte past the promised length, so none is
            // counted, though a file that grew on disk hands more over.
            bytes: self.head.promised.map_or(self.bytes, |n| self.bytes.min(n)),
            outcome,
            micros: self.head.micros,
        };
        let head = &self.head;
        self.log
            .finish(request, &head.method, &head.path, head.status, sent);
    }
}

#[cfg(test)]
mod tests;
