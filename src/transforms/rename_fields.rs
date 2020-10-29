use super::Transform;
use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    event::Event,
    internal_events::{
        RenameFieldsEventProcessed, RenameFieldsFieldDoesNotExist, RenameFieldsFieldOverwritten,
    },
    serde::Fields,
};
use indexmap::map::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RenameFieldsConfig {
    pub fields: Fields<String>,
    drop_empty: Option<bool>,
}

pub struct RenameFields {
    fields: IndexMap<String, String>,
    drop_empty: bool,
}

inventory::submit! {
    TransformDescription::new::<RenameFieldsConfig>("rename_fields")
}

impl GenerateConfig for RenameFieldsConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(r#"fields.old_field_name = "new_field_name""#).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "rename_fields")]
impl TransformConfig for RenameFieldsConfig {
    async fn build(&self, _exec: TransformContext) -> crate::Result<Box<dyn Transform>> {
        let mut fields = IndexMap::default();
        for (key, value) in self.fields.clone().all_fields() {
            fields.insert(key.to_string(), value.to_string());
        }
        Ok(Box::new(RenameFields::new(
            fields,
            self.drop_empty.unwrap_or(false),
        )?))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "rename_fields"
    }
}

impl RenameFields {
    pub fn new(fields: IndexMap<String, String>, drop_empty: bool) -> crate::Result<Self> {
        Ok(RenameFields { fields, drop_empty })
    }
}

impl Transform for RenameFields {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        emit!(RenameFieldsEventProcessed);

        for (old_key, new_key) in &self.fields {
            let log = event.as_mut_log();
            match log.remove_prune(&old_key, self.drop_empty) {
                Some(v) => {
                    if event.as_mut_log().insert(&new_key, v).is_some() {
                        emit!(RenameFieldsFieldOverwritten { field: old_key });
                    }
                }
                None => {
                    emit!(RenameFieldsFieldDoesNotExist { field: old_key });
                }
            }
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RenameFieldsConfig>();
    }

    #[test]
    fn rename_fields() {
        let mut event = Event::from("message");
        event.as_mut_log().insert("to_move", "some value");
        event.as_mut_log().insert("do_not_move", "not moved");
        let mut fields = IndexMap::new();
        fields.insert(String::from("to_move"), String::from("moved"));
        fields.insert(
            String::from("not_present"),
            String::from("should_not_exist"),
        );

        let mut transform = RenameFields::new(fields, false).unwrap();

        let new_event = transform.transform(event).unwrap();

        assert!(new_event.as_log().get("to_move").is_none());
        assert_eq!(new_event.as_log()["moved"], "some value".into());
        assert!(new_event.as_log().get("not_present").is_none());
        assert!(new_event.as_log().get("should_not_exist").is_none());
        assert_eq!(new_event.as_log()["do_not_move"], "not moved".into());
    }
}
