use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    env,
    io::ErrorKind,
    path::{Path, PathBuf},
};
use tokio::{fs::File, io::AsyncReadExt};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    HTTP,
    HTTPS,
}

impl Scheme {
    pub fn as_str(&self) -> &str {
        match self {
            Self::HTTP => "http",
            Self::HTTPS => "https",
        }
    }
}

/// A saved set of configuration for making requests against a given authority
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    /// The headers to include in the request
    ///
    /// A header can have more than one value, so we use a `Vec` to store them.
    pub headers: Option<HashMap<String, Vec<String>>>,

    /// The scheme to use when making requests
    pub scheme: Option<Scheme>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// Creates a new session with the default scheme and an empty set of
    /// headers
    pub fn new() -> Self {
        Self {
            headers: None,
            scheme: None,
        }
    }

    /// Loads a session for the given authority
    pub async fn load(authority: &str) -> Result<Option<Self>> {
        let store = SessionStore::load().await?;
        Ok(store.get(authority).cloned())
    }
}

/// A map of URL authorities to their respective session configurations
#[derive(Serialize, Deserialize)]
struct SessionStore(HashMap<String, Session>);

impl SessionStore {
    fn get(&self, authority: &str) -> Option<&Session> {
        self.0.get(authority)
    }

    async fn load() -> Result<SessionStore> {
        let sessions_path = get_data_home()?.join("get").join("sessions.json");

        match File::open(sessions_path).await {
            Ok(mut file) => {
                let mut dest = Vec::new();
                file.read_to_end(&mut dest).await?;

                let session_store: SessionStore =
                    serde_json::from_slice(&dest).context("parse session store")?;

                Ok(session_store)
            }

            Err(err) if err.kind() == ErrorKind::NotFound => Ok(SessionStore(HashMap::new())),
            Err(err) => Err(err).context("open session store"),
        }
    }
}

fn get_data_home() -> Result<PathBuf> {
    match env::var("XDG_DATA_HOME") {
        Ok(path) => Ok(Path::new(&path).to_path_buf()),
        Err(_) => Ok(homedir::my_home()?
            .context("home dir")?
            .join(".local")
            .join("share")),
    }
}
