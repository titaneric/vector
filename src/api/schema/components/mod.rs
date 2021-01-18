pub mod sink;
pub mod source;
pub mod state;
pub mod transform;

use crate::{
    api::schema::{
        components::state::component_by_name,
        filter::{self, filter_items},
        relay,
    },
    config::Config,
    filter_check,
};
use async_graphql::{Enum, InputObject, Interface, Object, Subscription};
use lazy_static::lazy_static;
use std::collections::{HashMap, HashSet};
use tokio::stream::{Stream, StreamExt};

#[derive(Debug, Clone, Interface)]
#[graphql(
    field(name = "name", type = "String"),
    field(name = "component_type", type = "String")
)]
pub enum Component {
    Source(source::Source),
    Transform(transform::Transform),
    Sink(sink::Sink),
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
/// Type of component
pub enum ComponentType {
    Source,
    Transform,
    Sink,
}

impl Component {
    fn get_name(&self) -> &str {
        match self {
            Component::Source(c) => c.0.name.as_str(),
            Component::Transform(c) => c.0.name.as_str(),
            Component::Sink(c) => c.0.name.as_str(),
        }
    }

    fn get_filter_type(&self) -> ComponentType {
        match self {
            Component::Source(_) => ComponentType::Source,
            Component::Transform(_) => ComponentType::Transform,
            Component::Sink(_) => ComponentType::Sink,
        }
    }
}

#[derive(Default, InputObject)]
pub struct ComponentsFilter {
    name: Option<Vec<filter::StringFilter>>,
    component_type: Option<Vec<filter::EqualityFilter<ComponentType>>>,
    or: Option<Vec<Self>>,
}

impl filter::CustomFilter<Component> for ComponentsFilter {
    fn matches(&self, component: &Component) -> bool {
        filter_check!(
            self.name
                .as_ref()
                .map(|f| f.iter().all(|f| f.filter_value(component.get_name()))),
            self.component_type.as_ref().map(|f| f
                .iter()
                .all(|f| f.filter_value(component.get_filter_type())))
        );
        true
    }

    fn or(&self) -> Option<&Vec<Self>> {
        self.or.as_ref()
    }
}

#[derive(Default)]
pub struct ComponentsQuery;

#[Object]
impl ComponentsQuery {
    /// Configured components (sources/transforms/sinks)
    async fn components(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
        filter: Option<ComponentsFilter>,
    ) -> relay::ConnectionResult<Component> {
        let filter = filter.unwrap_or_else(ComponentsFilter::default);
        let components = filter_items(state::get_components().into_iter(), &filter);

        relay::query(
            components.into_iter(),
            relay::Params::new(after, before, first, last),
            10,
        )
        .await
    }

    /// Configured sources
    async fn sources(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
        filter: Option<source::SourcesFilter>,
    ) -> relay::ConnectionResult<source::Source> {
        let filter = filter.unwrap_or_else(source::SourcesFilter::default);
        let sources = filter_items(state::get_sources().into_iter(), &filter);

        relay::query(
            sources.into_iter(),
            relay::Params::new(after, before, first, last),
            10,
        )
        .await
    }

    /// Configured transforms
    async fn transforms(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
        filter: Option<transform::TransformsFilter>,
    ) -> relay::ConnectionResult<transform::Transform> {
        let filter = filter.unwrap_or_else(transform::TransformsFilter::default);
        let transforms = filter_items(state::get_transforms().into_iter(), &filter);

        relay::query(
            transforms.into_iter(),
            relay::Params::new(after, before, first, last),
            10,
        )
        .await
    }

    /// Configured sinks
    async fn sinks(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
        filter: Option<sink::SinksFilter>,
    ) -> relay::ConnectionResult<sink::Sink> {
        let filter = filter.unwrap_or_else(sink::SinksFilter::default);
        let sinks = filter_items(state::get_sinks().into_iter(), &filter);

        relay::query(
            sinks.into_iter(),
            relay::Params::new(after, before, first, last),
            10,
        )
        .await
    }

    /// Gets a configured component by name
    async fn component_by_name(&self, name: String) -> Option<Component> {
        component_by_name(&name)
    }
}

#[derive(Clone, Debug)]
enum ComponentChanged {
    Added(Component),
    Removed(Component),
}

lazy_static! {
    static ref COMPONENT_CHANGED: tokio::sync::broadcast::Sender<ComponentChanged> = {
        let (tx, _) = tokio::sync::broadcast::channel(10);
        tx
    };
}

#[derive(Debug, Default)]
pub struct ComponentsSubscription;

#[Subscription]
impl ComponentsSubscription {
    /// Subscribes to all newly added components
    async fn component_added(&self) -> impl Stream<Item = Component> {
        COMPONENT_CHANGED
            .subscribe()
            .into_stream()
            .filter_map(|c| match c {
                Ok(ComponentChanged::Added(c)) => Some(c),
                _ => None,
            })
    }

