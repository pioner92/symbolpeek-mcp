use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

/// Runs the `SymbolPeek` CLI entry point.
///
/// # Errors
///
/// Returns an error when the MCP stdio service cannot start or shut down
/// cleanly.
pub async fn run() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next();
    if command.as_deref() == Some("stats") {
        let reset_lifetime = arguments.any(|argument| argument == "--reset");
        crate::server::print_cli_statistics(reset_lifetime);
        return Ok(());
    }
    if matches!(command.as_deref(), Some("--help" | "-h")) {
        print_help();
        return Ok(());
    }

    let service = crate::server::SymbolPeekServer::new()
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

fn print_help() {
    println!(
        "SymbolPeek\n\nUsage:\n  symbolpeek              Start the MCP server\n  symbolpeek stats        Show lifetime statistics\n  symbolpeek stats --reset\n\nAlias:\n  sym                     Equivalent short command name\n\nThe MCP server communicates over stdio."
    );
}
