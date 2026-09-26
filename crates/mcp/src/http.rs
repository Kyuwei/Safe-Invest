//! The same MCP server, reachable at a URL instead of down a pipe.
//!
//! Stdio works when the client can spawn the executable. Plenty of clients
//! would rather point at an address — so this serves the identical tool set
//! over MCP's streamable HTTP transport, on loopback, behind a token.
//!
//! It is off unless somebody turns it on. Opening a port is a decision, and it
//! is not one this program makes on a user's behalf at first launch.

use crate::SafeInvestServer;
use crate::guard::{self, Refusal};
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::body::Bytes;
use hyper_util::rt::{TokioIo, TokioTimer};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use safe_invest_service::Context;
use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinSet;
use tower_service::Service;

/// The port used when nothing else is asked for.
///
/// Not 98 000: a TCP port is sixteen bits, so 65 535 is the ceiling. This is
/// the nearest thing that fits, well clear of the registered range and of the
/// ports most local tooling reaches for.
pub const DEFAULT_PORT: u16 = 9800;

/// The lowest port worth allowing. Below 1024 needs privileges on Unix and is
/// reserved everywhere; a game does not belong there.
pub const MIN_PORT: u16 = 1024;

type Body = BoxBody<Bytes, Infallible>;

type McpService = StreamableHttpService<SafeInvestServer, LocalSessionManager>;

/// How long to wait before accepting again after `accept` failed.
///
/// The usual cause is running out of file handles, which does not clear on the
/// next instruction. Looping straight back would pin a core for nothing.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// How long a client has to finish sending a request's headers.
const HEADER_TIMEOUT: Duration = Duration::from_secs(20);

/// Claims the port, and says so plainly when it cannot.
///
/// Binding is separate from serving so a caller — the settings screen, say —
/// finds out that the port is taken while a person is still looking at it,
/// rather than in a log nobody reads.
///
/// The address is not configurable. Loopback is the whole security model: the
/// token and the header checks are the second and third locks, not the first.
pub async fn bind(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await
}

/// Serves MCP on an already-bound listener until the future is dropped.
///
/// Every connection belongs to this future. Dropping it — the port switched
/// off, moved, or restarted with a new token — closes the connections already
/// open too, event streams included. A connection spawned loose would outlive
/// its server: still answering, and still accepting the old token.
pub async fn serve(listener: TcpListener, context: Context, token: String) {
    let port = local_port(&listener);

    let config = StreamableHttpServerConfig::default()
        // A browser page that made DNS point `evil.com` at 127.0.0.1 sends
        // `Host: evil.com`, and reaches us as same-origin with CORS out of the
        // picture. Pinning the accepted hosts is what closes that.
        .with_allowed_hosts(["localhost", "127.0.0.1", "::1"])
        // rmcp defaults this to empty, which means "do not check" — not what a
        // local server wants. A page served from anywhere may POST here without
        // ever reading the answer, and that is enough to place an order. Only
        // loopback origins pass; a client that sends no Origin at all — every
        // native one — still does.
        .with_allowed_origins([
            format!("http://127.0.0.1:{port}"),
            format!("http://localhost:{port}"),
        ]);

    let service = StreamableHttpService::new(
        move || Ok(SafeInvestServer::new(context.clone())),
        Arc::new(LocalSessionManager::default()),
        config,
    );

    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok((stream, _peer)) => {
                    connections.spawn(connection(stream, service.clone(), token.clone()));
                }
                Err(error) => {
                    tracing::debug!(%error, "connexion MCP non acceptée");
                    tokio::time::sleep(ACCEPT_BACKOFF).await;
                }
            },
            // Finished connections are collected as they end, so the set holds
            // only the live ones however long the server runs.
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}

/// Answers one connection until the client closes it or the server stops.
async fn connection(stream: TcpStream, service: McpService, token: String) {
    let handler = hyper::service::service_fn(move |request| {
        let mut service = service.clone();
        let token = token.clone();
        async move {
            match guard::check(&request, &token) {
                Ok(()) => service.call(request).await,
                Err(refusal) => {
                    tracing::debug!(?refusal, "requête MCP refusée");
                    Ok(refused(refusal))
                }
            }
        }
    });

    if let Err(error) = hyper::server::conn::http1::Builder::new()
        .timer(TokioTimer::new())
        // A client that opens a connection and dribbles its headers holds a
        // task and a socket for as long as it likes. Twenty seconds is far
        // more than any local client takes.
        .header_read_timeout(HEADER_TIMEOUT)
        .serve_connection(TokioIo::new(stream), handler)
        .await
    {
        tracing::debug!(%error, "connexion MCP interrompue");
    }
}

/// Binds and serves in one call, for the command line.
pub async fn serve_http(context: Context, port: u16, token: String) -> anyhow::Result<()> {
    let listener = bind(port).await?;
    serve(listener, context, token).await;
    Ok(())
}

fn local_port(listener: &TcpListener) -> u16 {
    listener
        .local_addr()
        .map_or(DEFAULT_PORT, |addr| addr.port())
}

/// The answer to a request that did not get in.
///
/// No `Access-Control-Allow-Origin`, here or anywhere else in this file. A
/// browser that is told nothing cannot read what it got, and telling it
/// anything is the one change that would undo the rest of this module.
fn refused(refusal: Refusal) -> http::Response<Body> {
    http::Response::builder()
        .status(refusal.status())
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(refusal.message())).boxed())
        .unwrap_or_else(|_| {
            let mut fallback = http::Response::new(Full::new(Bytes::new()).boxed());
            *fallback.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;
            fallback
        })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_listener_is_bound_to_loopback_and_nowhere_else() {
        // Port 0 lets the OS choose, so the test never fights a real service.
        let listener = bind(0).await.unwrap();
        let addr = listener.local_addr().unwrap();

        assert!(addr.ip().is_loopback(), "{addr} n'est pas en loopback");
        assert_eq!(addr.ip(), Ipv4Addr::LOCALHOST);
    }

    #[tokio::test]
    async fn a_port_already_taken_is_reported_rather_than_swallowed() {
        let first = bind(0).await.unwrap();
        let port = first.local_addr().unwrap().port();

        // Whoever asks for a busy port must hear about it: the settings screen
        // shows this failure to the person while they are still looking at the
        // field they typed it into.
        assert!(bind(port).await.is_err());
    }

    #[test]
    fn the_default_port_is_a_port() {
        const { assert!(DEFAULT_PORT >= MIN_PORT) };
        // Stated as a test because the number came from a request for 98 000,
        // which is not expressible at all.
        assert_eq!(u32::from(DEFAULT_PORT), 9800);
    }
}