    /// Subscribes to all removed components
    async fn component_removed(&self) -> impl Stream<Item = Component> {
        COMPONENT_CHANGED
            .subscribe()
            .into_stream()
            .filter_map(|c| match c {
                Ok(ComponentChanged::Removed(c)) => Some(c),
                _ => None,
            })
    }
}

/// Update the 'global' configuration that will be consumed by component queries
pub fn update_config(config: &Config) {
    let mut new_components = HashMap::new();

    // Sources
    for (name, source) in config.sources.iter() {
        new_components.insert(
            name.to_owned(),
            Component::Source(source::Source(source::Data {
                name: name.to_owned(),
                component_type: source.source_type().to_string(),
                output_type: source.output_type(),
            })),
        );
    }

    // Transforms
    for (name, transform) in config.transforms.iter() {
        new_components.insert(
            name.to_string(),
            Component::Transform(transform::Transform(transform::Data {
                name: name.to_owned(),
                component_type: transform.inner.transform_type().to_string(),
                inputs: transform.inputs.clone(),
            })),
        );
    }

    // Sinks
    for (name, sink) in config.sinks.iter() {
        new_components.insert(
            name.to_string(),
            Component::Sink(sink::Sink(sink::Data {
                name: name.to_owned(),
                component_type: sink.inner.sink_type().to_string(),
                inputs: sink.inputs.clone(),
            })),
        );
    }

    // Get the names of existing components
    let existing_component_names = state::get_component_names();
    let new_component_names = new_components
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<HashSet<String>>();

    // Publish all components that have been removed
    existing_component_names
        .difference(&new_component_names)
        .for_each(|name| {
            let _ = COMPONENT_CHANGED.send(ComponentChanged::Removed(
                state::component_by_name(name).expect("Couldn't get component by name"),
            ));
        });

    // Publish all components that have been added
    new_component_names
        .difference(&existing_component_names)
        .for_each(|name| {
            let _ = COMPONENT_CHANGED.send(ComponentChanged::Added(
                new_components.get(name).unwrap().clone(),
            ));
        });

    // Override the old component state
    state::update(new_components);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DataType;

    /// Generate component fixes for use with tests
    fn component_fixtures() -> Vec<Component> {
        vec![
            Component::Source(source::Source(source::Data {
                name: "gen1".to_string(),
                component_type: "generator".to_string(),
                output_type: DataType::Metric,
            })),
            Component::Source(source::Source(source::Data {
                name: "gen2".to_string(),
                component_type: "generator".to_string(),
                output_type: DataType::Metric,
            })),
            Component::Source(source::Source(source::Data {
                name: "gen3".to_string(),
                component_type: "generator".to_string(),
                output_type: DataType::Metric,
            })),
            Component::Transform(transform::Transform(transform::Data {
                name: "parse_json".to_string(),
                component_type: "json".to_string(),
                inputs: vec!["gen1".to_string(), "gen2".to_string()],
            })),
            Component::Sink(sink::Sink(sink::Data {
                name: "devnull".to_string(),
                component_type: "blackhole".to_string(),
                inputs: vec!["gen3".to_string(), "parse_json".to_string()],
            })),
        ]
    }

    #[test]
    fn components_filter_contains() {
        let filter = ComponentsFilter {
            name: Some(vec![filter::StringFilter {
                contains: Some("gen".to_string()),
                ..Default::default()
            }]),
            ..Default::default()
        };

        let components = filter_items(component_fixtures().into_iter(), &filter);

        assert_eq!(components.len(), 3);
    }

    #[test]
    fn components_filter_equals_or() {
        let filter = ComponentsFilter {
            name: Some(vec![filter::StringFilter {
                equals: Some("gen1".to_string()),
                ..Default::default()
            }]),
            or: Some(vec![ComponentsFilter {
                name: Some(vec![filter::StringFilter {
                    equals: Some("devnull".to_string()),
                    ..Default::default()
                }]),
                ..Default::default()
            }]),
            ..Default::default()
        };

        let components = filter_items(component_fixtures().into_iter(), &filter);

        assert_eq!(components.len(), 2);
    }

    #[test]
    fn components_filter_and() {
        let filter = ComponentsFilter {
            name: Some(vec![filter::StringFilter {
                equals: Some("gen1".to_string()),
                ..Default::default()
            }]),
            component_type: Some(vec![filter::EqualityFilter {
                equals: Some(ComponentType::Source),
                not_equals: None,
            }]),
            ..Default::default()
        };

        let components = filter_items(component_fixtures().into_iter(), &filter);

        assert_eq!(components.len(), 1);
    }

    #[test]
    fn components_filter_and_or() {
        let filter = ComponentsFilter {
            name: Some(vec![filter::StringFilter {
                equals: Some("gen1".to_string()),
                ..Default::default()
            }]),
            component_type: Some(vec![filter::EqualityFilter {
                equals: Some(ComponentType::Source),
                not_equals: None,
            }]),
            or: Some(vec![ComponentsFilter {
                component_type: Some(vec![filter::EqualityFilter {
                    equals: Some(ComponentType::Sink),
                    not_equals: None,
                }]),
                ..Default::default()
            }]),
        };

        let components = filter_items(component_fixtures().into_iter(), &filter);

        assert_eq!(components.len(), 2);
    }
}
