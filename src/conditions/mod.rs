use serde::{Deserialize, Serialize};

use crate::{config::component::ComponentDescription, event::Event};

pub mod check_fields;
pub mod datadog_search;
pub mod is_log;
pub mod is_metric;
pub mod not;
pub mod vrl;

pub use self::vrl::VrlConfig;
pub use check_fields::CheckFieldsConfig;

#[derive(Debug, Clone)]
pub enum Condition {
    Not(not::Not),
    IsLog(is_log::IsLog),
    IsMetric(is_metric::IsMetric),
    Vrl(vrl::Vrl),
    CheckFields(check_fields::CheckFields),
    DatadogSearch(datadog_search::DatadogSearchRunner),
}

impl Condition {
    pub(crate) fn check(&self, e: &Event) -> bool {
        match self {
            Condition::IsLog(x) => x.check(e),
            Condition::IsMetric(x) => x.check(e),
            Condition::Not(x) => x.check(e),
            Condition::CheckFields(x) => x.check(e),
            Condition::DatadogSearch(x) => x.check(e),
            Condition::Vrl(x) => x.check(e),
        }
    }

    /// Provides context for a failure. This is potentially mildly expensive if
    /// it involves string building and so should be avoided in hot paths.
    pub(crate) fn check_with_context(&self, e: &Event) -> Result<(), String> {
        match self {
            Condition::IsLog(x) => x.check_with_context(e),
            Condition::IsMetric(x) => x.check_with_context(e),
            Condition::Not(x) => x.check_with_context(e),
            Condition::CheckFields(x) => x.check_with_context(e),
            Condition::DatadogSearch(x) => x.check_with_context(e),
            Condition::Vrl(x) => x.check_with_context(e),
        }
    }
}

pub trait Conditional {
    fn check(&self, e: &Event) -> bool;

    /// Provides context for a failure. This is potentially mildly expensive if
    /// it involves string building and so should be avoided in hot paths.
    fn check_with_context(&self, e: &Event) -> Result<(), String> {
        if self.check(e) {
            Ok(())
        } else {
            Err("condition failed".into())
        }
    }
}

#[typetag::serde(tag = "type")]
pub trait ConditionConfig: std::fmt::Debug + Send + Sync + dyn_clone::DynClone {
    fn build(&self, enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition>;
}

dyn_clone::clone_trait_object!(ConditionConfig);

pub type ConditionDescription = ComponentDescription<Box<dyn ConditionConfig>>;

inventory::collect!(ConditionDescription);

/// A condition can either be a raw string such as
/// `condition = '.message == "hooray"'`.
/// In this case it is turned into a VRL condition.
/// Otherwise it is a condition such as:
///
/// condition.type = 'check_fields'
/// condition."message.equals" = 'hooray'
///
///
/// It is important to note that because the way this is
/// structured, it is wrong to flatten a field that contains
/// an AnyCondition:
///
/// #[serde(flatten)]
/// condition: AnyCondition,
///
/// This will result in an error when serializing to json
/// which we need to do when determining which transforms have changed
/// when a config is reloaded.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum AnyCondition {
    String(String),
    Map(Box<dyn ConditionConfig>),
}

impl AnyCondition {
    pub fn build(&self, enrichment_tables: &enrichment::TableRegistry) -> crate::Result<Condition> {
        match self {
            AnyCondition::String(s) => VrlConfig { source: s.clone() }.build(enrichment_tables),
            AnyCondition::Map(m) => m.build(enrichment_tables),
        }
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[derive(Deserialize, Debug)]
    struct Test {
        condition: AnyCondition,
    }

    #[test]
    fn deserialize_anycondition_default() {
        let conf: Test = toml::from_str(r#"condition = ".nork == false""#).unwrap();
        assert_eq!(
            r#"String(".nork == false")"#,
            format!("{:?}", conf.condition)
        )
    }

    #[test]
    fn deserialize_anycondition_check_fields() {
        let conf: Test = toml::from_str(indoc! {r#"
            condition.type = "check_fields"
            condition."norg.equals" = "nork"
        "#})
        .unwrap();

        assert_eq!(
            r#"Map(CheckFieldsConfig { predicates: {"norg.equals": "nork"} })"#,
            format!("{:?}", conf.condition)
        )
    }

    #[test]
    fn deserialize_anycondition_vrl() {
        let conf: Test = toml::from_str(indoc! {r#"
            condition.type = "vrl"
            condition.source = '.nork == true'
        "#})
        .unwrap();

        assert_eq!(
            r#"Map(VrlConfig { source: ".nork == true" })"#,
            format!("{:?}", conf.condition)
        )
    }
}
