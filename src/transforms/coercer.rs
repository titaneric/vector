use super::Transform;
use crate::event::Event;
use crate::runtime::TaskExecutor;
use crate::topology::config::{DataType, TransformConfig, TransformDescription};
use crate::types::{parse_conversion_map, Conversion};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str;
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct CoercerConfig {
    types: HashMap<Atom, String>,
    drop_unspecified: bool,
}

inventory::submit! {
    TransformDescription::new::<CoercerConfig>("coercer")
}

#[typetag::serde(name = "coercer")]
impl TransformConfig for CoercerConfig {
    fn build(&self, _exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        let types = parse_conversion_map(&self.types)?;
        Ok(Box::new(Coercer {
            types,
            drop_unspecified: self.drop_unspecified,
        }))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "coercer"
    }
}

pub struct Coercer {
    types: HashMap<Atom, Conversion>,
    drop_unspecified: bool,
}

impl Transform for Coercer {
    fn transform(&mut self, event: Event) -> Option<Event> {
        let mut log = event.into_log();
        if self.drop_unspecified {
            // This uses a different algorithm from the default path
            // below, as it will be fewer steps to fully recreate the
            // event than to scan the event for extraneous fields after
            // conversion.
            let drain = log.drain().collect::<Vec<_>>();
            for (field, value) in drain {
                if let Some(conv) = self.types.get(&field) {
                    match conv.convert(value) {
                        Ok(converted) => log.insert(field, converted),
                        Err(error) => {
                            warn!(
                                message = "Could not convert types.",
                                field = &field[..],
                                %error,
                                rate_limit_secs = 10,
                            );
                        }
                    }
                }
            }
        } else {
            for (field, conv) in &self.types {
                if let Some(value) = log.remove(field) {
                    match conv.convert(value) {
                        Ok(converted) => log.insert(field, converted),
                        Err(error) => {
                            warn!(
                                message = "Could not convert types.",
                                field = &field[..],
                                %error,
                                rate_limit_secs = 10,
                            );
                        }
                    }
                }
            }
        }
        Some(Event::Log(log))
    }
}

#[cfg(test)]
mod tests {
    use super::CoercerConfig;
    use crate::event::{flatten::flatten, LogEvent, Value};
    use crate::{topology::config::TransformConfig, Event};
    use pretty_assertions::assert_eq;
    use serde_json::Value as JsonValue;

    fn parse_it(extra: &str) -> LogEvent {
        let rt = crate::runtime::Runtime::single_threaded().unwrap();
        let mut event = Event::from("dummy message");
        for &(key, value) in &[
            ("number", "1234"),
            ("bool", "yes"),
            ("other", "no"),
            ("float", "broken"),
        ] {
            event.as_mut_log().insert(key, value);
        }

        let mut coercer = toml::from_str::<CoercerConfig>(&format!(
            r#"{}
            [types]
            number = "int"
            float = "float"
            bool = "bool"
            "#,
            extra
        ))
        .unwrap()
        .build(rt.executor())
        .unwrap();
        coercer.transform(event).unwrap().into_log()
    }

    #[test]
    fn converts_valid_fields() {
        let log = parse_it("");
        assert_eq!(log[&"number".into()], Value::Integer(1234));
        assert_eq!(log[&"bool".into()], Value::Boolean(true));
    }

    #[test]
    fn leaves_unnamed_fields_as_is() {
        let log = parse_it("");
        assert_eq!(log[&"other".into()], Value::Bytes("no".into()));
    }

    #[test]
    fn drops_nonconvertible_fields() {
        let log = parse_it("");
        assert!(log.get(&"float".into()).is_none());
    }

    #[test]
    fn drops_unspecified_fields() {
        let log = parse_it("drop_unspecified = true");

        let mut expected = Event::new_empty_log();
        match serde_json::from_str::<JsonValue>(
            r#"{
                 "bool": true,
                 "number": 1234
               }"#,
        )
        .unwrap()
        {
            JsonValue::Object(object) => flatten(expected.as_mut_log(), object),
            _ => panic!("Invalid result from serde_json::from_str"),
        }
        assert_eq!(log, expected.into_log());
    }
}
