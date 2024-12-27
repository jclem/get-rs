use anyhow::Result;

mod cli;
mod config;
mod json_builder;
mod parser;
mod request_builder;
mod session;
mod url_builder;

#[tokio::main]
async fn main() -> Result<()> {
    cli::run().await
}
