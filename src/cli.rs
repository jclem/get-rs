use std::str::FromStr;

use anyhow::{bail, Result};
use clap::Parser;
use http::{HeaderMap, HeaderName, HeaderValue};
use reqwest::RequestBuilder;
use serde_json::json;
use url;

use crate::config::Config;
use crate::json_builder;
use crate::parser::{BodyValue, ParsedRequest};
use crate::session::Session;
use crate::url_builder::URLBuilder;

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

    let mut url_builder = URLBuilder::from_input(&cli.url, &config.fallback_hostname)?;

    let authority = url_builder.authority()?;
    let session = Session::load(&authority).await?.unwrap_or_default();

    if url_builder.scheme == None {
        let hostname = url_builder
            .hostname
            .as_ref()
            .expect("hostname parsed from request");

        url_builder.scheme = Some(get_scheme(hostname, &session, &config.http_hostnames));
    }

    build_request(
        &mut url_builder,
        &session,
        &cli.components,
        cli.data.clone(),
    )?
    .send()
    .await?;

    Ok(())
}

fn build_request(
    url_builder: &mut URLBuilder,
    session: &Session,
    inputs: &[String],
    data: Option<String>,
) -> Result<RequestBuilder> {
    let parsed_request = ParsedRequest::from_inputs(inputs)?;

    if parsed_request.query.len() > 0 {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());

        for (name, value) in parsed_request.query.iter() {
            serializer.append_pair(name, value);
        }

        url_builder.query = match url_builder.query.as_ref() {
            Some(query) if !query.is_empty() => Some(format!("{}&{}", query, serializer.finish())),
            _ => Some(serializer.finish()),
        }
    }

    let url = url_builder.build()?;

    let client = reqwest::Client::new();
    let mut req = client.get(url);

    if let Some(headers) = session.headers.as_ref() {
        let mut headers_map = HeaderMap::new();
        for (name, values) in headers {
            for value in values {
                let name = HeaderName::from_str(name)?;
                let value = HeaderValue::from_str(value)?;
                headers_map.append(name, value);
            }
        }

        if !headers_map.is_empty() {
            req = req.headers(headers_map);
        }
    }

    let mut headers_map = HeaderMap::new();
    for (name, value) in parsed_request.headers.iter() {
        let name = HeaderName::from_str(name)?;
        let value = HeaderValue::from_str(value)?;
        headers_map.append(name, value);
    }

    if !headers_map.is_empty() {
        req = req.headers(headers_map);
    }

    if let Some(data) = data {
        if !parsed_request.body.is_empty() {
            bail!("Cannot specify both body data and body values");
        }

        req = req.body(data);
    }

    if !parsed_request.body.is_empty() {
        let mut root = json!(null);

        for value in parsed_request.body.iter() {
            match value {
                BodyValue::String { path, value } => {
                    json_builder::put_value(&mut root, path, json!(value))?;
                }

                BodyValue::JSON { path, value } => {
                    let value = serde_json::from_str(value)?;
                    json_builder::put_value(&mut root, path, value)?;
                }
            }
        }

        req = req.body(root.to_string());
    }

    Ok(req)
}

