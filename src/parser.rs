use anyhow::{bail, Result};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::u32,
    combinator::{opt, value},
    multi::many0,
    sequence::{delimited, separated_pair},
    IResult,
};

use crate::json_builder::PathAccess;

#[derive(Debug, PartialEq)]
pub enum RequestComponent {
    QueryParam { name: String, value: String },
    Header { name: String, value: String },
    Body(Body),
}

#[derive(Debug, PartialEq)]
pub enum Body {
    String {
        path: Vec<PathAccess>,
        value: String,
    },

    JSON {
        path: Vec<PathAccess>,
        value: String,
    },
}

pub fn parse_component(input: &str) -> Result<RequestComponent> {
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

    let (input, first_key) = opt(take_while1(|c| c != ':' && c != '=' && c != '['))(input)?;
    if let Some(first_key) = first_key {
        path.push(PathAccess::ObjectKey(first_key.to_string()));
    }

    let (input, mut keys) = many0(alt((array_index, object_key, array_end)))(input)?;
    path.append(&mut keys);

    let body = match alt((value(true, tag(":=")), value(false, tag("="))))(input)? {
        (value, true) => Body::JSON {
            path,
            value: value.to_string(),
        },
        (value, false) => Body::String {
            path,
            value: value.to_string(),
        },
    };

    Ok(("", RequestComponent::Body(body)))
}

fn object_key(input: &str) -> IResult<&str, PathAccess> {
    let (remainder, key) = delimited(tag("["), take_while1(|c| c != ']'), tag("]"))(input)?;
    Ok((remainder, PathAccess::ObjectKey(key.to_string())))
}

fn array_index(input: &str) -> IResult<&str, PathAccess> {
    let (remainder, index) = delimited(tag("["), u32, tag("]"))(input)?;
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
