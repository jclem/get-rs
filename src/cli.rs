use std::str::FromStr;

use anyhow::{bail, Result};
use clap::Parser;
use http::{Method, Version};
use reqwest::Response;

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
        help = "Path to the config file [default: $XDG_CONFIG_HOME/get/config.json]"
    )]
    config: Option<String>,

    #[arg(short, long, help = "Data to send in the request body")]
    data: Option<String>,

    #[arg(
        short = 'X',
        long,
        help = r#"HTTP method to use [default: GET, POST with data]"#
    )]
    method: Option<String>,

    #[arg(long, help = "Use HTTP, regardless of the URL scheme or session")]
    http: bool,

    #[arg(long, help = "Use HTTPS, regardless of the URL scheme or session")]
    https: bool,

    #[arg(short, long, help = "Print verbose output")]
    verbose: bool,

    #[arg(short = 'H', long, help = "Do not print response headers")]
    no_headers: bool,

    #[arg(short = 'B', long, help = "Do not print response body")]
    no_body: bool,

    #[arg(
        long,
        help = "Maximum number of redirects to follow",
        default_value = "10"
    )]
    max_redirects: usize,
}

pub async fn run() -> Result<()> {
    let cli = CLI::parse();

    let config = if let Some(config) = cli.config {
        Config::load_from_path(&config).await?
    } else {
        Config::load().await?
    };

    let parsed_request = ParsedRequest::from_inputs(&cli.components)?;

    if cli.http && cli.https {
        bail!("Cannot specify both --http and --https");
    }

    let scheme = if cli.http {
        Some("http")
    } else if cli.https {
        Some("https")
    } else {
        None
    };

    let mut req = RequestBuilder::from_input(scheme, &cli.url, &config)
        .await?
        .version(Version::default())
        .add_query(&parsed_request.query)
        .merge_headers(parsed_request.headers)?
        .add_data(&parsed_request.body, cli.data.as_ref().map(String::as_ref))?;

    let method = if let Some(method) = cli.method {
        Method::from_str(&method)?
    } else if req.body.is_some() {
        Method::POST
    } else {
        Method::GET
    };

    if cli.verbose {
        print_request(&method, &req)?;
        println!();
    }

    let response = req.send(method, cli.max_redirects).await?;

    print_response(response, !cli.no_headers, !cli.no_body).await?;

    Ok(())
}

fn print_request(method: &Method, req: &RequestBuilder) -> Result<()> {
    let mut path = req.url.path.clone().unwrap_or(String::from("/"));

    if let Some(query) = &req.url.query {
        path.push('?');
        path.push_str(query);
    }

    println!(
        "{}",
        green(&format!("{} {} {:?}", method, path, req.version))
    );

    for (key, value) in req.headers.iter() {
        println!("{} {}", cyan(&format!("{}:", key)), value.to_str()?);
    }

    if let Some(body) = &req.body {
        println!("\n{}", body);
    }

    Ok(())
}

async fn print_response(resp: Response, headers: bool, body: bool) -> Result<()> {
    if headers {
        println!(
            "{}",
            green(&format!("{:?} {}", resp.version(), resp.status()))
        );

        for (key, value) in resp.headers() {
            println!("{} {}", cyan(&format!("{}:", key)), value.to_str()?);
        }

        if body {
            println!();
        }
    }

    if body {
        let body = resp.text().await?;
        println!("{}", body);
    }

    Ok(())
}

fn green(s: &str) -> String {
    format!("\x1b[0;32m{}\x1b[0m", s)
}

fn cyan(s: &str) -> String {
    format!("\x1b[0;36m{}\x1b[0m", s)
}
