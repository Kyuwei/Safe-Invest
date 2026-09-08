//! Who is allowed through the door, when the door is a port.
//!
//! Serving MCP over a socket instead of a pipe changes the threat model
//! completely. A pipe is handed to one child process by its parent; a listening
//! port is reachable by every program on the machine, and — through the
//! browser — by every web page the person visits. This module is the check that
//! runs before any request reaches the game.
//!
//! Three separate things have to be true, and each stops something the others
//! do not:
//!
//! * the socket is bound to loopback, so nothing off the machine can reach it;
//! * the `Host` and `Origin` headers name a loopback address, which is what
//!   stops a page at `evil.com` from pointing DNS at `127.0.0.1` and talking to
//!   this server as if it were same-origin;
//! * the request carries the bearer token, which is what stops another program
//!   on the same machine — where loopback and headers prove nothing.
//!
//! The first two are configured on rmcp's own service. This module owns the
//! third, and the one rule rmcp cannot know: a browser must never be able to
//! send a request here without a preflight it will not get an answer to.

use http::{HeaderMap, Method, Request, StatusCode, header};

/// Why a request was turned away. The reason never reaches the caller — the
/// response says only "no" — but it is worth logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Not the MCP endpoint.
    UnknownPath,
    /// A verb the transport does not use.
    BadMethod,
    /// No `Authorization: Bearer …`, or the wrong token.
    BadToken,
    /// A POST that did not declare JSON.
    ///
    /// This is a security check, not pedantry. A cross-origin `fetch` carrying
    /// `application/json` is not a "simple request", so the browser must
    /// preflight it — and this server answers no preflight, so it never
    /// happens. Accepting `text/plain` would hand every web page a way in that
    /// skips that gate entirely.
    NotJson,
}

impl Refusal {
    pub fn status(self) -> StatusCode {
        match self {
            Self::UnknownPath => StatusCode::NOT_FOUND,
            Self::BadMethod => StatusCode::METHOD_NOT_ALLOWED,
            Self::BadToken => StatusCode::UNAUTHORIZED,
            Self::NotJson => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        }
    }

    /// What the client is told. Deliberately terse: an error that explains
    /// which half of the check failed is an error that helps someone guess.
    pub fn message(self) -> &'static str {
        match self {
            Self::UnknownPath => "Not found",
            Self::BadMethod => "Method not allowed",
            Self::BadToken => "Unauthorized",
            Self::NotJson => "Expected Content-Type: application/json",
        }
    }
}

/// The endpoint the server answers on. Anything else is a 404.
pub const PATH: &str = "/mcp";

/// Decides whether a request may reach the MCP service.
pub fn check<B>(request: &Request<B>, token: &str) -> Result<(), Refusal> {
    if request.uri().path() != PATH {
        return Err(Refusal::UnknownPath);
    }

    // POST carries requests, GET opens the event stream, DELETE closes a
    // session. OPTIONS is absent on purpose: answering a CORS preflight is
    // exactly how a browser would be granted permission to talk to us.
    match *request.method() {
        Method::POST => require_json(request.headers())?,
        Method::GET | Method::DELETE => {}
        _ => return Err(Refusal::BadMethod),
    }

    if !token_matches(request.headers(), token) {
        return Err(Refusal::BadToken);
    }

    Ok(())
}

fn require_json(headers: &HeaderMap) -> Result<(), Refusal> {
    let declared = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    // `application/json; charset=utf-8` is still JSON.
    let essence = declared
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    if essence == "application/json" {
        Ok(())
    } else {
        Err(Refusal::NotJson)
    }
}

fn token_matches(headers: &HeaderMap, expected: &str) -> bool {
    let Some(offered) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
    else {
        return false;
    };

    constant_time_eq(offered.trim().as_bytes(), expected.as_bytes())
}

/// Compares two byte strings without giving away where they first differ.
///
/// A comparison that stops at the first wrong byte takes measurably longer for
/// a token that shares a prefix, which is enough to recover one byte at a time.
/// The lengths are compared first and separately — that much is public, since a
/// token of the wrong length is wrong whatever it contains.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn request(method: Method, path: &str) -> http::request::Builder {
        Request::builder().method(method).uri(path)
    }

    fn post() -> http::request::Builder {
        request(Method::POST, PATH)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
    }

    #[test]
    fn a_well_formed_request_gets_through() {
        assert_eq!(check(&post().body(()).unwrap(), TOKEN), Ok(()));
    }

    #[test]
    fn the_charset_may_be_spelled_out() {
        let req = request(Method::POST, PATH)
            .header(header::CONTENT_TYPE, "Application/JSON; charset=utf-8")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .body(())
            .unwrap();
        assert_eq!(check(&req, TOKEN), Ok(()));
    }

    #[test]
    fn a_request_without_a_token_is_refused() {
        let req = request(Method::POST, PATH)
            .header(header::CONTENT_TYPE, "application/json")
            .body(())
            .unwrap();
        assert_eq!(check(&req, TOKEN), Err(Refusal::BadToken));
    }

    #[test]
    fn a_wrong_token_is_refused_however_close_it_is() {
        let nearly = format!("{}0", &TOKEN[..TOKEN.len() - 1]);
        let req = request(Method::POST, PATH)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {nearly}"))
            .body(())
            .unwrap();
        assert_eq!(check(&req, TOKEN), Err(Refusal::BadToken));
    }

    /// `text/plain` is a "simple request": a browser sends it cross-origin with
    /// no preflight at all. Refusing it is what keeps the preflight — which
    /// this server never answers — on the only road in.
    #[test]
    fn a_post_that_is_not_json_is_refused_before_the_token_is_even_read() {
        let req = request(Method::POST, PATH)
            .header(header::CONTENT_TYPE, "text/plain")
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .body(())
            .unwrap();
        assert_eq!(check(&req, TOKEN), Err(Refusal::NotJson));
    }

    #[test]
    fn a_post_with_no_content_type_is_refused() {
        let req = request(Method::POST, PATH)
            .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
            .body(())
            .unwrap();
        assert_eq!(check(&req, TOKEN), Err(Refusal::NotJson));
    }

    /// Answering a preflight would be handing a browser written permission.
    #[test]
    fn a_cors_preflight_is_not_answered() {
        let req = request(Method::OPTIONS, PATH).body(()).unwrap();
        assert_eq!(check(&req, TOKEN), Err(Refusal::BadMethod));
    }

    #[test]
    fn the_stream_and_the_session_close_need_no_content_type() {
        for method in [Method::GET, Method::DELETE] {
            let req = request(method.clone(), PATH)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(())
                .unwrap();
            assert_eq!(check(&req, TOKEN), Ok(()), "{method}");
        }
    }

    #[test]
    fn nothing_is_served_anywhere_but_the_endpoint() {
        for path in ["/", "/mcp/", "/mcp/../admin", "/health"] {
            let req = request(Method::GET, path)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(())
                .unwrap();
            assert_eq!(check(&req, TOKEN), Err(Refusal::UnknownPath), "{path}");
        }
    }

    #[test]
    fn a_token_of_the_wrong_length_is_refused() {
        for offered in ["", "Bearer", &TOKEN[..8], &format!("{TOKEN}extra")] {
            let req = request(Method::GET, PATH)
                .header(header::AUTHORIZATION, format!("Bearer {offered}"))
                .body(())
                .unwrap();
            assert_eq!(check(&req, TOKEN), Err(Refusal::BadToken), "{offered}");
        }
    }

    #[test]
    fn constant_time_eq_still_answers_the_question_correctly() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
        assert!(constant_time_eq(b"", b""));
    }
}
