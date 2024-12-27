use std::str::FromStr;

use anyhow::{bail, Result};
use http::{HeaderMap, HeaderName, HeaderValue};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::u32,
    combinator::value,
    multi::many0,
    sequence::{delimited, preceded, separated_pair},
    IResult,
};

use crate::json_builder::PathAccess;

#[derive(Debug)]
pub struct ParsedRequest {
    pub query: Vec<(String, String)>,
    pub headers: HeaderMap,
    pub body: Vec<BodyValue>,
}

impl ParsedRequest {
    pub fn from_inputs<T>(inputs: &[T]) -> Result<Self>
    where
        T: AsRef<str>,
    {
        let mut query = vec![];
        let mut headers = HeaderMap::new();
        let mut body = vec![];

        for input in inputs {
            let component = parse_component(input.as_ref())?;

            match component {
                RequestComponent::QueryParam { name, value } => {
                    query.push((name, value));
                }

                RequestComponent::Header { key, value } => {
                    let key = HeaderName::from_str(&key)?;
                    let value = HeaderValue::from_str(&value)?;
                    headers.append(key, value);
                }

                RequestComponent::BodyValue(value) => {
                    body.push(value);
                }
            }
        }

        Ok(Self {
            query,
            headers,
            body,
        })
    }
}

#[derive(Debug)]
pub enum BodyValue {
    String {
        path: Vec<PathAccess>,
        value: String,
    },

    JSON {
        path: Vec<PathAccess>,
        value: String,
    },
}

#[derive(Debug)]
enum RequestComponent {
    QueryParam { name: String, value: String },
    Header { key: String, value: String },
    BodyValue(BodyValue),
}

fn parse_component(input: &str) -> Result<RequestComponent> {
    match alt((query_param, body, header))(input) {
        Ok((remainder, component)) => {
            if remainder.is_empty() {
                Ok(component)
            } else {
                bail!("Remainder found in request component")
            }
        }

        Err(_) => {
            bail!("Invalid request component")
        }
    }
}

fn body(input: &str) -> IResult<&str, RequestComponent> {
    let mut path: Vec<PathAccess> = vec![];

    let (input, mut keys) = many0(alt((array_index, object_key, array_end)))(input)?;

    path.append(&mut keys);

    let body = match alt((value(true, tag(":=")), value(false, tag("="))))(input)? {
        (value, true) => BodyValue::JSON {
            path,
            value: value.to_string(),
        },
        (value, false) => BodyValue::String {
            path,
            value: value.to_string(),
        },
    };

    Ok(("", RequestComponent::BodyValue(body)))
}

fn object_key(input: &str) -> IResult<&str, PathAccess> {
    let raw_object_key = take_while1(|c| c != '.' && c != '[' && c != '=' && c != ':');

    let (remainder, key) = alt((
        delimited(tag("["), take_while1(|c| c != ']'), tag("]")),
        preceded(tag("."), &raw_object_key),
        &raw_object_key,
    ))(input)?;

    Ok((remainder, PathAccess::ObjectKey(key.to_string())))
}

fn array_index(input: &str) -> IResult<&str, PathAccess> {
    let (remainder, index) = alt((
        delimited(tag("["), u32, tag("]")),
        preceded(tag("."), u32),
        u32,
    ))(input)?;

    Ok((remainder, PathAccess::ArrayIndex(index)))
}

fn array_end(input: &str) -> IResult<&str, PathAccess> {
    let (remainder, _) = tag("[]")(input)?;
    Ok((remainder, PathAccess::ArrayEnd))
}

fn query_param(input: &str) -> IResult<&str, RequestComponent> {
    let (remainder, (name, value)) =
        separated_pair(query_param_key, tag("=="), query_param_value)(input)?;

    Ok((
        remainder,
        RequestComponent::QueryParam {
            name: name.to_string(),
            value: value.to_string(),
        },
    ))
}

fn query_param_key(input: &str) -> IResult<&str, &str> {
    take_while1(|c| c != '=')(input)
}

fn query_param_value(input: &str) -> IResult<&str, &str> {
    take_while1(|_| true)(input)
}

fn header(input: &str) -> IResult<&str, RequestComponent> {
    let (remainder, (name, value)) = separated_pair(header_name, tag(":"), header_value)(input)?;

    Ok((
        remainder,
        RequestComponent::Header {
            key: name.to_string(),
            value: value.to_string(),
        },
    ))
}

fn header_name(input: &str) -> IResult<&str, &str> {
    take_while1(|c| {
        ('A'..'Z').contains(&c)
            || ('a'..'z').contains(&c)
            || ('0'..'9').contains(&c)
            || c == '_'
            || c == '-'
    })(input)
}

fn header_value(input: &str) -> IResult<&str, &str> {
    take_while1(|_| true)(input)
}

#[cfg(test)]
mod tests {
    use crate::json_builder;

    use super::*;

    #[test]
    fn parse_simple_header() {
        let request = ParsedRequest::from_inputs(&["foo:bar"]).unwrap();

        let headers = vec![(
            HeaderName::from_str("foo").unwrap(),
            HeaderValue::from_str("bar").unwrap(),
        )];

        assert_eq!(request.headers, HeaderMap::from_iter(headers));
    }

    #[test]
    fn parse_quoted_header() {
        let request = ParsedRequest::from_inputs(&["foo:bar baz"]).unwrap();

        let headers = vec![(
            HeaderName::from_str("foo").unwrap(),
            HeaderValue::from_str("bar baz").unwrap(),
        )];

        assert_eq!(request.headers, HeaderMap::from_iter(headers));
    }

