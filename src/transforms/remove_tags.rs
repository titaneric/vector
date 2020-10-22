use super::Transform;
use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    internal_events::RemoveTagsEventProcessed,
    Event,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RemoveTagsConfig {
    pub tags: Vec<String>,
}

pub struct RemoveTags {
    tags: Vec<String>,
}

inventory::submit! {
    TransformDescription::new::<RemoveTagsConfig>("remove_tags")
}

impl GenerateConfig for RemoveTagsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self { tags: Vec::new() }).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "remove_tags")]
impl TransformConfig for RemoveTagsConfig {
    async fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(RemoveTags::new(self.tags.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn output_type(&self) -> DataType {
        DataType::Metric
    }

    fn transform_type(&self) -> &'static str {
        "remove_tags"
    }
}

impl RemoveTags {
    pub fn new(tags: Vec<String>) -> Self {
        RemoveTags { tags }
    }
}

impl Transform for RemoveTags {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        emit!(RemoveTagsEventProcessed);

        let tags = &mut event.as_mut_metric().tags;

        if let Some(map) = tags {
            for tag in &self.tags {
                map.remove(tag);

                if map.is_empty() {
                    *tags = None;
                    break;
                }
            }
        }

        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::{RemoveTags, RemoveTagsConfig};
    use crate::{
        event::metric::{Metric, MetricKind, MetricValue},
        event::Event,
        transforms::Transform,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RemoveTagsConfig>();
    }

    #[test]
    fn remove_tags() {
        let event = Event::Metric(Metric {
            name: "foo".into(),
            timestamp: None,
            tags: Some(
                vec![
                    ("env".to_owned(), "production".to_owned()),
                    ("region".to_owned(), "us-east-1".to_owned()),
                    ("host".to_owned(), "127.0.0.1".to_owned()),
                ]
                .into_iter()
                .collect(),
            ),
            kind: MetricKind::Incremental,
            value: MetricValue::Counter { value: 10.0 },
        });

        let mut transform = RemoveTags::new(vec!["region".into(), "host".into()]);
        let metric = transform.transform(event).unwrap().into_metric();
        let tags = metric.tags.unwrap();

        assert_eq!(tags.len(), 1);
        assert!(tags.contains_key("env"));
        assert!(!tags.contains_key("region"));
        assert!(!tags.contains_key("host"));
    }

    #[test]
    fn remove_all_tags() {
        let event = Event::Metric(Metric {
            name: "foo".into(),
            timestamp: None,
            tags: Some(
                vec![("env".to_owned(), "production".to_owned())]
                    .into_iter()
                    .collect(),
            ),
            kind: MetricKind::Incremental,
            value: MetricValue::Counter { value: 10.0 },
        });

        let mut transform = RemoveTags::new(vec!["env".into()]);
        let metric = transform.transform(event).unwrap().into_metric();

        assert!(metric.tags.is_none());
    }

    #[test]
    fn remove_tags_from_none() {
        let event = Event::Metric(Metric {
            name: "foo".into(),
            timestamp: None,
            tags: None,
            kind: MetricKind::Incremental,
            value: MetricValue::Set {
                values: vec!["bar".into()].into_iter().collect(),
            },
        });

        let mut transform = RemoveTags::new(vec!["env".into()]);
        let metric = transform.transform(event).unwrap().into_metric();

        assert!(metric.tags.is_none());
    }
}
