use super::Error as E;
use crate::{
    expression::{Path, Variable},
    path, state,
    value::Kind,
    Expr, Expression, Field, InnerTypeDef, Object, Result, Segment, TypeDef, Value,
};
use std::fmt;
use std::str::FromStr;

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("unable to insert value in path: {0}")]
    PathInsertion(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Path(Path),
    Variable(Variable),
    Infallible { ok: Box<Target>, err: Box<Target> },
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Path(path) => path.fmt(f),
            Target::Variable(var) => var.fmt(f),
            Target::Infallible { ok, err } => {
                ok.as_ref().fmt(f)?;
                f.write_str(", ")?;
                err.as_ref().fmt(f)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    target: Target,
    value: Box<Expr>,
}

/// Add the type def for this path to the compiler state.
/// We recurse down any inner typedefs and add those paths to the state too.
fn path_type_def(state: &mut state::Compiler, path: &path::Path, type_def: TypeDef) {
    let query_types = state.path_query_types_mut();

    // Remove any current typedefs that start with this path.
    query_types.retain(|key, _| !key.starts_with(&path));

    // Insert the current path type def.
    query_types.insert(path.clone(), type_def.clone());

    // Recursively insert new ones from the inner type def.
    // Note we are not handling Array inner types, since array indexing
    // is fallible (there may not be enough elements in the array) any
    // indexing needs to be handled.
    // This may change in future.
    if let Some(InnerTypeDef::Map(map)) = type_def.inner_type_def {
        for (field, type_def) in map {
            if let Ok(field) = Field::from_str(&field) {
                let mut path = path.clone();
                path.append(Segment::Field(field));
                path_type_def(state, &path, type_def);
            }
        }
    }
}

impl Assignment {
    pub fn new(target: Target, value: Box<Expr>, state: &mut state::Compiler) -> Self {
        let type_def = value.type_def(state);

        let var_type_def = |state: &mut state::Compiler, var: &Variable, type_def| {
            state
                .variable_types_mut()
                .insert(var.ident().to_owned(), type_def);
        };

        match &target {
            Target::Variable(var) => var_type_def(state, var, type_def),
            Target::Path(path) => path_type_def(state, path.as_ref(), type_def),
            Target::Infallible { ok, err } => {
                // If the type definition of the rhs expression is infallible,
                // then an infallible assignment is redundant.
                //
                // This invariant is upheld (for now) by the parser.
                assert!(type_def.is_fallible());

                // "ok" target takes on the type definition of the value, but is
                // set to being infallible, as the error will be captured by the
                // "err" target.
                let type_def = type_def.into_fallible(false);

                match ok.as_ref() {
                    Target::Variable(var) => var_type_def(state, var, type_def),
                    Target::Path(path) => path_type_def(state, path.as_ref(), type_def),
                    Target::Infallible { .. } => unimplemented!("nested infallible target"),
                }

                // "err" target is assigned `null` or a string containing the
                // error message.
                let err_type_def = TypeDef {
                    kind: Kind::Bytes | Kind::Null,
                    ..Default::default()
                };

                match err.as_ref() {
                    Target::Variable(var) => var_type_def(state, var, err_type_def),
                    Target::Path(path) => path_type_def(state, path.as_ref(), err_type_def),
                    Target::Infallible { .. } => unimplemented!("nested infallible target"),
                }
            }
        }

        Self { target, value }
    }
}

impl Expression for Assignment {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object);

        fn var_assignment<'a>(
            state: &mut state::Program,
            var: &Variable,
            value: &'a Value,
        ) -> Result<&'a Value> {
            state
                .variables_mut()
                .insert(var.ident().to_owned(), value.to_owned());

            Ok(value)
        }

        fn path_assignment<'a>(
            object: &mut dyn Object,
            path: &Path,
            value: &'a Value,
        ) -> Result<&'a Value> {
            object
                .insert(path.as_ref(), value.to_owned())
                .map_err(|e| E::Assignment(Error::PathInsertion(e)))?;

            Ok(value)
        }

        match &self.target {
            Target::Variable(var) => var_assignment(state, var, &value?).map(ToOwned::to_owned),
            Target::Path(path) => path_assignment(object, path, &value?).map(ToOwned::to_owned),
            Target::Infallible { ok, err } => {
                let (ok_value, err_value) = match value {
                    Ok(value) => (value, Value::Null),
                    Err(err) => (Value::Null, Value::from(err)),
                };

                match ok.as_ref() {
                    Target::Variable(var) => var_assignment(state, var, &ok_value)?,
                    Target::Path(path) => path_assignment(object, path, &ok_value)?,
                    Target::Infallible { .. } => unimplemented!("nested infallible target"),
                };

                match err.as_ref() {
                    Target::Variable(var) => var_assignment(state, var, &err_value)?,
                    Target::Path(path) => path_assignment(object, path, &err_value)?,
                    Target::Infallible { .. } => unimplemented!("nested infallible target"),
                };

                if err_value.is_null() {
                    Ok(ok_value)
                } else {
                    Ok(err_value)
                }
            }
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let var_type_def = |var: &Variable| {
            state
                .variable_type(var.ident().to_owned())
                .cloned()
                .expect("variable must be assigned via Assignment::new")
        };

        let path_type_def = |path: &Path| {
            state
                .path_query_type(path)
                .cloned()
                .expect("path must be assigned via Assignment::new")
        };

        match &self.target {
            Target::Variable(var) => var_type_def(var),
            Target::Path(path) => path_type_def(path),
            Target::Infallible { ok, err } => {
                let ok_type_def = match ok.as_ref() {
                    Target::Variable(var) => var_type_def(var),
                    Target::Path(path) => path_type_def(path),
                    Target::Infallible { .. } => unimplemented!("nested infallible target"),
                };

                // Technically the parser rejects this invariant, because an
                // expression that is known to be infallible cannot be assigned
                // to an infallible target, since the error will always be
                // `null`.
                if !ok_type_def.is_fallible() {
                    return ok_type_def;
                }

                let err_type_def = match err.as_ref() {
                    Target::Variable(var) => var_type_def(var),
                    Target::Path(path) => path_type_def(path),
                    Target::Infallible { .. } => unimplemented!("nested infallible target"),
                };

                ok_type_def.merge(err_type_def).into_fallible(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        expression::{Arithmetic, Literal},
        lit, path, test_type_def, Operator,
    };

    #[test]
    fn path_typedef_added_to_state() {
        let mut state = state::Compiler::default();

        // Assign a type with inner type to a path.
        let path = path::Path::from_str(".ook").unwrap();
        path_type_def(
            &mut state,
            &path,
            TypeDef::from(Kind::Map)
                .with_inner_type(Some(crate::inner_type_def!({ "flork": Kind::Integer }))),
        );

        assert_eq!(
            Some(Kind::Map),
            state
                .path_query_type(Box::new(path))
                .map(|typedef| typedef.kind)
        );

        let nested_path = path::Path::from_str(".ook.flork").unwrap();

        assert_eq!(
            Some(Kind::Integer),
            state
                .path_query_type(Box::new(nested_path.clone()))
                .map(|typedef| typedef.kind)
        );

        // Assign a different type to this path.
        let path = path::Path::from_str(".ook").unwrap();
        path_type_def(&mut state, &path, Kind::Bytes.into());

        // Ensure the inner type is no longer defined.
        assert_eq!(
            None,
            state
                .path_query_type(Box::new(nested_path))
                .map(|typedef| typedef.kind)
        );
    }

    test_type_def![
        variable {
            expr: |state: &mut state::Compiler| {
                let target = Target::Variable(Variable::new("foo".to_owned(), None));
                let value = Box::new(Literal::from(true).into());

                Assignment::new(target, value, state)
            },
            def: TypeDef {
                kind: Kind::Boolean,
                ..Default::default()
            },
        }

        path {
            expr: |state: &mut state::Compiler| {
                let target = Target::Path(Path::from("foo"));
                let value = Box::new(Literal::from("foo").into());

                Assignment::new(target, value, state)
            },
            def: TypeDef {
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        infallible {
            expr: |state: &mut state::Compiler| {
                let ok = Box::new(Target::Variable(Variable::new("ok".to_owned(), None)));
                let err = Box::new(Target::Variable(Variable::new("err".to_owned(), None)));

                let target = Target::Infallible { ok, err };
                let value = Box::new(Arithmetic::new(
                    Box::new(lit!(true).into()),
                    Box::new(lit!(3).into()),
                    Operator::Multiply,
                ).into());

                Assignment::new(target, value, state)
            },
            def: TypeDef {
                fallible: false,
                kind: Kind::Bytes | Kind::Integer | Kind::Float,
                ..Default::default()
            },
        }
    ];
}
