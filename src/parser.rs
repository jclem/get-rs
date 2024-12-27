use anyhow::{bail, Result};
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

pub struct ParsedRequest {
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<BodyValue>,
}

impl ParsedRequest {
    pub fn from_inputs<T>(inputs: &[T]) -> Result<Self>
    where
        T: AsRef<str>,
    {
        let mut query = vec![];
        let mut headers = vec![];
        let mut body = vec![];

        for input in inputs {
            let component = parse_component(input.as_ref())?;

            match component {
                RequestComponent::QueryParam { name, value } => {
                    query.push((name, value));
                }

                RequestComponent::Header { name, value } => {
                    headers.push((name, value));
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
    Header { name: String, value: String },
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
            name: name.to_string(),
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
