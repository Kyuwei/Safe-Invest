//! The MCP server, talked to over its port the way a real client would.
//!
//! `guard.rs` proves the rules in isolation. This proves they are actually
//! wired in front of the service — that a request without a token is turned
//! away by a running server, and that one with a token gets a real MCP
//! handshake back.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test that trips is a test that failed"
)]

use safe_invest_mcp::http;
use safe_invest_service::{Context, ContextConfig};
use serde_json::{Value, json};

const TOKEN: &str = "aaaaaaaabbbbbbbbccccccccddddddddaaaaaaaabbbbbbbbccccccccdddddddd";

/// Starts a server on a port the OS picks, and hands back its URL.
async fn server(data_dir: &std::path::Path) -> String {
    let context = Context::new(&ContextConfig {
        data_dir: Some(data_dir.to_path_buf()),
        force_simulated: true,
    })
    .unwrap();

    let listener = http::bind(0).await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(http::serve(listener, context, TOKEN.to_owned()));

    // Give the accept loop a turn before the first request.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    format!("http://127.0.0.1:{port}/mcp")
}

fn initialize() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "test", "version": "0" }
        }
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_with_the_token_completes_a_handshake() {
    let dir = tempfile::tempdir().unwrap();
    let url = server(dir.path()).await;
    let client = reqwest::Client::new();

    let response = client
        .post(&url)
        .header("authorization", format!("Bearer {TOKEN}"))
        .header("accept", "application/json, text/event-stream")
        .json(&initialize())
        .send()
        .await
        .unwrap();

    assert!(
        response.status().is_success(),
        "poignée de main refusée : {}",
        response.status()
    );

    let body = response.text().await.unwrap();
    assert!(
        body.contains("safe-invest"),
        "le serveur ne s'est pas nommé : {body}"
    );
}