    #[test]
    fn reject_bad_header() {
        let error = ParsedRequest::from_inputs(&["foo bar:baz"]).unwrap_err();
        assert_eq!(error.to_string(), "Invalid request component");
    }

    #[test]
    fn parse_simple_query_param() {
        let request = ParsedRequest::from_inputs(&["foo==bar"]).unwrap();
        assert_eq!(request.query, vec![("foo".to_string(), "bar".to_string())]);
    }

    #[test]
    fn parse_quoted_query_param() {
        let request = ParsedRequest::from_inputs(&["foo bar==baz qux"]).unwrap();
        assert_eq!(
            request.query,
            vec![("foo bar".to_string(), "baz qux".to_string())]
        );
    }

    #[test]
    fn parse_simple_body_param() {
        let request = ParsedRequest::from_inputs(&["foo=bar"]).unwrap();
        assert_eq!(to_json(&request.body), r#"{"foo":"bar"}"#)
    }

    #[test]
    fn parse_nested_body_param() {
        let request = ParsedRequest::from_inputs(&["foo[bar]=baz"]).unwrap();
        assert_eq!(to_json(&request.body), r#"{"foo":{"bar":"baz"}}"#)
    }

    #[test]
    fn parse_flexible_object_key_body_param() {
        let request = ParsedRequest::from_inputs(&["foo[bar]baz.qux=quux"]).unwrap();
        assert_eq!(
            to_json(&request.body),
            r#"{"foo":{"bar":{"baz":{"qux":"quux"}}}}"#
        )
    }

    #[test]
    fn parse_flexible_array_index_body_param() {
        let request = ParsedRequest::from_inputs(&["foo[bar]0.qux=quux"]).unwrap();
        assert_eq!(
            to_json(&request.body),
            r#"{"foo":{"bar":[{"qux":"quux"}]}}"#
        )
    }

    #[test]
    fn parse_flexible_leading_body_param() {
        let request = ParsedRequest::from_inputs(&["[foo][bar]=baz"]).unwrap();
        assert_eq!(to_json(&request.body), r#"{"foo":{"bar":"baz"}}"#)
    }

    #[test]
    fn parse_multi_nested_body_param() {
        let request = ParsedRequest::from_inputs(&["foo[bar][baz][qux]=quux"]).unwrap();
        assert_eq!(
            to_json(&request.body),
            r#"{"foo":{"bar":{"baz":{"qux":"quux"}}}}"#
        )
    }

    #[test]
    fn parse_array_end_body_param() {
        let request = ParsedRequest::from_inputs(&["[]=foo"]).unwrap();
        assert_eq!(to_json(&request.body), r#"["foo"]"#)
    }

    #[test]
    fn parse_nested_array_end_body_param() {
        let request = ParsedRequest::from_inputs(&["foo[][]=bar"]).unwrap();
        assert_eq!(to_json(&request.body), r#"{"foo":[["bar"]]}"#)
    }

    #[test]
    fn parse_array_index_body_param() {
        let request = ParsedRequest::from_inputs(&["[1]=foo"]).unwrap();
        assert_eq!(to_json(&request.body), r#"[null,"foo"]"#)
    }

    #[test]
    fn parse_nested_array_index_body_param() {
        let request = ParsedRequest::from_inputs(&["foo[0][1]=bar"]).unwrap();
        assert_eq!(to_json(&request.body), r#"{"foo":[[null,"bar"]]}"#)
    }

    #[test]
    fn parse_mixed_body_param() {
        let request = ParsedRequest::from_inputs(&["[][foo][bar][][1][baz]=qux"]).unwrap();
        assert_eq!(
            to_json(&request.body),
            r#"[{"foo":{"bar":[[null,{"baz":"qux"}]]}}]"#
        )
    }

    #[test]
    fn parse_multiple_mixed_body_params() {
        let request = ParsedRequest::from_inputs(&[
            "a[b]=c",
            "a[d]=e",
            "a[f][]=g",
            "a[f][1]=h",
            "a[f][2][i]=j",
        ])
        .unwrap();
        assert_eq!(
            to_json(&request.body),
            r#"{"a":{"b":"c","d":"e","f":["g","h",{"i":"j"}]}}"#
        )
    }

    #[test]
    fn parse_raw_json_string_body_param() {
        let request = ParsedRequest::from_inputs(&[r#"foo:="bar""#]).unwrap();
        assert_eq!(to_json(&request.body), r#"{"foo":"bar"}"#)
    }

    #[test]
    fn parse_raw_json_int_body_param() {
        let request = ParsedRequest::from_inputs(&["foo:=1"]).unwrap();
        assert_eq!(to_json(&request.body), r#"{"foo":1}"#)
    }

    #[test]
    fn parse_raw_json_null_body_param() {
        let request = ParsedRequest::from_inputs(&["foo:=null"]).unwrap();
        assert_eq!(to_json(&request.body), r#"{"foo":null}"#)
    }

    #[test]
    fn parse_raw_json_map_body_param() {
        let request = ParsedRequest::from_inputs(&[r#"foo:={"bar":"baz"}"#]).unwrap();
        assert_eq!(to_json(&request.body), r#"{"foo":{"bar":"baz"}}"#)
    }

    fn to_json(body: &[BodyValue]) -> String {
        json_builder::build(body).unwrap()
    }
}
