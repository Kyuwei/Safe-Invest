//! Keeping the MCP port in step with the setting that governs it.
//!
//! The window can serve MCP while it is open, which is the point of the
//! feature: a client that would rather hold a URL than spawn an executable has
//! something to point at as soon as Safe Invest is running. That means the
//! server has to start, stop and move when a person changes their mind in the
//! settings, and it has to say so when the port is already taken.

use safe_invest_service::Context;
use std::sync::Mutex;
use tauri::async_runtime::JoinHandle;

/// What the port is doing right now, for the settings screen to show.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortStatus {
    pub listening: bool,
    pub port: Option<u16>,
    pub url: Option<String>,
    /// Why it is not listening, when it was asked to.
    pub problem: Option<String>,
}

impl PortStatus {
    fn idle() -> Self {
        Self {
            listening: false,
            port: None,
            url: None,
            problem: None,
        }
    }
}

struct Running {
    port: u16,
    task: JoinHandle<()>,
}

impl Drop for Running {
    fn drop(&mut self) {
        // Dropping the task drops the listener with it, which is what actually
        // releases the port — otherwise turning the setting off would leave it
        // held until the window closed.
        self.task.abort();
    }
}

/// The supervisor. One per window, handed to Tauri as managed state.
#[derive(Default)]
pub struct McpPort {
    running: Mutex<Option<Running>>,
    problem: Mutex<Option<String>>,
}

impl McpPort {
    /// Brings the server into line with what the settings ask for.
    ///
    /// Safe to call on every settings save: already serving the right port is
    /// a no-op, a different port is a move, and disabled is a stop.
    pub async fn reconcile(&self, context: &Context) -> PortStatus {
        let settings = context.settings();

        if !settings.mcp_http_enabled {
            self.stop();
            return self.status();
        }

        if self.current_port() == Some(settings.mcp_http_port) {
            return self.status();
        }

        self.stop();

        let token = match context.settings_service().ensure_mcp_token() {
            Ok(token) => token,
            Err(error) => {
                self.fail(format!("jeton MCP indisponible : {error}"));
                return self.status();
            }
        };

        let port = settings.mcp_http_port;
        match safe_invest_mcp::http::bind(port).await {
            Ok(listener) => {
                let task = tauri::async_runtime::spawn(safe_invest_mcp::http::serve(
                    listener,
                    context.clone(),
                    token,
                ));
                tracing::info!(port, "serveur MCP à l'écoute sur 127.0.0.1");
                self.set_running(Running { port, task });
            }
            Err(error) => {
                // The person is looking at the settings screen right now. Say
                // which port failed and why, not "erreur".
                self.fail(format!(
                    "impossible d'écouter sur 127.0.0.1:{port} ({error}). \
                     Un autre programme utilise sans doute ce port."
                ));
            }
        }

        self.status()
    }

    pub fn status(&self) -> PortStatus {
        let port = self.current_port();
        let problem = self.problem.lock().ok().and_then(|p| p.clone());

        match port {
            Some(port) => PortStatus {
                listening: true,
                port: Some(port),
                url: Some(format!("http://127.0.0.1:{port}/mcp")),
                problem: None,
            },
            None => PortStatus {
                problem,
                ..PortStatus::idle()
            },
        }
    }

    fn current_port(&self) -> Option<u16> {
        self.running.lock().ok()?.as_ref().map(|r| r.port)
    }

    fn set_running(&self, running: Running) {
        if let Ok(mut slot) = self.running.lock() {
            *slot = Some(running);
        }
        self.clear_problem();
    }

    /// Drops the running server so the next `reconcile` starts a fresh one.
    pub fn stop_for_restart(&self) {
        self.stop();
    }

    fn stop(&self) {
        if let Ok(mut slot) = self.running.lock() {
            // The `Drop` above aborts the task and frees the port.
            *slot = None;
        }
        self.clear_problem();
    }

    fn clear_problem(&self) {
        if let Ok(mut problem) = self.problem.lock() {
            *problem = None;
        }
    }

    fn fail(&self, message: String) {
        tracing::warn!(%message, "serveur MCP non démarré");
        if let Ok(mut problem) = self.problem.lock() {
            *problem = Some(message);
        }
    }
}
