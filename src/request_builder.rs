use std::str::FromStr;

use anyhow::{bail, Context, Result};
use http::{
    header::{Entry, OccupiedEntry},
    HeaderMap, HeaderName, HeaderValue, Method, Version,
};
use reqwest::Response;

use crate::{
    config::Config, json_builder, parser::BodyValue, session::Session, url_builder::URLBuilder,
};

/// Wraps a reqwest::RequestBuilder to provide additional functionality by
/// parsing user input and stored sessions
pub struct RequestBuilder {
    pub url: URLBuilder,
    pub headers: HeaderMap,
    pub body: Option<String>,
    pub version: Version,
}

impl RequestBuilder {
    /// Creates a new RequestBuilder from a URL and configuration object
    pub async fn from_input(url: &str, config: &Config) -> Result<Self> {
        let mut url = URLBuilder::from_input(&url, &config.fallback_hostname)?;
        let authority = url.authority().context("URL has authority")?;
        let session = Session::load(&authority).await?.unwrap_or_default();

        if url.scheme == None {
            let hostname = url.hostname.as_ref().context("hostname parsed")?;
            url.scheme = Some(get_scheme(hostname, &session, &config.http_hostnames))
        }

        let mut header_map = HeaderMap::new();
        let mut host = url.hostname.clone().context("hostname parsed")?;
        if let Some(port) = &url.port {
            host = format!("{}:{}", host, port);
        }

        let host_header = HeaderValue::from_str(&host)?;
        header_map.append("Host", host_header);

        if let Some(headers) = session.headers.as_ref() {
            for (key, values) in headers {
                for value in values {
                    add_header(&mut header_map, key, value)?;
                }
            }
        }

        Ok(Self {
            url,
            headers: header_map,
            body: None,
            version: Version::default(),
        })
    }

    /// Adds the given query parameters to the request
    pub fn add_query(mut self, query: &[(String, String)]) -> Self {
        if query.is_empty() {
            return self;
        }

        let mut serializer = url::form_urlencoded::Serializer::new(String::new());

        for (key, value) in query {
            serializer.append_pair(key, value);
        }

        self.url.query = match &self.url.query {
            Some(query) if !query.is_empty() => Some(format!("{}&{}", query, serializer.finish())),
            _ => Some(serializer.finish()),
        };

        return self;
    }

    /// Merges the given headers into the request
    pub fn merge_headers(mut self, headers: HeaderMap) -> Result<Self> {
        if headers.is_empty() {
            return Ok(self);
        }

        replace_headers(&mut self.headers, headers);

        Ok(self)
    }

    /// Adds data to the request body
    pub fn add_data(mut self, values: &[BodyValue], data: Option<&str>) -> Result<Self> {
        if data.is_some() && !values.is_empty() {
            bail!("Cannot specify both data and body values");
        }

        if let Some(data) = data {
            self.body = Some(data.to_owned());
        }

        if !values.is_empty() {
            self.body = Some(json_builder::build(values)?);
        }

        Ok(self)
    }

    /// Sets the HTTP version of the request
    pub fn version(mut self, version: Version) -> Self {
        self.version = version;
        self
    }

    /// Sends the request
    pub async fn send(&mut self, method: Method) -> Result<Response> {
        let client = reqwest::Client::new();
        let mut request = client
            .request(method, self.url.build()?)
            .version(self.version);
        request = request.headers(self.headers.clone());

        let body = self.body.take();

        if let Some(body) = body {
            request = request.body(body);
        }

        let response = request.send().await?;

        Ok(response)
    }
}

fn get_scheme(hostname: &str, session: &Session, http_hostnames: &[String]) -> String {
    if let Some(scheme) = &session.scheme {
        return scheme.as_str().to_string();
    }

    if http_hostnames.contains(&hostname.to_string()) {
        return "http".to_string();
    } else {
        return "https".to_string();
    }
}

fn add_header(map: &mut HeaderMap, key: &str, value: &str) -> Result<()> {
    let key = HeaderName::from_str(key).context("valid header name")?;
    let value = HeaderValue::from_str(value).context("valid header value")?;
    map.append(key, value);
    Ok(())
}

// Taken vebatim from reqwest::util.
fn replace_headers(dst: &mut HeaderMap, src: HeaderMap) {
    // IntoIter of HeaderMap yields (Option<HeaderName>, HeaderValue).
    // The first time a name is yielded, it will be Some(name), and if
    // there are more values with the same name, the next yield will be
    // None.

    let mut prev_entry: Option<OccupiedEntry<_>> = None;
    for (key, value) in src {
        match key {
            Some(key) => match dst.entry(key) {
                Entry::Occupied(mut e) => {
                    e.insert(value);
                    prev_entry = Some(e);
                }
                Entry::Vacant(e) => {
                    let e = e.insert_entry(value);
                    prev_entry = Some(e);
                }
            },
            None => match prev_entry {
                Some(ref mut entry) => {
                    entry.append(value);
                }
                None => unreachable!("HeaderMap::into_iter yielded None first"),
            },
        }
    }
}
