use crate::json_builder;
use crate::parser::{self, Body, RequestComponent};
use anyhow::Result;
use clap::Parser;
use http::uri;
use reqwest::{RequestBuilder, Url};
use serde_json::json;
use url;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct CLI {
    url: String,
    components: Vec<String>,
}

pub async fn run() -> Result<()> {
    let cli = CLI::parse();

    build_request(&cli.url, &cli.components)?.send().await?;

    Ok(())
}

fn build_request(url: &str, components: &[String]) -> Result<RequestBuilder> {
    let url = parse_url(url)?;

    let components = components
        .iter()
        .map(|input| parser::parse_component(&input))
        .collect::<Result<Vec<RequestComponent>>>()?;

    let mut serializer = form_urlencoded::Serializer::new(String::new());

    for component in &components {
        match component {
            RequestComponent::QueryParam { name, value } => {
                serializer.append_pair(&name, &value);
            }
            _ => {}
        }
    }

    let query = match url.query() {
        Some(query) if !query.is_empty() => format!("{}&{}", query, serializer.finish()),
        _ => serializer.finish(),
    };

    let uri = uri::Builder::new()
        .scheme(url.scheme())
        .authority(url.authority())
        .path_and_query(format!("{}?{}", url.path(), query))
        .build()?
        .to_string();

    let client = reqwest::Client::new();
    let mut req = client.get(uri);

    for component in &components {
        match component {
            RequestComponent::Header { name, value } => {
                req = req.header(name, value);
            }
            _ => {}
        }
    }

    let mut root = json!(null);

    for component in &components {
        match component {
            RequestComponent::Body(Body::String { path, value }) => {
                json_builder::put_value(&mut root, path, json!(value))?;
            }
            RequestComponent::Body(Body::JSON { path, value }) => {
                let value = serde_json::from_str(value)?;
                json_builder::put_value(&mut root, path, value)?;
            }
            _ => {}
        }
    }

    req = req.body(root.to_string());

    Ok(req)
}

