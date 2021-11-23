use crate::{
    config::{
        log_schema, ComponentKey, DataType, TransformConfig, TransformContext, TransformDescription,
    },
    event::{Event, VrlTarget},
    internal_events::{RemapMappingAbort, RemapMappingError},
    transforms::{FallibleFunctionTransform, Transform},
    Result,
};

use serde::{Deserialize, Serialize};
use shared::TimeZone;
use snafu::{ResultExt, Snafu};
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use vrl::diagnostic::Formatter;
use vrl::prelude::ExpressionError;
use vrl::{Program, Runtime, Terminate};

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct RemapConfig {
    pub source: Option<String>,
    pub file: Option<PathBuf>,
    #[serde(default)]
    pub timezone: TimeZone,
    pub drop_on_error: bool,
    #[serde(default = "crate::serde::default_true")]
    pub drop_on_abort: bool,
    pub reroute_dropped: bool,
}

inventory::submit! {
    TransformDescription::new::<RemapConfig>("remap")
}

impl_generate_config_from_default!(RemapConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "remap")]
impl TransformConfig for RemapConfig {
    async fn build(&self, context: &TransformContext) -> Result<Transform> {
        let remap = Remap::new(self.clone(), context)?;
        Ok(if self.reroute_dropped {
            Transform::fallible_function(remap)
        } else {
            Transform::function(remap)
        })
    }

    fn named_outputs(&self) -> Vec<String> {
        if self.reroute_dropped {
            vec![String::from("dropped")]
        } else {
            vec![]
        }
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "remap"
    }
}

#[derive(Debug)]
pub struct Remap {
    component_key: Option<ComponentKey>,
    program: Program,
    runtime: Runtime,
    timezone: TimeZone,
    drop_on_error: bool,
    drop_on_abort: bool,
    reroute_dropped: bool,
}

impl Remap {
    pub fn new(config: RemapConfig, context: &TransformContext) -> crate::Result<Self> {
        let source = match (&config.source, &config.file) {
            (Some(source), None) => source.to_owned(),
            (None, Some(path)) => {
                let mut buffer = String::new();

                File::open(path)
                    .with_context(|| FileOpenFailed { path })?
                    .read_to_string(&mut buffer)
                    .with_context(|| FileReadFailed { path })?;

                buffer
            }
            _ => return Err(Box::new(BuildError::SourceAndOrFile)),
        };

        let mut functions = vrl_stdlib::all();
        functions.append(&mut enrichment::vrl_functions());

        let program = vrl::compile(
            &source,
            &functions,
            Some(Box::new(context.enrichment_tables.clone())),
        )
        .map_err(|diagnostics| Formatter::new(&source, diagnostics).colored().to_string())?;

        Ok(Remap {
            component_key: context.key.clone(),
            program,
            runtime: Runtime::default(),
            timezone: config.timezone,
            drop_on_error: config.drop_on_error,
            drop_on_abort: config.drop_on_abort,
            reroute_dropped: config.reroute_dropped,
        })
    }

    #[cfg(test)]
    const fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    fn annotate_dropped(&self, event: &mut Event, reason: &str, error: ExpressionError) {
        match event {
            Event::Log(ref mut log) => {
                log.insert(
                    log_schema().metadata_key(),
                    serde_json::json!({
                        "dropped": {
                            "reason": reason,
                            "message": error.to_string(),
                            "component_id": self.component_key,
                            "component_type": "remap",
                            "component_kind": "transform",
                        }
                    }),
                );
            }
            Event::Metric(ref mut metric) => {
                let m = log_schema().metadata_key();
                metric.insert_tag(format!("{}.dropped.reason", m), reason.into());
                metric.insert_tag(
                    format!("{}.dropped.component_id", m),
                    self.component_key
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(String::new),
                );
                metric.insert_tag(format!("{}.dropped.component_type", m), "remap".into());
                metric.insert_tag(format!("{}.dropped.component_kind", m), "transform".into());
            }
        }
    }
}

