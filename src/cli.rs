use anyhow::Result;
use clap::Parser;

use crate::config::Config;
use crate::parser::ParsedRequest;
use crate::request_builder::RequestBuilder;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct CLI {
    url: String,
    components: Vec<String>,

    #[arg(
        long,
        help = "Path to the config file (defaults to $XDG_CONFIG_HOME/get/config.json"
    )]
    config: Option<String>,

    #[arg(short, long, help = "Data to send in the request body")]
    data: Option<String>,
}

pub async fn run() -> Result<()> {
    let cli = CLI::parse();

    let config = if let Some(config) = cli.config {
        Config::load_from_path(&config).await?
    } else {
        Config::load().await?
    };

    let parsed_request = ParsedRequest::from_inputs(&cli.components)?;

    let mut req = RequestBuilder::from_input(&cli.url, &config)
        .await?
        .add_query(&parsed_request.query)
        .merge_headers(parsed_request.headers)?
        .add_data(&parsed_request.body, cli.data.as_ref().map(String::as_ref))?;

    req.send().await?;

    Ok(())
}
