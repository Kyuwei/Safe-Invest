//! Drives the shipped binary as a real MCP client would.
//!
//! This is the test the previous version of the project never had: it launches
//! `safe-invest mcp` as a separate process, speaks JSON-RPC over its pipes, and
//! plays a whole round. If the executable cannot start, or the protocol
//! handshake is wrong, or a tool answers something the schema does not allow,
//! it fails here rather than on someone's machine.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::needless_pass_by_value,
    clippy::too_many_lines,
    reason = "a test that trips is a test that failed"
)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A server that never answers would block `read_line` forever, and a hung test
/// sits on a CI runner until the job's own timeout. This bounds it.
const WATCHDOG: std::time::Duration = std::time::Duration::from_secs(60);

const PROTOCOL_VERSION: &str = "2025-06-18";

/// A live MCP server, speaking over its own stdin and stdout.
struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
    finished: Arc<AtomicBool>,
}

impl Session {
    fn start(data_dir: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_safe-invest"))
            .arg("mcp")
            // `--demo` keeps the whole test offline: prices come from the
            // simulator, so it never depends on a live API or a rate limit.
            .arg("--demo")
            .arg("--data-dir")
            .arg(data_dir)
            .env("SAFEINVEST_LOG", "error")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("le binaire safe-invest doit démarrer");

        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());

        // Killing the child unblocks any read waiting on its output, which
        // turns a hang into an ordinary test failure.
        let finished = Arc::new(AtomicBool::new(false));
        let watched = Arc::clone(&finished);
        let pid = child.id();
        std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + WATCHDOG;
            while std::time::Instant::now() < deadline {
                if watched.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            // `Child::kill` needs the handle the test still owns, so go through
            // the platform's own command instead.
            let _ = kill(pid);
        });

        let mut session = Self {
            child,
            stdin,
            stdout,
            next_id: 0,
            finished,
        };
        session.handshake();
        session
    }

    fn handshake(&mut self) {
        let result = self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "safe-invest-smoke-test", "version": "1.0" }
            }),
        );

        assert_eq!(result["serverInfo"]["name"], "safe-invest");
        assert!(
            result["instructions"]
                .as_str()
                .unwrap_or_default()
                .contains("rationale"),
            "les instructions doivent annoncer la règle de justification"
        );

        self.notify("notifications/initialized", &json!({}));
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").expect("écriture sur stdin du serveur");
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: &Value) {
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.send(&message);
    }

    /// Sends a request and returns its `result`, panicking on a protocol error.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let response = self.raw_request(method, &params);
        assert!(
            response.get("error").is_none(),
            "erreur de protocole sur {method} : {response}"
        );
        response["result"].clone()
    }

    fn raw_request(&mut self, method: &str, params: &Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));

        // Skip any notification the server sends in between.
        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .expect("lecture de la réponse");
            assert!(
                read > 0,
                "le serveur a fermé sa sortie avant de répondre à {method}"
            );

            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                panic!("ligne non-JSON sur stdout, le flux est corrompu : {line}");
            };
            if message.get("id").and_then(Value::as_i64) == Some(id) {
                return message;
            }
        }
    }

    /// Calls a tool and returns its structured content.
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        assert_ne!(
            result.get("isError").and_then(Value::as_bool),
            Some(true),
            "l'outil {name} a échoué : {result}"
        );
        result
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| result["content"][0]["text"].clone())
    }

    /// Calls a tool expecting it to be refused, and returns the error message.
    fn call_expecting_refusal(&mut self, name: &str, arguments: Value) -> String {
        let response = self.raw_request(
            "tools/call",
            &json!({ "name": name, "arguments": arguments }),
        );

        if let Some(error) = response.get("error") {
            return error["message"].as_str().unwrap_or_default().to_owned();
        }
        let result = &response["result"];
        assert_eq!(
            result.get("isError").and_then(Value::as_bool),
            Some(true),
            "l'outil {name} aurait dû refuser : {result}"
        );
        result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.finished.store(true, Ordering::SeqCst);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(windows)]
fn kill(pid: u32) -> std::io::Result<()> {
    Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| ())
}

