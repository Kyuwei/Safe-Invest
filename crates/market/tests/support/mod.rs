//! A throwaway HTTP server for the provider tests.
//!
//! Hand-rolled on a `TcpListener` rather than pulled in as a mock-server crate:
//! the whole need is "answer these paths with this text", and a test dependency
//! is still a dependency to audit.

#![allow(
    dead_code,
    unreachable_pub,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test support code"
)]

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// One canned answer.
#[derive(Clone)]
pub struct Reply {
    pub status: u16,
    pub body: String,
}

impl Reply {
    pub fn ok(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            body: body.into(),
        }
    }

    pub fn status(status: u16) -> Self {
        Self {
            status,
            body: String::new(),
        }
    }
}

pub struct FakeApi {
    pub origin: String,
    hits: Arc<AtomicUsize>,
}

impl FakeApi {
    /// Starts a server answering by path prefix, longest prefix first.
    pub async fn start(routes: HashMap<String, Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));

        let served = hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let routes = routes.clone();
                let served = served.clone();
                tokio::spawn(async move {
                    let mut buffer = vec![0_u8; 8192];
                    let Ok(read) = socket.read(&mut buffer).await else {
                        return;
                    };
                    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                    let path = request
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/")
                        .to_owned();

                    served.fetch_add(1, Ordering::SeqCst);

                    let mut matched: Vec<(&String, &Reply)> = routes
                        .iter()
                        .filter(|(prefix, _)| path.starts_with(prefix.as_str()))
                        .collect();
                    matched.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));

                    let reply = matched
                        .first()
                        .map_or_else(|| Reply::status(404), |(_, reply)| (*reply).clone());

                    let response = format!(
                        "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        reply.status,
                        reply.body.len(),
                        reply.body
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.flush().await;
                });
            }
        });

        Self {
            origin: format!("http://127.0.0.1:{port}"),
            hits,
        }
    }

    /// How many requests the server has answered — used to prove the cache works.
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

pub fn routes(pairs: &[(&str, Reply)]) -> HashMap<String, Reply> {
    pairs
        .iter()
        .map(|(path, reply)| ((*path).to_owned(), reply.clone()))
        .collect()
}