/// The handshake is the easy half. This proves a session survives it: the
/// server hands back a session id, and a second call on that id reaches the
/// tools — which is what an actual client does, and what would break quietly.
#[tokio::test(flavor = "multi_thread")]
async fn a_session_survives_the_handshake_and_lists_the_tools() {
    let dir = tempfile::tempdir().unwrap();
    let url = server(dir.path()).await;
    let client = reqwest::Client::new();

    let opened = client
        .post(&url)
        .header("authorization", format!("Bearer {TOKEN}"))
        .header("accept", "application/json, text/event-stream")
        .json(&initialize())
        .send()
        .await
        .unwrap();

    let session = opened
        .headers()
        .get("mcp-session-id")
        .expect("le serveur n'a pas ouvert de session")
        .to_str()
        .unwrap()
        .to_owned();

    // Drain the handshake stream before reusing the session.
    let _ = opened.text().await.unwrap();

    client
        .post(&url)
        .header("authorization", format!("Bearer {TOKEN}"))
        .header("mcp-session-id", &session)
        .header("accept", "application/json, text/event-stream")
        .json(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .send()
        .await
        .unwrap();

    let listed = client
        .post(&url)
        .header("authorization", format!("Bearer {TOKEN}"))
        .header("mcp-session-id", &session)
        .header("accept", "application/json, text/event-stream")
        .json(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
        .send()
        .await
        .unwrap();

    assert!(listed.status().is_success(), "{}", listed.status());
    let body = listed.text().await.unwrap();

    // The same tools stdio serves. A transport that quietly served a subset
    // would be the worst kind of difference between the two doors.
    for tool in safe_invest_mcp::server::TOOL_NAMES {
        assert!(
            body.contains(tool),
            "outil absent de la liste HTTP : {tool}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_without_a_token_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let url = server(dir.path()).await;

    let response = reqwest::Client::new()
        .post(&url)
        .header("accept", "application/json, text/event-stream")
        .json(&initialize())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_with_the_wrong_token_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let url = server(dir.path()).await;

    let response = reqwest::Client::new()
        .post(&url)
        .header("authorization", "Bearer not-the-token")
        .header("accept", "application/json, text/event-stream")
        .json(&initialize())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
}

/// The DNS-rebinding case. A page at `evil.example` that made the name resolve
/// to 127.0.0.1 reaches this server as same-origin, so CORS never gets a say —
/// even holding the token, the Origin has to stop it.
#[tokio::test(flavor = "multi_thread")]
async fn a_browser_origin_is_refused_even_with_the_token() {
    let dir = tempfile::tempdir().unwrap();
    let url = server(dir.path()).await;

    let response = reqwest::Client::new()
        .post(&url)
        .header("authorization", format!("Bearer {TOKEN}"))
        .header("origin", "https://evil.example")
        .header("accept", "application/json, text/event-stream")
        .json(&initialize())
        .send()
        .await
        .unwrap();

    assert!(
        response.status().is_client_error(),
        "une origine étrangère a été acceptée : {}",
        response.status()
    );
}

/// A `Host` naming anything but loopback is the other half of the same attack.
#[tokio::test(flavor = "multi_thread")]
async fn a_foreign_host_header_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let url = server(dir.path()).await;

    let response = reqwest::Client::new()
        .post(&url)
        .header("authorization", format!("Bearer {TOKEN}"))
        .header("host", "evil.example")
        .header("accept", "application/json, text/event-stream")
        .json(&initialize())
        .send()
        .await
        .unwrap();

    assert!(
        response.status().is_client_error(),
        "un Host étranger a été accepté : {}",
        response.status()
    );
}

/// `text/plain` is a simple request: a browser sends it cross-origin with no
/// preflight. It must not be a way in.
#[tokio::test(flavor = "multi_thread")]
async fn a_plain_text_post_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let url = server(dir.path()).await;

    let response = reqwest::Client::new()
        .post(&url)
        .header("authorization", format!("Bearer {TOKEN}"))
        .header("content-type", "text/plain")
        .body(initialize().to_string())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 415);
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_else_is_served_on_the_port() {
    let dir = tempfile::tempdir().unwrap();
    let url = server(dir.path()).await;
    let root = url.replace("/mcp", "/");

    let response = reqwest::Client::new()
        .get(&root)
        .header("authorization", format!("Bearer {TOKEN}"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 404);
}

/// No CORS header, ever. One of these in a response is the single change that
/// would undo every other check in front of this port.
#[tokio::test(flavor = "multi_thread")]
async fn no_response_ever_grants_a_browser_permission() {
    let dir = tempfile::tempdir().unwrap();
    let url = server(dir.path()).await;

    for response in [
        reqwest::Client::new()
            .post(&url)
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("accept", "application/json, text/event-stream")
            .json(&initialize())
            .send()
            .await
            .unwrap(),
        reqwest::Client::new().post(&url).send().await.unwrap(),
    ] {
        let headers = response.headers();
        assert!(
            headers.get("access-control-allow-origin").is_none(),
            "une réponse autorise une origine"
        );
        assert!(
            headers.get("access-control-allow-credentials").is_none(),
            "une réponse autorise des identifiants"
        );
    }
}

/// Switching the port off, moving it, or regenerating the token all come down
/// to dropping the server. A connection that outlived it would keep answering
/// with the old token — so an open one has to close with it.
#[tokio::test(flavor = "multi_thread")]
async fn stopping_the_server_closes_the_connections_already_open() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let dir = tempfile::tempdir().unwrap();
    let context = Context::new(&ContextConfig {
        data_dir: Some(dir.path().to_path_buf()),
        force_simulated: true,
    })
    .unwrap();
    let listener = http::bind(0).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(http::serve(listener, context, TOKEN.to_owned()));

    // A keep-alive connection that has had one answer and stays open.
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    stream
        .write_all(b"GET /ailleurs HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .await
        .unwrap();
    let mut answer = Vec::new();
    let mut chunk = [0_u8; 512];
    while !String::from_utf8_lossy(&answer).contains("Not found") {
        let read = tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .expect("pas de réponse")
            .unwrap();
        assert!(read > 0, "la connexion s'est fermée avant de répondre");
        answer.extend_from_slice(&chunk[..read]);
    }

    server.abort();
    let _ = server.await;

    let after = tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut chunk))
        .await
        .expect("la connexion est restée ouverte après l'arrêt du serveur");
    assert!(
        matches!(after, Ok(0) | Err(_)),
        "la connexion répond encore : {after:?}"
    );
}