impl Clone for Remap {
    fn clone(&self) -> Self {
        Self {
            component_key: self.component_key.clone(),
            program: self.program.clone(),
            runtime: Runtime::default(),
            timezone: self.timezone,
            drop_on_error: self.drop_on_error,
            drop_on_abort: self.drop_on_abort,
            reroute_dropped: self.reroute_dropped,
        }
    }
}

impl FallibleFunctionTransform for Remap {
    fn transform(&mut self, output: &mut Vec<Event>, err_output: &mut Vec<Event>, event: Event) {
        // If a program can fail or abort at runtime and we know that we will still need to forward
        // the event in that case (either to the main output or `dropped`, depending on the
        // config), we need to clone the original event and keep it around, to allow us to discard
        // any mutations made to the event while the VRL program runs, before it failed or aborted.
        //
        // The `drop_on_{error, abort}` transform config allows operators to remove events from the
        // main output if they're failed or aborted, in which case we can skip the cloning, since
        // any mutations made by VRL will be ignored regardless. If they hav configured
        // `reroute_dropped`, however, we still need to do the clone to ensure that we can forward
        // the event to the `dropped` output.
        let forward_on_error = !self.drop_on_error || self.reroute_dropped;
        let forward_on_abort = !self.drop_on_abort || self.reroute_dropped;
        let original_event = if (self.program.can_fail() && forward_on_error)
            || (self.program.can_abort() && forward_on_abort)
        {
            Some(event.clone())
        } else {
            None
        };

        let mut target: VrlTarget = event.into();

        let result = self
            .runtime
            .resolve(&mut target, &self.program, &self.timezone);
        self.runtime.clear();

        match result {
            Ok(_) => {
                for event in target.into_events() {
                    output.push(event)
                }
            }
            Err(Terminate::Abort(error)) => {
                emit!(&RemapMappingAbort {
                    event_dropped: self.drop_on_abort,
                });

                if !self.drop_on_abort {
                    output.push(original_event.expect("event will be set"))
                } else if self.reroute_dropped {
                    let mut event = original_event.expect("event will be set");
                    self.annotate_dropped(&mut event, "abort", error);
                    err_output.push(event)
                }
            }
            Err(Terminate::Error(error)) => {
                emit!(&RemapMappingError {
                    error: error.to_string(),
                    event_dropped: self.drop_on_error,
                });

                if !self.drop_on_error {
                    output.push(original_event.expect("event will be set"))
                } else if self.reroute_dropped {
                    let mut event = original_event.expect("event will be set");
                    self.annotate_dropped(&mut event, "error", error);
                    err_output.push(event)
                }
            }
        }
    }
}

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("must provide exactly one of `source` or `file` configuration"))]
    SourceAndOrFile,

    #[snafu(display("Could not open vrl program {:?}: {}", path, source))]
    FileOpenFailed { path: PathBuf, source: io::Error },
    #[snafu(display("Could not read vrl program {:?}: {}", path, source))]
    FileReadFailed { path: PathBuf, source: io::Error },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{
            metric::{MetricKind, MetricValue},
            LogEvent, Metric, Value,
        },
        transforms::test::transform_one,
    };
    use indoc::{formatdoc, indoc};
    use shared::btreemap;
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RemapConfig>();
    }

    #[test]
    fn config_missing_source_and_file() {
        let config = RemapConfig {
            source: None,
            file: None,
            ..Default::default()
        };

        let err = Remap::new(config, &Default::default())
            .unwrap_err()
            .to_string();
        assert_eq!(
            &err,
            "must provide exactly one of `source` or `file` configuration"
        )
    }

    #[test]
    fn config_both_source_and_file() {
        let config = RemapConfig {
            source: Some("".to_owned()),
            file: Some("".into()),
            ..Default::default()
        };

        let err = Remap::new(config, &Default::default())
            .unwrap_err()
            .to_string();
        assert_eq!(
            &err,
            "must provide exactly one of `source` or `file` configuration"
        )
    }

    fn get_field_string(event: &Event, field: &str) -> String {
        event.as_log().get(field).unwrap().to_string_lossy()
    }

    #[test]
    fn check_remap_doesnt_share_state_between_events() {
        let conf = RemapConfig {
            source: Some(".foo = .sentinel".to_string()),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();
        assert!(tform.runtime().is_empty());

        let event1 = {
            let mut event1 = LogEvent::from("event1");
            event1.insert("sentinel", "bar");
            Event::from(event1)
        };
        let metadata1 = event1.metadata().clone();
        let result1 = transform_one(&mut tform, event1).unwrap();
        assert_eq!(get_field_string(&result1, "message"), "event1");
        assert_eq!(get_field_string(&result1, "foo"), "bar");
        assert_eq!(result1.metadata(), &metadata1);
        assert!(tform.runtime().is_empty());

        let event2 = {
            let event2 = LogEvent::from("event2");
            Event::from(event2)
        };
        let metadata2 = event2.metadata().clone();
        let result2 = transform_one(&mut tform, event2).unwrap();
        assert_eq!(get_field_string(&result2, "message"), "event2");
        assert_eq!(result2.as_log().get("foo"), Some(&Value::Null));
        assert_eq!(result2.metadata(), &metadata2);
        assert!(tform.runtime().is_empty());
    }

    #[test]
    fn check_remap_adds() {
        let event = {
            let mut event = LogEvent::from("augment me");
            event.insert("copy_from", "buz");
            Event::from(event)
        };
        let metadata = event.metadata().clone();

        let conf = RemapConfig {
            source: Some(
                r#"  .foo = "bar"
  .bar = "baz"
  .copy = .copy_from
"#
                .to_string(),
            ),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let result = transform_one(&mut tform, event).unwrap();
        assert_eq!(get_field_string(&result, "message"), "augment me");
        assert_eq!(get_field_string(&result, "copy_from"), "buz");
        assert_eq!(get_field_string(&result, "foo"), "bar");
        assert_eq!(get_field_string(&result, "bar"), "baz");
        assert_eq!(get_field_string(&result, "copy"), "buz");
        assert_eq!(result.metadata(), &metadata);
    }

    #[test]
    fn check_remap_emits_multiple() {
        let event = {
            let mut event = LogEvent::from("augment me");
            event.insert(
                "events",
                vec![btreemap!("message" => "foo"), btreemap!("message" => "bar")],
            );
            Event::from(event)
        };
        let metadata = event.metadata().clone();

        let conf = RemapConfig {
            source: Some(
                indoc! {r#"
                . = .events
            "#}
                .to_owned(),
            ),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let mut result = vec![];
        let mut err_result = vec![];
        tform.transform(&mut result, &mut err_result, event);

        assert_eq!(get_field_string(&result[0], "message"), "foo");
        assert_eq!(get_field_string(&result[1], "message"), "bar");
        assert_eq!(result[0].metadata(), &metadata);
        assert_eq!(result[1].metadata(), &metadata);
    }

    #[test]
    fn check_remap_error() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                .foo = "foo"
                .not_an_int = int!(.bar)
                .baz = 12
            "#}),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let event = transform_one(&mut tform, event).unwrap();

        assert_eq!(event.as_log().get("bar"), Some(&Value::from("is a string")));
        assert!(event.as_log().get("foo").is_none());
        assert!(event.as_log().get("baz").is_none());
    }

    #[test]
    fn check_remap_error_drop() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                .foo = "foo"
                .not_an_int = int!(.bar)
                .baz = 12
            "#}),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        assert!(transform_one(&mut tform, event).is_none())
    }

    #[test]
    fn check_remap_error_infallible() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                .foo = "foo"
                .baz = 12
            "#}),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let event = transform_one(&mut tform, event).unwrap();

        assert_eq!(event.as_log().get("foo"), Some(&Value::from("foo")));
        assert_eq!(event.as_log().get("bar"), Some(&Value::from("is a string")));
        assert_eq!(event.as_log().get("baz"), Some(&Value::from(12)));
    }

    #[test]
    fn check_remap_abort() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                .foo = "foo"
                abort
                .baz = 12
            "#}),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let event = transform_one(&mut tform, event).unwrap();

        assert_eq!(event.as_log().get("bar"), Some(&Value::from("is a string")));
        assert!(event.as_log().get("foo").is_none());
        assert!(event.as_log().get("baz").is_none());
    }

    #[test]
    fn check_remap_abort_drop() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                .foo = "foo"
                abort
                .baz = 12
            "#}),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: true,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        assert!(transform_one(&mut tform, event).is_none())
    }

    #[test]
    fn check_remap_metric() {
        let metric = Event::Metric(Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        ));
        let metadata = metric.metadata().clone();

        let conf = RemapConfig {
            source: Some(
                r#".tags.host = "zoobub"
                       .name = "zork"
                       .namespace = "zerk"
                       .kind = "incremental""#
                    .to_string(),
            ),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let result = transform_one(&mut tform, metric).unwrap();
        assert_eq!(
            result,
            Event::Metric(
                Metric::new_with_metadata(
                    "zork",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                    metadata,
                )
                .with_namespace(Some("zerk"))
                .with_tags(Some({
                    let mut tags = BTreeMap::new();
                    tags.insert("host".into(), "zoobub".into());
                    tags
                }))
            )
        );
    }

    #[test]
    fn check_remap_branching() {
        let happy = Event::try_from(serde_json::json!({"hello": "world"})).unwrap();
        let abort = Event::try_from(serde_json::json!({"hello": "goodbye"})).unwrap();
        let error = Event::try_from(serde_json::json!({"hello": 42})).unwrap();

        let happy_metric = {
            let mut metric = Metric::new(
                "counter",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            );
            metric.insert_tag("hello".into(), "world".into());
            Event::Metric(metric)
        };

        let abort_metric = {
            let mut metric = Metric::new(
                "counter",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            );
            metric.insert_tag("hello".into(), "goodbye".into());
            Event::Metric(metric)
        };

        let error_metric = {
            let mut metric = Metric::new(
                "counter",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            );
            metric.insert_tag("not_hello".into(), "oops".into());
            Event::Metric(metric)
        };

        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                if exists(.tags) {{
                    # metrics
                    .tags.foo = "bar"
                    if string!(.tags.hello) == "goodbye" {{
                      abort
                    }}
                }} else {{
                    # logs
                    .foo = "bar"
                    if string!(.hello) == "goodbye" {{
                      abort
                    }}
                }}
            "#}),
            drop_on_error: true,
            drop_on_abort: true,
            reroute_dropped: true,
            ..Default::default()
        };
        let context = TransformContext {
            key: Some(ComponentKey::from("remapper")),
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &context).unwrap();

        let output = transform_one_fallible(&mut tform, happy).unwrap();
        let log = output.as_log();
        assert_eq!(log["hello"], "world".into());
        assert_eq!(log["foo"], "bar".into());
        assert!(!log.contains("metadata"));

        let output = transform_one_fallible(&mut tform, abort).unwrap_err();
        let log = output.as_log();
        assert_eq!(log["hello"], "goodbye".into());
        assert!(!log.contains("foo"));
        assert_eq!(
            log["metadata"],
            serde_json::json!({
                "dropped": {
                    "reason": "abort",
                    "message": "aborted",
                    "component_id": "remapper",
                    "component_type": "remap",
                    "component_kind": "transform",
                }
            })
            .try_into()
            .unwrap()
        );

        let output = transform_one_fallible(&mut tform, error).unwrap_err();
        let log = output.as_log();
        assert_eq!(log["hello"], 42.into());
        assert!(!log.contains("foo"));
        assert_eq!(
            log["metadata"],
            serde_json::json!({
                "dropped": {
                    "reason": "error",
                    "message": "function call error for \"string\" at (160:175): expected \"string\", got \"integer\"",
                    "component_id": "remapper",
                    "component_type": "remap",
                    "component_kind": "transform",
                }
            })
            .try_into()
            .unwrap()
        );

        let output = transform_one_fallible(&mut tform, happy_metric).unwrap();
        pretty_assertions::assert_eq!(
            output,
            Event::Metric(
                Metric::new(
                    "counter",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_tags(Some({
                    let mut tags = BTreeMap::new();
                    tags.insert("hello".into(), "world".into());
                    tags.insert("foo".into(), "bar".into());
                    tags
                }))
            )
        );

        let output = transform_one_fallible(&mut tform, abort_metric).unwrap_err();
        pretty_assertions::assert_eq!(
            output,
            Event::Metric(
                Metric::new(
                    "counter",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_tags(Some({
                    let mut tags = BTreeMap::new();
                    tags.insert("hello".into(), "goodbye".into());
                    tags.insert("metadata.dropped.reason".into(), "abort".into());
                    tags.insert("metadata.dropped.component_id".into(), "remapper".into());
                    tags.insert("metadata.dropped.component_type".into(), "remap".into());
                    tags.insert("metadata.dropped.component_kind".into(), "transform".into());
                    tags
                }))
            )
        );

        let output = transform_one_fallible(&mut tform, error_metric).unwrap_err();
        pretty_assertions::assert_eq!(
            output,
            Event::Metric(
                Metric::new(
                    "counter",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_tags(Some({
                    let mut tags = BTreeMap::new();
                    tags.insert("not_hello".into(), "oops".into());
                    tags.insert("metadata.dropped.reason".into(), "error".into());
                    tags.insert("metadata.dropped.component_id".into(), "remapper".into());
                    tags.insert("metadata.dropped.component_type".into(), "remap".into());
                    tags.insert("metadata.dropped.component_kind".into(), "transform".into());
                    tags
                }))
            )
        );
    }

    #[test]
    fn check_remap_branching_disabled() {
        let happy = Event::try_from(serde_json::json!({"hello": "world"})).unwrap();
        let abort = Event::try_from(serde_json::json!({"hello": "goodbye"})).unwrap();
        let error = Event::try_from(serde_json::json!({"hello": 42})).unwrap();

        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                if exists(.tags) {{
                    # metrics
                    .tags.foo = "bar"
                    if string!(.tags.hello) == "goodbye" {{
                      abort
                    }}
                }} else {{
                    # logs
                    .foo = "bar"
                    if string!(.hello) == "goodbye" {{
                      abort
                    }}
                }}
            "#}),
            drop_on_error: true,
            drop_on_abort: true,
            reroute_dropped: false,
            ..Default::default()
        };

        assert!(conf.named_outputs().is_empty());

        let context = TransformContext {
            key: Some(ComponentKey::from("remapper")),
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &context).unwrap();

        let output = transform_one_fallible(&mut tform, happy).unwrap();
        let log = output.as_log();
        assert_eq!(log["hello"], "world".into());
        assert_eq!(log["foo"], "bar".into());
        assert!(!log.contains("metadata"));

        let mut out = Vec::new();
        let mut err = Vec::new();
        tform.transform(&mut out, &mut err, abort);
        assert!(out.is_empty());
        assert!(err.is_empty());

        let mut out = Vec::new();
        let mut err = Vec::new();
        tform.transform(&mut out, &mut err, error);
        assert!(out.is_empty());
        assert!(err.is_empty());
    }

    fn transform_one_fallible(
        ft: &mut dyn FallibleFunctionTransform,
        event: Event,
    ) -> std::result::Result<Event, Event> {
        let mut buf = Vec::with_capacity(1);
        let mut err_buf = Vec::with_capacity(1);

        ft.transform(&mut buf, &mut err_buf, event);

        assert!(buf.len() < 2);
        assert!(err_buf.len() < 2);
        match (buf.pop(), err_buf.pop()) {
            (Some(good), None) => Ok(good),
            (None, Some(bad)) => Err(bad),
            (a, b) => panic!("expected output xor error output, got {:?} and {:?}", a, b),
        }
    }
}
