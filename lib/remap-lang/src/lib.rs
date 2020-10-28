mod error;
mod expression;
mod function;
mod operator;
mod parser;
mod program;
mod runtime;
mod state;
mod value;

use expression::Expr;
use operator::Operator;

pub mod prelude;
pub use error::Error;
pub use expression::{Expression, Literal, Path};
pub use function::{Argument, ArgumentList, Function, Parameter};
pub use program::Program;
pub use runtime::Runtime;
pub use state::State;
pub use value::Value;

pub type Result<T> = std::result::Result<T, Error>;

/// Any object you want to map through the remap language has to implement this
/// trait.
pub trait Object: std::fmt::Debug {
    /// Insert a given [`Value`] in the provided [`Object`].
    ///
    /// The `path` parameter determines _where_ in the given object the value
    /// should be inserted.
    ///
    /// A path contains dot-delimited segments, and can contain a combination
    /// of:
    ///
    /// * regular path segments:
    ///
    ///   ```txt
    ///   .foo.bar.baz
    ///   ```
    ///
    /// * quoted path segments:
    ///
    ///   ```txt
    ///   .foo."bar.baz"
    ///   ```
    ///
    /// * coalesced path segments:
    ///
    ///   ```txt
    ///   .foo.(bar.baz | foobar | "bar.baz").qux
    ///   ```
    ///
    /// * path indices:
    ///
    ///   ```txt
    ///   .foo[2]
    ///   ```
    ///
    /// When inserting into a coalesced path, the implementor is encouraged to
    /// insert into the right-most segment if none exists, but can return an
    /// error if needed.
    fn insert(&mut self, path: &[Vec<String>], value: Value) -> std::result::Result<(), String>;

    /// Find a value for a given path.
    ///
    /// See [`Object::insert`] for more details.
    fn find(&self, path: &[Vec<String>]) -> std::result::Result<Option<Value>, String>;

    /// Get the list of paths in the object.
    ///
    /// Paths are represented similar to what's documented in [`Object::insert`].
    fn paths(&self) -> Vec<String>;

    /// Remove the given path from the object.
    ///
    /// If `compact` is true, after deletion, if an empty object or array is
    /// left behind, it should be removed as well.
    fn remove(&mut self, path: &str, compact: bool);
}

impl Object for std::collections::HashMap<String, Value> {
    fn insert(&mut self, path: &[Vec<String>], value: Value) -> std::result::Result<(), String> {
        self.insert(vec_path_to_string(path), value);

        Ok(())
    }

    fn find(&self, path: &[Vec<String>]) -> std::result::Result<Option<Value>, String> {
        Ok(self.get(&vec_path_to_string(path)).cloned())
    }

    fn paths(&self) -> Vec<String> {
        self.keys().cloned().collect::<Vec<_>>()
    }

    fn remove(&mut self, path: &str, _: bool) {
        self.remove(path);
    }
}

impl Object for std::collections::BTreeMap<String, Value> {
    fn insert(&mut self, path: &[Vec<String>], value: Value) -> std::result::Result<(), String> {
        self.insert(vec_path_to_string(path), value);

        Ok(())
    }

    fn find(&self, path: &[Vec<String>]) -> std::result::Result<Option<Value>, String> {
        Ok(self.get(&vec_path_to_string(path)).cloned())
    }

    fn paths(&self) -> Vec<String> {
        self.keys().cloned().collect::<Vec<_>>()
    }

    fn remove(&mut self, path: &str, _: bool) {
        self.remove(path);
    }
}

fn vec_path_to_string(path: &[Vec<String>]) -> String {
    path.iter()
        .map(|v| v.join("."))
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn it_works() {
        let cases: Vec<(&str, Result<Option<Value>>)> = vec![
            (r#".foo = null || "bar""#, Ok(Some("bar".into()))),
            // (r#".foo == .bar"#, Ok(Some(Value::Boolean(false)))),
            (
                r#".foo == .bar"#,
                Err(
                    expression::Error::Path(expression::path::Error::Missing("foo".to_owned()))
                        .into(),
                ),
            ),
            (r#".foo = (null || "bar")"#, Ok(Some("bar".into()))),
            (r#"!false"#, Ok(Some(true.into()))),
            (r#"!!false"#, Ok(Some(false.into()))),
            (r#"!!!true"#, Ok(Some(false.into()))),
            (r#"if true { "yes" } else { "no" }"#, Ok(Some("yes".into()))),
            // (
            //     r#".a.b.(c | d) == .e."f.g"[2].(h | i)"#,
            //     Ok(Some(Value::Boolean(false))),
            // ),
            (".bar = true\n.foo = .bar", Ok(Some(Value::Boolean(true)))),
            (
                r#"{
                    .foo = "foo"
                    .foo = .foo + "bar"
                    .foo
                }"#,
                Ok(Some("foobar".into())),
            ),
            (
                r#"
                    .foo = false
                    false || (.foo = true) && true
                    .foo
                "#,
                Ok(Some(true.into())),
            ),
        ];

        for (script, result) in cases {
            let program = Program::new(script, vec![])
                .map_err(|e| {
                    println!("{}", &e);
                    e
                })
                .unwrap();
            let mut runtime = Runtime::new(State::default());
            let mut event = HashMap::default();

            assert_eq!(runtime.execute(&mut event, &program), result);
        }
    }
}
