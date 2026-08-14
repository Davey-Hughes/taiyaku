use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use super::*;

/// What one request looked like on the wire.
struct Captured {
    line: String,
    headers: Vec<String>,
    body: String,
}

/// A one-shot HTTP server on a loopback port, answering `reply` and handing
/// back what it was sent.
///
/// Hand-rolled on `TcpListener` rather than pulled in as a dependency:
/// what is under test is the four lines of `ureq` configuration in this
/// file, and a test server is 30 lines of `write!`. `UreqTransport` is the
/// one thing in `stump` that talks to the outside world, and it is exactly
/// the thing `Transport` exists to let every other test avoid — so nothing
/// offline exercised it, and every one of its methods could be replaced by
/// a constant unnoticed.
fn serve_once(
    status: u16,
    content_type: &str,
    body: &[u8],
) -> (String, std::sync::mpsc::Receiver<Captured>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = std::sync::mpsc::channel();
    let status_line = format!("HTTP/1.1 {status} X");
    let ct = content_type.to_string();
    let body = body.to_vec();

    std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("one connection");
        let captured = read_request(&stream);
        let mut stream = stream;
        let _ = write!(
            stream,
            "{status_line}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(&body);
        let _ = stream.flush();
        let _ = tx.send(captured);
    });

    (format!("http://127.0.0.1:{port}"), rx)
}

fn read_request(stream: &TcpStream) -> Captured {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();

    let mut headers = Vec::new();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        let header = header.trim_end().to_string();
        if header.is_empty() {
            break;
        }
        if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0);
        }
        headers.push(header);
    }

    let mut body = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut body).unwrap();
    }
    Captured {
        line: line.trim_end().to_string(),
        headers,
        body: String::from_utf8_lossy(&body).to_string(),
    }
}

fn header<'a>(c: &'a Captured, name: &str) -> Option<&'a str> {
    let name = format!("{}:", name.to_ascii_lowercase());
    c.headers
        .iter()
        .find(|h| h.to_ascii_lowercase().starts_with(&name))
        .map(|h| h[name.len()..].trim())
}

/// The two limits are bounds on how much of a reply is read, and a bound
/// nothing asserts is a bound every arithmetic mutation of it survives.
///
/// The page limit has to clear this corpus's largest pages, which is what
/// the comment above it claims and what `ureq`'s 10 MB default does not.
#[test]
fn the_read_limits_are_the_sizes_the_comments_claim() {
    assert_eq!(MAX_PAGE_BYTES, 536_870_912, "512 MiB");
    assert_eq!(MAX_REPLY_BYTES, 16_777_216, "16 MiB");
    // Through bindings rather than as literal constant expressions:
    // `assert!` on a wholly-constant condition is `clippy::assertions_on_constants`,
    // and its suggested `const { assert!(…) }` would move the check to
    // compile time — which sounds better and is worse here, because a
    // mutated constant would then fail to *build* and be reported as an
    // unviable mutant rather than a caught one.
    let largest_corpus_page: u64 = 430 * 1024 * 1024;
    let (page_limit, reply_limit) = (MAX_PAGE_BYTES, MAX_REPLY_BYTES);
    assert!(
        page_limit > largest_corpus_page,
        "the corpus reaches 430 MB a page; ureq's 10 MB default would truncate one"
    );
    assert!(reply_limit < page_limit, "a metadata reply is not an image");
}

/// `url` joins, `bearer` spells the header. Both are one line, both were
/// replaceable by `String::new()` unnoticed, and a wrong `url` sends every
/// request to the wrong place while a wrong `bearer` fails every one.
///
/// The trailing-slash trim is the part with judgement in it: `new` takes a
/// configured base that a person typed, and paths already start with `/`.
#[test]
fn the_base_url_is_joined_without_doubling_the_slash() {
    let t = UreqTransport::new("http://stump.example:10801/", "sekrit");
    assert_eq!(
        t.url("/api/graphql"),
        "http://stump.example:10801/api/graphql"
    );
    assert_eq!(t.bearer(), "Bearer sekrit");

    let untrimmed = UreqTransport::new("http://stump.example:10801", "k");
    assert_eq!(
        untrimmed.url("/x"),
        "http://stump.example:10801/x",
        "a base with no trailing slash joins the same way"
    );
}

/// What actually goes out on the wire, against a real socket.
///
/// `post_json` could return `Ok(vec![])` and no offline test noticed,
/// because everything else in `stump` runs against `FakeTransport`. This
/// asserts the four things the configuration is for: the path is below the
/// base, the bearer token is attached, the body is sent as typed, and the
/// reply is handed back rather than invented.
#[test]
fn post_json_sends_the_body_with_a_bearer_and_returns_the_reply() {
    let (base, rx) = serve_once(200, "application/json", br#"{"data":{"ok":true}}"#);
    let t = UreqTransport::new(base, "sekrit");

    let got = t
        .post_json("/api/graphql", r#"{"query":"x"}"#)
        .expect("the local server answers");

    assert_eq!(got, br#"{"data":{"ok":true}}"#.to_vec());
    let sent = rx.recv().expect("the server captured a request");
    assert_eq!(sent.line, "POST /api/graphql HTTP/1.1");
    assert_eq!(sent.body, r#"{"query":"x"}"#);
    assert_eq!(header(&sent, "authorization"), Some("Bearer sekrit"));
    assert_eq!(header(&sent, "content-type"), Some("application/json"));
}

/// A page fetch returns its status, its type and its bytes — all three,
/// and the status *as a value*.
#[test]
fn get_bytes_returns_the_status_content_type_and_body() {
    let (base, rx) = serve_once(200, "image/jpeg", &[0xFF, 0xD8, 0xFF, 0xE0]);
    let t = UreqTransport::new(base, "k");

    let (status, content_type, bytes) = t.get_bytes("/api/book/1/page/2").expect("answered");

    assert_eq!(status, 200);
    assert_eq!(content_type, "image/jpeg");
    assert_eq!(bytes, vec![0xFF, 0xD8, 0xFF, 0xE0]);
    let sent = rx.recv().expect("captured");
    assert_eq!(sent.line, "GET /api/book/1/page/2 HTTP/1.1");
    assert_eq!(header(&sent, "authorization"), Some("Bearer k"));
}

/// The one piece of judgement in `get_bytes`'s configuration:
/// `http_status_as_error(false)`, so a 404 from a page route comes back as
/// the number 404 rather than as a transport error. The caller says "this
/// book has no page N" better than an error string could — and without
/// this, `composite` would read a missing page as Stump being *down* and
/// trip the cooldown latch.
#[test]
fn a_404_is_an_answer_rather_than_a_transport_error() {
    let (base, _rx) = serve_once(404, "text/plain", b"no such page");
    let t = UreqTransport::new(base, "k");

    let (status, _, body) = t
        .get_bytes("/api/book/1/page/999")
        .expect("a 404 is returned, not raised");

    assert_eq!(status, 404);
    assert_eq!(body, b"no such page".to_vec());
}

/// A reply with no `Content-Type` gets the generic one rather than an
/// empty string, which would reach a browser as a header it cannot use.
#[test]
fn a_reply_with_no_content_type_falls_back_to_octet_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_request(&stream);
        let _ =
            stream.write_all(b"HTTP/1.1 200 X\r\nContent-Length: 2\r\nConnection: close\r\n\r\nhi");
    });

    let t = UreqTransport::new(format!("http://127.0.0.1:{port}"), "k");
    let (_, content_type, _) = t.get_bytes("/x").expect("answered");

    assert_eq!(content_type, "application/octet-stream");
}
