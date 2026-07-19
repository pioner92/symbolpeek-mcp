use anyhow::Result;
use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() -> Result<()> {
    let mut arguments = std::env::args().skip(1);
    if arguments.next().as_deref() == Some("stats") {
        let reset_lifetime = arguments.any(|argument| argument == "--reset");
        codescope::server::print_cli_statistics(reset_lifetime);
        return Ok(());
    }

    let service = codescope::server::CodeScopeServer::new()
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
