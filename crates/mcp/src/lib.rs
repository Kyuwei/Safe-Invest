//! The Model Context Protocol face of Safe Invest.
//!
//! It is a thin shell over `safe-invest-service`: parse the arguments, call the
//! operation, render the answer. Nothing about the rules of the game lives
//! here, which is the point — an AI cannot be given a rule a person does not
//! also play by.

pub mod params;
pub mod server;

pub use server::SafeInvestServer;

use rmcp::ServiceExt;
use rmcp::transport::stdio;
use safe_invest_service::Context;

/// Serves the tools over stdin/stdout until the client disconnects.
///
/// Nothing may be written to stdout but protocol messages — a stray `println!`
/// corrupts the stream and the client simply stops answering. Every log line in
/// this process therefore goes to stderr.
pub async fn serve_stdio(context: Context) -> anyhow::Result<()> {
    let server = SafeInvestServer::new(context);
    let running = server.serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}
