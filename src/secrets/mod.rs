use std::collections::HashMap;

use enum_dispatch::enum_dispatch;
use vector_config::{configurable_component, NamedComponent};

use crate::{config::SecretBackend, signal};

mod exec;
mod test;

/// Configurable secret backends in Vector.
#[configurable_component]
#[derive(Clone, Debug)]
#[enum_dispatch(SecretBackend)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SecretBackends {
    /// Exec.
    Exec(#[configurable(derived)] exec::ExecBackend),

    /// Test.
    #[configurable(metadata(docs::hidden))]
    Test(#[configurable(derived)] test::TestBackend),
}

// We can't use `enum_dispatch` here because it doesn't support associated constants.
impl NamedComponent for SecretBackends {
    const NAME: &'static str = "_invalid_usage";

    fn get_component_name(&self) -> &'static str {
        match self {
            Self::Exec(config) => config.get_component_name(),
            Self::Test(config) => config.get_component_name(),
        }
    }
}