#[cfg(not(windows))]
fn kill(pid: u32) -> std::io::Result<()> {
    Command::new("kill")
        .args(["-9", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| ())
}

#[test]
fn an_ai_can_play_a_whole_round_through_the_shipped_binary() {
    let dir = tempfile::tempdir().unwrap();
    let mut mcp = Session::start(dir.path());

    // --- the fourteen tools are announced -------------------------------
    let tools = mcp.request("tools/list", json!({}));
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    for expected in [
        "list_games",
        "create_game",
        "open_game",
        "get_portfolio",
        "set_goal",
        "get_goal_progress",
        "get_trade_history",
        "get_market_sources",
        "search_assets",
        "list_popular_assets",
        "get_quotes",
        "get_price_history",
        "buy",
        "sell",
    ] {
        assert!(names.contains(&expected), "outil manquant : {expected}");
    }

    // --- start an AI game with a goal ------------------------------------
    let game = mcp.call(
        "create_game",
        json!({
            "player_name": "Claude",
            "player_kind": "ai",
            "starting_cash": 10000,
            "currency": "EUR",
            "target_amount": 15000,
            "deadline": "2027-12-31"
        }),
    );
    assert_eq!(game["playerKind"], "ai");
    assert_eq!(game["rationaleRequired"], true);

    // --- a quote, and it admits it is simulated ---------------------------
    let quotes = mcp.call(
        "get_quotes",
        json!({ "symbols": ["BTC"], "kind": "crypto" }),
    );
    let btc = &quotes["quotes"][0];
    assert_eq!(btc["symbol"], "BTC");
    assert_eq!(btc["available"], true);
    assert_eq!(
        btc["isSimulated"], true,
        "en mode démo, le cours doit être signalé comme simulé"
    );

    // --- the rule that makes AI mode worth having -------------------------
    let refusal = mcp.call_expecting_refusal(
        "buy",
        json!({ "symbol": "BTC", "kind": "crypto", "amount": 1000 }),
    );
    assert!(
        refusal.contains("justification"),
        "un achat sans justification doit être refusé : {refusal}"
    );

    // --- a justified buy --------------------------------------------------
    let bought = mcp.call(
        "buy",
        json!({
            "symbol": "BTC",
            "kind": "crypto",
            "amount": 3000,
            "rationale": "Ouverture d'une position crypto limitée à 30 % du capital."
        }),
    );
    assert_eq!(bought["side"], "buy");
    assert_eq!(
        bought["rationale"],
        "Ouverture d'une position crypto limitée à 30 % du capital."
    );

    // --- the portfolio reflects it ----------------------------------------
    let portfolio = mcp.call("get_portfolio", json!({}));
    assert_eq!(portfolio["positions"].as_array().unwrap().len(), 1);
    assert_eq!(portfolio["containsSimulatedPrices"], true);
    assert_eq!(portfolio["currency"], "EUR");

    // --- the goal is scored ------------------------------------------------
    let goal = mcp.call("get_goal_progress", json!({}));
    assert_eq!(goal["targetAmount"], "15000");
    assert!(goal["daysRemaining"].as_i64().unwrap() > 0);

    // --- selling everything closes the position ----------------------------
    let sold = mcp.call(
        "sell",
        json!({
            "symbol": "BTC",
            "kind": "crypto",
            "all": true,
            "rationale": "Prise de bénéfices : l'objectif de court terme est atteint."
        }),
    );
    assert_eq!(sold["side"], "sell");
    assert!(sold["realizedPnl"].is_string());

    let after = mcp.call("get_portfolio", json!({}));
    assert!(
        after["positions"].as_array().unwrap().is_empty(),
        "vendre tout doit solder la position"
    );

    // --- the history keeps both justifications ------------------------------
    let history = mcp.call("get_trade_history", json!({}));
    let trades = history["trades"].as_array().unwrap();
    assert_eq!(trades.len(), 2);
    assert!(
        trades.iter().all(|t| t["rationale"].is_string()),
        "chaque opération d'une IA doit rester justifiée dans l'historique"
    );
    // Newest first.
    assert_eq!(trades[0]["side"], "sell");
}

#[test]
fn the_server_refuses_an_ambiguous_order_rather_than_guessing() {
    let dir = tempfile::tempdir().unwrap();
    let mut mcp = Session::start(dir.path());

    mcp.call(
        "create_game",
        json!({ "player_name": "Léa", "player_kind": "human", "starting_cash": 1000 }),
    );

    let both = mcp.call_expecting_refusal(
        "buy",
        json!({ "symbol": "BTC", "kind": "crypto", "quantity": 1, "amount": 500 }),
    );
    assert!(both.contains("une seule"), "{both}");

    let neither = mcp.call_expecting_refusal("buy", json!({ "symbol": "BTC", "kind": "crypto" }));
    assert!(neither.contains("quantity"), "{neither}");
}

#[test]
fn acting_with_no_game_open_says_what_to_do_next() {
    let dir = tempfile::tempdir().unwrap();
    let mut mcp = Session::start(dir.path());

    let message = mcp.call_expecting_refusal("get_portfolio", json!({}));
    assert!(message.contains("Aucune partie"), "{message}");
}

#[test]
fn a_human_game_needs_no_justification() {
    let dir = tempfile::tempdir().unwrap();
    let mut mcp = Session::start(dir.path());

    mcp.call(
        "create_game",
        json!({ "player_name": "Léa", "player_kind": "human", "starting_cash": 5000 }),
    );
    let bought = mcp.call(
        "buy",
        json!({ "symbol": "ETH", "kind": "crypto", "amount": 500 }),
    );

    assert_eq!(bought["side"], "buy");
    assert!(bought["rationale"].is_null());
}

#[test]
fn the_market_sources_report_names_the_simulator_in_demo_mode() {
    let dir = tempfile::tempdir().unwrap();
    let mut mcp = Session::start(dir.path());

    let sources = mcp.call("get_market_sources", json!({}));
    let list = sources["sources"].as_array().unwrap();

    let simulator = list.iter().find(|s| s["id"] == "simulated").unwrap();
    assert_eq!(simulator["isSimulated"], true);
    assert!(list.len() >= 5, "toutes les sources doivent être listées");
}

#[test]
fn amounts_may_be_written_as_numbers_or_as_strings() {
    let dir = tempfile::tempdir().unwrap();
    let mut mcp = Session::start(dir.path());

    mcp.call(
        "create_game",
        json!({ "player_name": "Léa", "player_kind": "human", "starting_cash": "10000.50" }),
    );
    let portfolio = mcp.call("get_portfolio", json!({}));

    assert_eq!(portfolio["startingCash"], "10000.50");
}
