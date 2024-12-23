use anyhow::Result;

mod cli;
mod json_builder;
mod parser;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
