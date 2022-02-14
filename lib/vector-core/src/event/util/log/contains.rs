use std::collections::BTreeMap;

use super::{PathComponent, PathIter, Value};

/// Checks whether a field specified by a given path is present.
pub fn contains(fields: &BTreeMap<String, Value>, path: &str) -> bool {
    let mut path_iter = PathIter::new(path);

    match path_iter.next() {
        Some(PathComponent::Key(key)) => match fields.get(key.as_ref()) {
            None => false,
            Some(value) => value_contains(value, path_iter),
        },
        _ => false,
    }
}

fn value_contains<'a, I>(mut value: &Value, mut path_iter: I) -> bool
where
    I: Iterator<Item = PathComponent<'a>>,
{
    loop {
        value = match (path_iter.next(), value) {
            (None, _) => return true,
            (Some(PathComponent::Key(key)), Value::Object(map)) => match map.get(key.as_ref()) {
                None => return false,
                Some(nested_value) => nested_value,
            },
            (Some(PathComponent::Index(index)), Value::Array(array)) => match array.get(index) {
                None => return false,
                Some(nested_value) => nested_value,
            },
            _ => return false,
        }
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use super::{super::test::fields_from_json, *};

    #[test]
    fn contains_simple() {
        let fields = fields_from_json(json!({
            "field": 123
        }));

        assert!(contains(&fields, "field"));
    }

    #[test]
    fn contains_nested() {
        let fields = fields_from_json(json!({
            "a": {
                "b": {
                    "c": 5
                },
                "d": 4,
                "array": [null, 3, {
                    "x": 5
                }, [5]]
            }
        }));
        let queries = [
            ("a.b.c", true),
            ("a.d", true),
            ("a.array[1]", true),
            ("a.array[2].x", true),
            ("a.array[3][0]", true),
            ("a.array[3][1]", false),
            ("a.x", false),
            ("z", false),
            (".123", false),
            ("", false),
        ];

        for (query, expected) in &queries {
            assert_eq!(contains(&fields, query), *expected, "{}", query);
        }
    }
}