// We expect a few forms of URL input from a user:
// - A port with an optional path, etc. e.g. ":8080/foo?bar"
// - A URL with no scheme, e.g. "example.com/foo?bar"
// - A complete URL, e.g. "https://example.com/foo?bar"
fn parse_url(uri: &str) -> Result<Url, url::ParseError> {
    match uri {
        s if s.starts_with("http://") || s.starts_with("https://") => s.parse::<Url>(),
        s if s.starts_with(":") => format!("http://localhost{}", s).parse::<Url>(),
        s if s.starts_with("localhost") => format!("http://{}", s).parse::<Url>(),
        s => format!("https://{}", s).parse::<Url>(),
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
        let url = "http://example.com";
        let components = vec!["foo:bar".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();

        let headers = vec![(
            HeaderName::from_str("foo").unwrap(),
            HeaderValue::from_str("bar").unwrap(),
        )];

        assert_eq!(request.headers(), &HeaderMap::from_iter(headers));
    }

    #[test]
    fn parse_quoted_header() {
        let url = "http://example.com";
        let components = vec!["foo:bar baz".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();

        let headers = vec![(
            HeaderName::from_str("foo").unwrap(),
            HeaderValue::from_str("bar baz").unwrap(),
        )];

        assert_eq!(request.headers(), &HeaderMap::from_iter(headers));
    }

    #[test]
    fn reject_bad_header() {
        let url = "http://example.com";
        let components = vec!["foo bar:baz".to_string()];
        let error = build_request(url, &components).unwrap_err();
        assert_eq!(error.to_string(), "Invalid request component");
    }

    #[test]
    fn parse_simple_query_param() {
        let url = "http://example.com";
        let components = vec!["foo==bar".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();

        assert_eq!(request.url().to_string(), "http://example.com/?foo=bar");
    }

    #[test]
    fn parse_quoted_query_param() {
        let url = "http://example.com";
        let components = vec!["foo bar==baz qux".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();

        assert_eq!(
            request.url().to_string(),
            "http://example.com/?foo+bar=baz+qux"
        );
    }

    #[test]
    fn parse_simple_body_param() {
        let url = "http://example.com";
        let components = vec!["foo=bar".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(request_body(&request), r#"{"foo":"bar"}"#)
    }

    #[test]
    fn parse_nested_body_param() {
        let url = "http://example.com";
        let components = vec!["foo[bar]=baz".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(request_body(&request), r#"{"foo":{"bar":"baz"}}"#)
    }

    #[test]
    fn parse_multi_nested_body_param() {
        let url = "http://example.com";
        let components = vec!["foo[bar][baz][qux]=quux".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(
            request_body(&request),
            r#"{"foo":{"bar":{"baz":{"qux":"quux"}}}}"#
        )
    }

    #[test]
    fn parse_array_end_body_param() {
        let url = "http://example.com";
        let components = vec!["[]=foo".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(request_body(&request), r#"["foo"]"#)
    }

    #[test]
    fn parse_nested_array_end_body_param() {
        let url = "http://example.com";
        let components = vec!["foo[][]=bar".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(request_body(&request), r#"{"foo":[["bar"]]}"#)
    }

    #[test]
    fn parse_array_index_body_param() {
        let url = "http://example.com";
        let components = vec!["[1]=foo".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(request_body(&request), r#"[null,"foo"]"#)
    }

    #[test]
    fn parse_nested_array_index_body_param() {
        let url = "http://example.com";
        let components = vec!["foo[0][1]=bar".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(request_body(&request), r#"{"foo":[[null,"bar"]]}"#)
    }

    #[test]
    fn parse_mixed_body_param() {
        let url = "http://example.com";
        let components = vec!["[][foo][bar][][1][baz]=qux".to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(
            request_body(&request),
            r#"[{"foo":{"bar":[[null,{"baz":"qux"}]]}}]"#
        )
    }

    #[test]
    fn parse_multiple_mixed_body_params() {
        let url = "http://example.com";
        let components = vec![
            "a[b]=c".to_string(),
            "a[d]=e".to_string(),
            "a[f][]=g".to_string(),
            "a[f][1]=h".to_string(),
            "a[f][2][i]=j".to_string(),
        ];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(
            request_body(&request),
            r#"{"a":{"b":"c","d":"e","f":["g","h",{"i":"j"}]}}"#
        )
    }

    #[test]
    fn parse_mixed_type_overlapping_body_params() {
        let url = "http://example.com";
        let components = vec!["a[b]=c".to_string(), "a[b][]=e".to_string()];
        let error = build_request(url, &components).unwrap_err();
        assert_eq!(error.to_string(), "expect array root");
    }

    #[test]
    fn parse_raw_json_string_body_param() {
        let url = "http://example.com";
        let components = vec![r#"foo:="bar""#.to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(request_body(&request), r#"{"foo":"bar"}"#)
    }

    #[test]
    fn parse_raw_json_int_body_param() {
        let url = "http://example.com";
        let components = vec![r#"foo:=1"#.to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(request_body(&request), r#"{"foo":1}"#)
    }

    #[test]
    fn parse_raw_json_null_body_param() {
        let url = "http://example.com";
        let components = vec![r#"foo:=null"#.to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(request_body(&request), r#"{"foo":null}"#)
    }

    #[test]
    fn parse_raw_json_map_body_param() {
        let url = "http://example.com";
        let components = vec![r#"foo:={"bar":"baz"}"#.to_string()];
        let request = build_request(url, &components).unwrap().build().unwrap();
        assert_eq!(request_body(&request), r#"{"foo":{"bar":"baz"}}"#)
    }

    fn request_body(req: &Request) -> String {
        String::from_utf8(req.body().unwrap().as_bytes().unwrap().to_owned()).unwrap()
    }
}
