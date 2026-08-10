//! Field accessors for the loosely-typed JSON that the Codex app-server and the
//! GitHub CLI return. These payloads are read field-by-field rather than
//! deserialized into structs because both sides add fields freely and a missing
//! one must never fail the whole read.

use serde_json::Value;

/// A string field, or `None` when absent or not a string.
pub(crate) fn str_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// An integer field, or `None` when absent or not a number.
pub(crate) fn i64_at(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

/// An array field, or `None` when absent or not an array.
pub(crate) fn arr_at<'a>(value: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    value.get(key).and_then(Value::as_array)
}

/// An array field as a slice, empty when absent. For the common
/// "iterate whatever is there" case where absent and empty mean the same thing.
pub(crate) fn arr_or_empty<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    arr_at(value, key).map(Vec::as_slice).unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_present_fields_by_type() {
        let value = json!({"name": "pingu", "count": 3, "ok": true, "items": [1, 2]});
        assert_eq!(str_at(&value, "name"), Some("pingu"));
        assert_eq!(i64_at(&value, "count"), Some(3));
        assert_eq!(arr_at(&value, "items").map(Vec::len), Some(2));
    }

    #[test]
    fn absent_and_mistyped_fields_are_none() {
        let value = json!({"name": 42});
        assert_eq!(str_at(&value, "name"), None);
        assert_eq!(str_at(&value, "missing"), None);
        assert_eq!(i64_at(&value, "missing"), None);
        assert!(arr_or_empty(&value, "missing").is_empty());
    }

    #[test]
    fn non_object_values_read_as_absent() {
        let value = json!("just a string");
        assert_eq!(str_at(&value, "anything"), None);
        assert!(arr_or_empty(&value, "anything").is_empty());
    }
}
