use anyhow::Result;
use std::env;

use crate::config::Config;

mod cli;
mod config;
mod json_builder;
mod parser;
mod session;
mod url_builder;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load().await?;
    let args = env::args();

    cli::run(args, &config).await
}
