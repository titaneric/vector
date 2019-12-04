use crate::{
    buffers::Acker,
    conditions,
    dns::Resolver,
    event::{Event, Metric},
    runtime::TaskExecutor,
    sinks, sources, transforms,
};
use component::ComponentDescription;
use futures::sync::mpsc;
use indexmap::IndexMap; // IndexMap preserves insertion order, allowing us to output errors in the same order they are present in the file
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::fs::DirBuilder;
use std::{collections::HashMap, path::PathBuf};

pub mod component;
mod validation;
mod vars;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(flatten)]
    pub global: GlobalOptions,
    #[serde(default)]
    pub sources: IndexMap<String, Box<dyn SourceConfig>>,
    #[serde(default)]
    pub sinks: IndexMap<String, SinkOuter>,
    #[serde(default)]
    pub transforms: IndexMap<String, TransformOuter>,
    #[serde(default)]
    pub tests: Vec<TestDefinition>,
}

#[derive(Default, Debug, Deserialize, Serialize)]
pub struct GlobalOptions {
    #[serde(default = "default_data_dir")]
    pub data_dir: Option<PathBuf>,
    #[serde(default)]
    pub dns_servers: Vec<String>,
}

pub fn default_data_dir() -> Option<PathBuf> {
    Some(PathBuf::from("/var/lib/vector/"))
}

#[derive(Debug, Snafu)]
pub enum DataDirError {
    #[snafu(display("data_dir option required, but not given here or globally"))]
    MissingDataDir,
    #[snafu(display("data_dir {:?} does not exist", data_dir))]
    DoesNotExist { data_dir: PathBuf },
    #[snafu(display("data_dir {:?} is not writable", data_dir))]
    NotWritable { data_dir: PathBuf },
    #[snafu(display(
        "Could not create subdirectory {:?} inside of data dir {:?}: {}",
        subdir,
        data_dir,
        source
    ))]
    CouldNotCreate {
        subdir: PathBuf,
        data_dir: PathBuf,
        source: std::io::Error,
    },
}

impl GlobalOptions {
    /// Resolve the `data_dir` option in either the global or local
    /// config, and validate that it exists and is writable.
    pub fn resolve_and_validate_data_dir(
        &self,
        local_data_dir: Option<&PathBuf>,
    ) -> crate::Result<PathBuf> {
        let data_dir = local_data_dir
            .or(self.data_dir.as_ref())
            .ok_or_else(|| DataDirError::MissingDataDir)
            .map_err(Box::new)?
            .to_path_buf();
        if !data_dir.exists() {
            return Err(DataDirError::DoesNotExist { data_dir }.into());
        }
        let readonly = std::fs::metadata(&data_dir)
            .map(|meta| meta.permissions().readonly())
            .unwrap_or(true);
        if readonly {
            return Err(DataDirError::NotWritable { data_dir }.into());
        }
        Ok(data_dir)
    }

    /// Resolve the `data_dir` option using
    /// `resolve_and_validate_data_dir` and then ensure a named
    /// subdirectory exists.
    pub fn resolve_and_make_data_subdir(
        &self,
        local: Option<&PathBuf>,
        subdir: &str,
    ) -> crate::Result<PathBuf> {
        let data_dir = self.resolve_and_validate_data_dir(local)?;

        let mut data_subdir = data_dir.clone();
        data_subdir.push(subdir);

        DirBuilder::new()
            .recursive(true)
            .create(&data_subdir)
            .with_context(|| CouldNotCreate { subdir, data_dir })?;
        Ok(data_subdir)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Any,
    Log,
    Metric,
}

#[typetag::serde(tag = "type")]
pub trait SourceConfig: core::fmt::Debug {
    fn build(
        &self,
        name: &str,
        globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<sources::Source>;

    fn output_type(&self) -> DataType;

    fn source_type(&self) -> &'static str;
}

pub type SourceDescription = ComponentDescription<Box<dyn SourceConfig>>;

inventory::collect!(SourceDescription);

#[derive(Deserialize, Serialize, Debug)]
pub struct SinkOuter {
    #[serde(default)]
    pub buffer: crate::buffers::BufferConfig,
    #[serde(default = "healthcheck_default")]
    pub healthcheck: bool,
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Box<dyn SinkConfig>,
}

#[typetag::serde(tag = "type")]
pub trait SinkConfig: core::fmt::Debug {
    fn build(&self, cx: SinkContext) -> crate::Result<(sinks::RouterSink, sinks::Healthcheck)>;

    fn input_type(&self) -> DataType;

    fn sink_type(&self) -> &'static str;
}

#[derive(Debug, Clone)]
pub struct SinkContext {
    pub(super) acker: Acker,
    pub(super) resolver: Resolver,
}

impl SinkContext {
    #[cfg(test)]
    pub fn new_test(exec: TaskExecutor) -> Self {
        Self {
            acker: Acker::Null,
            resolver: Resolver::new(Vec::new(), exec).unwrap(),
        }
    }

