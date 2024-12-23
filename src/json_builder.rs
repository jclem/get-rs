use anyhow::{Context, Result};
use serde_json::{json, Value};

#[derive(Debug, PartialEq)]
pub enum PathAccess {
    ObjectKey(String),
    ArrayIndex(u32),
    ArrayEnd,
}

pub fn put_value(root: &mut Value, path: &[PathAccess], value: Value) -> Result<()> {
    if path.is_empty() {
        *root = value;
        return Ok(());
    }

    match &path[0] {
        PathAccess::ObjectKey(key) => {
            if root.is_null() {
                *root = json!({});
            }

            let obj = root.as_object_mut().context("expect object root")?;
            let entry = obj.entry(key).or_insert(json!(null));

            put_value(entry, &path[1..], value)
        }
        PathAccess::ArrayIndex(index) => {
            if root.is_null() {
                *root = json!([]);
            }

            let arr = root.as_array_mut().context("expect array root")?;

            let index: usize = (*index).try_into().unwrap();

            if index >= arr.len() {
                arr.resize(index + 1, json!(null));
            }

            put_value(&mut arr[index], &path[1..], value)
        }
        PathAccess::ArrayEnd => {
            if root.is_null() {
                *root = json!([]);
            }

            let arr = root.as_array_mut().context("expect array root")?;
            arr.push(json!(null));
            let last_index = arr.len() - 1;
            put_value(&mut arr[last_index], &path[1..], value)
        }
    }
}
