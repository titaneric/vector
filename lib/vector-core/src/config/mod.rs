use std::{fmt, num::NonZeroUsize};

use bitmask_enum::bitmask;

mod global_options;
mod log_schema;
pub mod proxy;

use crate::event::LogEvent;
pub use global_options::GlobalOptions;
pub use log_schema::{init_log_schema, log_schema, LogSchema};
use lookup::lookup_v2::Path;
use lookup::path;
use serde::{Deserialize, Serialize};
use value::Value;
pub use vector_common::config::ComponentKey;
use vector_config::configurable_component;

use crate::schema;

pub const MEMORY_BUFFER_DEFAULT_MAX_EVENTS: NonZeroUsize =
    vector_buffers::config::memory_buffer_default_max_events();

// This enum should be kept alphabetically sorted as the bitmask value is used when
// sorting sources by data type in the GraphQL API.
#[bitmask(u8)]
pub enum DataType {
    Log,
    Metric,
    Trace,
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut t = Vec::new();
        self.contains(DataType::Log).then(|| t.push("Log"));
        self.contains(DataType::Metric).then(|| t.push("Metric"));
        self.contains(DataType::Trace).then(|| t.push("Trace"));
        f.write_str(&t.join(","))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    ty: DataType,
    log_schema_requirement: schema::Requirement,
}

impl Input {
    pub fn data_type(&self) -> DataType {
        self.ty
    }

    pub fn schema_requirement(&self) -> &schema::Requirement {
        &self.log_schema_requirement
    }

    pub fn new(ty: DataType) -> Self {
        Self {
            ty,
            log_schema_requirement: schema::Requirement::empty(),
        }
    }

    pub fn log() -> Self {
        Self {
            ty: DataType::Log,
            log_schema_requirement: schema::Requirement::empty(),
        }
    }

    pub fn metric() -> Self {
        Self {
            ty: DataType::Metric,
            log_schema_requirement: schema::Requirement::empty(),
        }
    }

    pub fn trace() -> Self {
        Self {
            ty: DataType::Trace,
            log_schema_requirement: schema::Requirement::empty(),
        }
    }

    pub fn all() -> Self {
        Self {
            ty: DataType::all(),
            log_schema_requirement: schema::Requirement::empty(),
        }
    }

    /// Set the schema requirement for this output.
    #[must_use]
    pub fn with_schema_requirement(mut self, schema_requirement: schema::Requirement) -> Self {
        self.log_schema_requirement = schema_requirement;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub port: Option<String>,
    pub ty: DataType,

    // NOTE: schema definitions are only implemented/supported for log-type events. There is no
    // inherent blocker to support other types as well, but it'll require additional work to add
    // the relevant schemas, and store them separately in this type.
    ///
    /// The `None` variant of a schema definition has two distinct meanings for a source component
    /// versus a transform component:
    ///
    /// For *sources*, a `None` schema is identical to a `Some(Definition::source_default())`.
    ///
    /// For a *transform*, a `None` schema means the transform inherits the merged [`Definition`]
    /// of its inputs, without modifying the schema further.
    pub log_schema_definition: Option<schema::Definition>,
}

impl Output {
    /// Create a default `Output` of the given data type.
    ///
    /// A default output is one without a port identifier (i.e. not a named output) and the default
    /// output consumers will receive if they declare the component itself as an input.
    pub fn default(ty: DataType) -> Self {
        Self {
            port: None,
            ty,
            log_schema_definition: None,
        }
    }

    /// Set the schema definition for this `Output`.
    #[must_use]
    pub fn with_schema_definition(mut self, schema_definition: schema::Definition) -> Self {
        self.log_schema_definition = Some(schema_definition);
        self
    }

    /// Set the port name for this `Output`.
    #[must_use]
    pub fn with_port(mut self, name: impl Into<String>) -> Self {
        self.port = Some(name.into());
        self
    }
}

/// Configuration of acknowledgement behavior.
#[configurable_component]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AcknowledgementsConfig {
    /// Enables end-to-end acknowledgements.
    enabled: Option<bool>,
}

impl AcknowledgementsConfig {
    #[must_use]
    pub fn merge_default(&self, other: &Self) -> Self {
        let enabled = self.enabled.or(other.enabled);
        Self { enabled }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.unwrap_or(false)
    }
}

impl From<Option<bool>> for AcknowledgementsConfig {
    fn from(enabled: Option<bool>) -> Self {
        Self { enabled }
    }
}

impl From<bool> for AcknowledgementsConfig {
    fn from(enabled: bool) -> Self {
        Some(enabled).into()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize, PartialOrd, Ord, Eq)]
pub enum LogNamespace {
    /// Vector native namespacing
    ///
    /// Deserialized data is placed in the root of the event.
    /// Extra data is placed in "event metadata"
    Vector,

    /// This is the legacy namespacing.
    ///
    /// All data is set in the root of the event. Since this can lead
    /// to collisions, deserialized data has priority over metadata
    Legacy,
}

/// The user-facing config for log namespace is a bool (enabling or disabling the "Log Namespacing" feature).
/// Internally, this is converted to a enum.
impl From<bool> for LogNamespace {
    fn from(x: bool) -> Self {
        if x {
            LogNamespace::Vector
        } else {
            LogNamespace::Legacy
        }
    }
}

impl Default for LogNamespace {
    fn default() -> Self {
        Self::Legacy
    }
}

impl LogNamespace {
    /// Vector: This is added to "event metadata", nested under the source name.
    ///
    /// Legacy: This is stored on the event root, only if a field with that name doesn't already exist.
    pub fn insert_source_metadata<'a>(
        &self,
        source_name: &'a str,
        log: &mut LogEvent,
        key: impl Path<'a>,
        value: impl Into<Value>,
    ) {
        match self {
            LogNamespace::Vector => {
                log.metadata_mut()
                    .value_mut()
                    .insert(path!(source_name).concat(key), value);
            }
            LogNamespace::Legacy => {
                log.try_insert(key, value);
            }
        }
    }

    /// Vector: This is added to the "event metadata", nested under the name "vector". This data
    /// will be marked as read-only in VRL.
    ///
    /// Legacy: This is stored on the event root, only if a field with that name doesn't already exist.
    pub fn insert_vector_metadata<'a>(
        &self,
        log: &mut LogEvent,
        legacy_key: impl Path<'a>,
        metadata_key: impl Path<'a>,
        value: impl Into<Value>,
    ) {
        match self {
            LogNamespace::Vector => {
                log.metadata_mut()
                    .value_mut()
                    .insert(path!("vector").concat(metadata_key), value);
            }
            LogNamespace::Legacy => log.try_insert(legacy_key, value),
        }
    }

    pub fn new_log_from_data(&self, value: impl Into<Value>) -> LogEvent {
        match self {
            LogNamespace::Vector | LogNamespace::Legacy => LogEvent::from(value.into()),
        }
    }

    // combine a global (self) and local value to get the actual namespace
    #[must_use]
    pub fn merge(&self, override_value: Option<impl Into<LogNamespace>>) -> LogNamespace {
        override_value.map_or(*self, Into::into)
    }
}