fn get_scheme(hostname: &str, session: &Session, fallback_hostnames: &[String]) -> String {
    if let Some(scheme) = session.scheme.clone() {
        return scheme.as_str().to_string();
    }

    if fallback_hostnames.iter().any(|h| h == hostname) {
        "http".to_string()
    } else {
        "https".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::{HeaderMap, HeaderName, HeaderValue};
    use reqwest::Request;
    use std::str::FromStr;

    #[test]
    fn parse_simple_header() {
        let components = vec!["foo:bar".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();

        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();

        let headers = vec![(
            HeaderName::from_str("foo").unwrap(),
            HeaderValue::from_str("bar").unwrap(),
        )];

        assert_eq!(request.headers(), &HeaderMap::from_iter(headers));
    }

    #[test]
    fn parse_quoted_header() {
        let components = vec!["foo:bar baz".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();

        let headers = vec![(
            HeaderName::from_str("foo").unwrap(),
            HeaderValue::from_str("bar baz").unwrap(),
        )];

        assert_eq!(request.headers(), &HeaderMap::from_iter(headers));
    }

    #[test]
    fn reject_bad_header() {
        let components = vec!["foo bar:baz".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let error =
            build_request(&mut url_builder, &Session::new(), &components, None).unwrap_err();
        assert_eq!(error.to_string(), "Invalid request component");
    }

    #[test]
    fn parse_simple_query_param() {
        let components = vec!["foo==bar".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(request.url().to_string(), "http://example.com/?foo=bar");
    }

    #[test]
    fn parse_quoted_query_param() {
        let components = vec!["foo bar==baz qux".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();

        assert_eq!(
            request.url().to_string(),
            "http://example.com/?foo+bar=baz+qux"
        );
    }

    #[test]
    fn parse_simple_body_param() {
        let components = vec!["foo=bar".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"{"foo":"bar"}"#)
    }

    #[test]
    fn parse_nested_body_param() {
        let components = vec!["foo[bar]=baz".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"{"foo":{"bar":"baz"}}"#)
    }

    #[test]
    fn parse_flexible_object_key_body_param() {
        let components = vec!["foo[bar]baz.qux=quux".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request_body(&request),
            r#"{"foo":{"bar":{"baz":{"qux":"quux"}}}}"#
        )
    }

    #[test]
    fn parse_flexible_array_index_body_param() {
        let components = vec!["foo[bar]0.qux=quux".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request_body(&request),
            r#"{"foo":{"bar":[{"qux":"quux"}]}}"#
        )
    }

    #[test]
    fn parse_flexible_leading_body_param() {
        let components = vec!["[foo][bar]=baz".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"{"foo":{"bar":"baz"}}"#)
    }

    #[test]
    fn parse_multi_nested_body_param() {
        let components = vec!["foo[bar][baz][qux]=quux".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request_body(&request),
            r#"{"foo":{"bar":{"baz":{"qux":"quux"}}}}"#
        )
    }

    #[test]
    fn parse_array_end_body_param() {
        let components = vec!["[]=foo".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"["foo"]"#)
    }

    #[test]
    fn parse_nested_array_end_body_param() {
        let components = vec!["foo[][]=bar".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"{"foo":[["bar"]]}"#)
    }

    #[test]
    fn parse_array_index_body_param() {
        let components = vec!["[1]=foo".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"[null,"foo"]"#)
    }

    #[test]
    fn parse_nested_array_index_body_param() {
        let components = vec!["foo[0][1]=bar".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"{"foo":[[null,"bar"]]}"#)
    }

    #[test]
    fn parse_mixed_body_param() {
        let components = vec!["[][foo][bar][][1][baz]=qux".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request_body(&request),
            r#"[{"foo":{"bar":[[null,{"baz":"qux"}]]}}]"#
        )
    }

    #[test]
    fn parse_overwrite_values() {
        let components = vec![
            "a[b]=c".to_string(),
            "a[b]=d".to_string(),
            "a[e]f[]=g".to_string(),
            "a[e]f.0=h".to_string(),
        ];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"{"a":{"b":"d","e":{"f":["h"]}}}"#)
    }

    #[test]
    fn parse_multiple_mixed_body_params() {
        let components = vec![
            "a[b]=c".to_string(),
            "a[d]=e".to_string(),
            "a[f][]=g".to_string(),
            "a[f][1]=h".to_string(),
            "a[f][2][i]=j".to_string(),
        ];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            request_body(&request),
            r#"{"a":{"b":"c","d":"e","f":["g","h",{"i":"j"}]}}"#
        )
    }

    #[test]
    fn parse_mixed_type_overlapping_body_params() {
        let components = vec!["a[b]=c".to_string(), "a[b][]=e".to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let error =
            build_request(&mut url_builder, &Session::new(), &components, None).unwrap_err();
        assert_eq!(error.to_string(), "expect array root");
    }

    #[test]
    fn parse_raw_json_string_body_param() {
        let components = vec![r#"foo:="bar""#.to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"{"foo":"bar"}"#)
    }

    #[test]
    fn parse_raw_json_int_body_param() {
        let components = vec![r#"foo:=1"#.to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"{"foo":1}"#)
    }

    #[test]
    fn parse_raw_json_null_body_param() {
        let components = vec![r#"foo:=null"#.to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"{"foo":null}"#)
    }

    #[test]
    fn parse_raw_json_map_body_param() {
        let components = vec![r#"foo:={"bar":"baz"}"#.to_string()];
        let mut url_builder = URLBuilder::from_input("http://example.com", "localhost").unwrap();
        let request = build_request(&mut url_builder, &Session::new(), &components, None)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(request_body(&request), r#"{"foo":{"bar":"baz"}}"#)
    }

    fn request_body(req: &Request) -> String {
        String::from_utf8(req.body().unwrap().as_bytes().unwrap().to_owned()).unwrap()
    }
}
