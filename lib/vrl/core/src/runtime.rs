use crate::{state, Context, Path, Program, Target, Value};
use compiler::ExpressionError;
use std::{error::Error, fmt};

pub type RuntimeResult = Result<Value, Terminate>;

#[derive(Debug, Default)]
pub struct Runtime {
    state: state::Runtime,
}

/// The error raised if the runtime is terminated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminate {
    /// A manual `abort` call.
    ///
    /// This is an intentional termination that does not result in an
    /// `Ok(Value)` result, but should neither be interpreted as an unexpected
    /// outcome.
    Abort,

    /// An unexpected program termination.
    Error(String),
}

impl fmt::Display for Terminate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Terminate::Abort => Ok(()),
            Terminate::Error(error) => f.write_str(&error),
        }
    }
}

impl Error for Terminate {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl Runtime {
    pub fn new(state: state::Runtime) -> Self {
        Self { state }
    }

    /// Given the provided [`Target`], resolve the provided [`Program`] to
    /// completion.
    pub fn resolve(&mut self, target: &mut dyn Target, program: &Program) -> RuntimeResult {
        // Validate that the path is an object.
        //
        // VRL technically supports any `Value` object as the root, but the
        // assumption is people are expected to use it to query objects.
        match target.get(&Path::root()) {
            Ok(Some(Value::Object(_))) => {}
            Ok(Some(value)) => {
                return Err(Terminate::Error(format!(
                    "target must be a valid object, got {}: {}",
                    value.kind(),
                    value
                )))
            }
            Ok(None) => {
                return Err(Terminate::Error(
                    "expected target object, got nothing".to_owned(),
                ))
            }
            Err(err) => {
                return Err(Terminate::Error(format!(
                    "error querying target object: {}",
                    err
                )))
            }
        };

        let mut context = Context::new(target, &mut self.state);

        let mut values = program
            .iter()
            .map(|expr| {
                expr.resolve(&mut context).map_err(|err| match err {
                    ExpressionError::Abort => Terminate::Abort,
                    err @ ExpressionError::Error { .. } => Terminate::Error(err.to_string()),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(values.pop().unwrap_or(Value::Null))
    }
}