    pub fn acker(&self) -> Acker {
        self.acker.clone()
    }

    pub fn resolver(&self) -> Resolver {
        self.resolver.clone()
    }
}

pub type SinkDescription = ComponentDescription<Box<dyn SinkConfig>>;

inventory::collect!(SinkDescription);

#[derive(Deserialize, Serialize, Debug)]
pub struct TransformOuter {
    pub inputs: Vec<String>,
    #[serde(flatten)]
    pub inner: Box<dyn TransformConfig>,
}

#[typetag::serde(tag = "type")]
pub trait TransformConfig: core::fmt::Debug {
    fn build(&self, exec: TaskExecutor) -> crate::Result<Box<dyn transforms::Transform>>;

    fn input_type(&self) -> DataType;

    fn output_type(&self) -> DataType;

    fn transform_type(&self) -> &'static str;
}

pub type TransformDescription = ComponentDescription<Box<dyn TransformConfig>>;

inventory::collect!(TransformDescription);

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestDefinition {
    pub name: String,
    pub input: TestInput,
    pub outputs: Vec<TestOutput>,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(untagged)]
pub enum TestInputValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestInput {
    pub insert_at: String,
    #[serde(default = "default_test_input_type", rename = "type")]
    pub type_str: String,
    pub value: Option<String>,
    pub log_fields: Option<IndexMap<String, TestInputValue>>,
    pub metric: Option<Metric>,
}

fn default_test_input_type() -> String {
    "raw".to_string()
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct TestOutput {
    pub extract_from: String,
    pub conditions: Vec<TestCondition>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum TestCondition {
    String(String),
    Embedded(Box<dyn conditions::ConditionConfig>),
}

// Helper methods for programming construction during tests
impl Config {
    pub fn empty() -> Self {
        Self {
            global: GlobalOptions {
                data_dir: None,
                dns_servers: Vec::new(),
            },
            sources: IndexMap::new(),
            sinks: IndexMap::new(),
            transforms: IndexMap::new(),
            tests: Vec::new(),
        }
    }

    pub fn add_source<S: SourceConfig + 'static>(&mut self, name: &str, source: S) {
        self.sources.insert(name.to_string(), Box::new(source));
    }

    pub fn add_sink<S: SinkConfig + 'static>(&mut self, name: &str, inputs: &[&str], sink: S) {
        let inputs = inputs.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        let sink = SinkOuter {
            buffer: Default::default(),
            healthcheck: true,
            inner: Box::new(sink),
            inputs,
        };

        self.sinks.insert(name.to_string(), sink);
    }

    pub fn add_transform<T: TransformConfig + 'static>(
        &mut self,
        name: &str,
        inputs: &[&str],
        transform: T,
    ) {
        let inputs = inputs.iter().map(|&s| s.to_owned()).collect::<Vec<_>>();
        let transform = TransformOuter {
            inner: Box::new(transform),
            inputs,
        };

        self.transforms.insert(name.to_string(), transform);
    }

    pub fn load(mut input: impl std::io::Read) -> Result<Self, Vec<String>> {
        let mut source_string = String::new();
        input
            .read_to_string(&mut source_string)
            .map_err(|e| vec![e.to_string()])?;

        let mut vars = std::env::vars().collect::<HashMap<_, _>>();
        if !vars.contains_key("HOSTNAME") {
            if let Some(hostname) = hostname::get_hostname() {
                vars.insert("HOSTNAME".into(), hostname);
            }
        }
        let with_vars = vars::interpolate(&source_string, &vars);

        toml::from_str(&with_vars).map_err(|e| vec![e.to_string()])
    }

    pub fn contains_cycle(&self) -> bool {
        validation::contains_cycle(self)
    }

    pub fn typecheck(&self) -> Result<(), Vec<String>> {
        validation::typecheck(self)
    }
}

impl Clone for Config {
    fn clone(&self) -> Self {
        // This is a hack around the issue of cloning
        // trait objects. So instead to clone the config
        // we first serialize it into json, then back from
        // json. Originally we used toml here but toml does not
        // support serializing `None`.
        let json = serde_json::to_vec(self).unwrap();
        serde_json::from_slice(&json[..]).unwrap()
    }
}

fn healthcheck_default() -> bool {
    true
}

#[cfg(test)]
mod test {
    use super::Config;
    use std::path::PathBuf;

    #[test]
    fn default_data_dir() {
        let config: Config = toml::from_str(
            r#"
      [sources.in]
      type = "file"
      include = ["/var/log/messages"]

      [sinks.out]
      type = "console"
      inputs = ["in"]
      encoding = "json"
      "#,
        )
        .unwrap();

        assert_eq!(
            Some(PathBuf::from("/var/lib/vector")),
            config.global.data_dir
        )
    }
}
