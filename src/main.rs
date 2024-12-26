use anyhow::{Context, Result};
use std::{
    env,
    path::{Path, PathBuf},
};

use crate::config::Config;

mod cli;
mod config;
mod json_builder;
mod parser;
mod session;
mod url_builder;

#[tokio::main]
async fn main() -> Result<()> {
    let config = load_config().await?;
    let args = env::args();
    cli::run(args, &config).await
}

async fn load_config() -> Result<Config> {
    let config_path = get_config_home()?
        .join("get")
        .join("config.json")
        .to_str()
        .context("valid config path")?
        .to_string();

    Config::load(&config_path).await
}

fn get_config_home() -> Result<PathBuf> {
    match env::var("XDG_CONFIG_HOME") {
        Ok(path) => Ok(Path::new(&path).to_path_buf()),
        Err(_) => Ok(homedir::my_home()?.context("home dir")?.join(".config")),
    }
}
