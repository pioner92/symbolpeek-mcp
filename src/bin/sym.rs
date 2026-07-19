#[tokio::main]
async fn main() -> anyhow::Result<()> {
    symbolpeek::cli::run().await
}
