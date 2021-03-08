use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::process::Command;

use crate::Test;

/// A list of function examples that should be skipped in the test run.
///
/// This mostly consists of functions that have a non-deterministic result.
const SKIP_FUNCTION_EXAMPLES: &[&str] = &[
    "uuid_v4",
    "strip_ansi_escape_codes",
    "get_hostname",
    "now",
    "get_env_var",
];

#[derive(Debug, Deserialize)]
pub struct Reference {
    examples: Vec<Example>,
    functions: HashMap<String, Examples>,
    expressions: HashMap<String, Examples>,
}

#[derive(Debug, Deserialize)]
pub struct Examples {
    examples: Vec<Example>,
}

#[derive(Debug, Deserialize)]
pub struct Example {
    title: String,
    #[serde(default)]
    input: Option<Event>,
    source: String,
    #[serde(rename = "return")]
    returns: Option<Value>,
    output: Option<Event>,
    raises: Option<Error>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Event {
    log: Map<String, Value>,

    // TODO: unsupported for now
    metric: Option<Map<String, Value>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Error {
    compiletime: String,
}

pub fn tests() -> Vec<Test> {
    let dir = fs::canonicalize("../../../scripts").unwrap();

    let output = Command::new("bash")
        .current_dir(dir)
        .args(&["cue.sh", "export", "-e", "remap"])
        .output()
        .expect("failed to execute process");

    let Reference {
        examples,
        functions,
        expressions,
    } = serde_json::from_slice(&output.stdout).unwrap();

    examples_to_tests("reference", {
        let mut map = HashMap::default();
        map.insert("program".to_owned(), Examples { examples });
        map
    })
    .chain(examples_to_tests("functions", functions))
    .chain(examples_to_tests("expressions", expressions))
    .collect()
}

fn examples_to_tests(
    category: &'static str,
    examples: HashMap<String, Examples>,
) -> Box<dyn Iterator<Item = Test>> {
    Box::new(
        examples
            .into_iter()
            .map(move |(k, v)| {
                v.examples
                    .into_iter()
                    .map(|example| Test::from_cue_example(category, k.clone(), example))
                    .collect::<Vec<_>>()
            })
            .flatten(),
    )
}

impl Test {
    fn from_cue_example(category: &'static str, name: String, example: Example) -> Self {
        use vrl::Value;

        let Example {
            title,
            input,
            mut source,
            returns,
            output,
            raises,
        } = example;

        let mut skip = SKIP_FUNCTION_EXAMPLES.contains(&name.as_str());

        let object = match input {
            Some(event) => {
                serde_json::from_value::<Value>(serde_json::Value::Object(event.log)).unwrap()
            }
            None => Value::Object(BTreeMap::default()),
        };

        if returns.is_some() && output.is_some() {
            panic!(
                "example must either specify return or output, not both: {}/{}",
                category, &name
            );
        }

        if let Some(output) = &output {
            if output.metric.is_some() {
                skip = true;
            }

            // when checking the output, we need to add `.` at the end of the
            // program to make sure we correctly evaluate the external object.
            source += "; .";
        }

        let result = match raises {
            Some(error) => error.compiletime,
            None => serde_json::to_string(
                &returns
                    .or_else(|| output.map(|event| serde_json::Value::Object(event.log)))
                    .unwrap_or_default(),
            )
            .unwrap(),
        };

        Self {
            name: title,
            category: format!("docs/{}/{}", category, name),
            error: None,
            source: source,
            object,
            result,
            result_approx: false,
            skip,
        }
    }
}
