use std::{
    env,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::{fs::File, io::AsyncReadExt};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub fallback_hostname: String,
    pub http_hostnames: Vec<String>,
}

impl Config {
    pub fn default() -> Self {
        Config {
            fallback_hostname: "localhost".to_string(),
            http_hostnames: vec!["localhost".to_string()],
        }
    }

    pub async fn load() -> Result<Self> {
        let path = Self::get_config_home()?
            .join("get")
            .join("config.json")
            .to_str()
            .context("valid config path")?
            .to_string();

        match File::open(path).await {
            Ok(mut file) => {
                let mut dest = Vec::new();
                file.read_to_end(&mut dest).await?;

                let config_file: ConfigFile =
                    serde_json::from_slice(&dest).context("parse config file")?;

                Ok(Config {
                    fallback_hostname: config_file
                        .fallback_hostname
                        .unwrap_or("localhost".to_string()),

                    http_hostnames: config_file
                        .http_hostnames
                        .unwrap_or(vec!["localhost".to_string()]),
                })
            }
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err).context("open config file"),
        }
    }

    fn get_config_home() -> Result<PathBuf> {
        match env::var("XDG_CONFIG_HOME") {
            Ok(path) => Ok(Path::new(&path).to_path_buf()),
            Err(_) => Ok(homedir::my_home()?.context("home dir")?.join(".config")),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    fallback_hostname: Option<String>,
    http_hostnames: Option<Vec<String>>,
}
