use anyhow::{Context, Result};
use url::Url;

/// A builder for URLs that allows for both reading and writing of URL parts
///
/// We need to be able to read and write URL parts in order to build a partial
/// URL from user input, and then add additional missing portions from session
/// configuration.
pub struct URLBuilder {
    pub scheme: Option<String>,
    pub hostname: Option<String>,
    pub port: Option<String>,
    pub path: Option<String>,
    pub query: Option<String>,
}

impl URLBuilder {
    /// Returns the URL authority
    ///
    /// The authority is the hostname and port, e.g. "example.com:8080"
    ///
    /// If the hostname is missing, we return an error. If the port is missing,
    /// we return just the hostname.
    pub fn authority(&self) -> Result<String> {
        let mut authority = String::new();
        let hostname = self.hostname.as_ref().context("hostname is required")?;

        authority.push_str(hostname);

        if let Some(port) = &self.port {
            authority.push_str(":");
            authority.push_str(port);
        }

        Ok(authority)
    }

    /// Builds the URL from the parts
    ///
    /// If a required component is missing, we return an error.
    pub fn build(&self) -> Result<String> {
        let scheme = self.scheme.as_ref().context("scheme is required")?;
        let authority = self.authority()?;
        let path = self.path.as_ref().context("path is required")?;
        let query = self
            .query
            .as_ref()
            .map(|q| format!("?{}", q))
            .unwrap_or_default();

        Ok(format!("{}://{}{}{}", scheme, authority, path, query))
    }

    /// Creates a new URL builder from user input
    ///
    /// We expect a few forms of URL input from a user:
    /// - A port with an optional path, etc. e.g. ":8080/foo?bar"
    /// - A URL with no scheme, e.g. "example.com/foo?bar"
    /// - A complete URL, e.g. "https://example.com/foo?bar"
    pub fn from_input(input: &str, fallback_hostname: &str) -> Result<Self> {
        let mut builder = Self::new();

        match input {
            input if input.starts_with("http://") || input.starts_with("https://") => {
                let parsed_url = input.parse::<Url>().context("parse URL")?;
                builder.scheme = Some(parsed_url.scheme().to_string());
                builder.hostname = Some(parsed_url.host_str().context("get host")?.to_string());
                builder.port = parsed_url.port().map(|p| p.to_string()).or(None);
                builder.path = Some(parsed_url.path().to_string());
                builder.query = parsed_url.query().map(|q| q.to_string()).or(None);
            }

            input if input.starts_with(":") => {
                let parsed_url = format!("https://{}{}", fallback_hostname, input)
                    .parse::<Url>()
                    .context("parse URL")?;
                builder.hostname = Some(fallback_hostname.to_owned());
                builder.port = parsed_url.port().map(|p| p.to_string()).or(None);
                builder.path = Some(parsed_url.path().to_string());
                builder.query = parsed_url.query().map(|q| q.to_string()).or(None);
            }

            s => {
                let parsed_url = format!("https://{}", s)
                    .parse::<Url>()
                    .context("parse URL")?;
                builder.hostname = Some(parsed_url.host_str().context("get host")?.to_string());
                builder.port = parsed_url.port().map(|p| p.to_string()).or(None);
                builder.path = Some(parsed_url.path().to_string());
                builder.query = parsed_url.query().map(|q| q.to_string()).or(None);
            }
        };

        Ok(builder)
    }

    /// Creates a new empty URL builder
    fn new() -> Self {
        Self {
            scheme: None,
            hostname: None,
            port: None,
            path: None,
            query: None,
        }
    }
}
