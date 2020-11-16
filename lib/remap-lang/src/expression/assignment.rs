use super::Error as E;
use crate::{state, Expr, Expression, Object, Result, TypeDef, Value};

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("unable to insert value in path: {0}")]
    PathInsertion(String),
}

#[derive(Debug, Clone)]
pub enum Target {
    Path(Vec<Vec<String>>),
    Variable(String),
}

#[derive(Debug, Clone)]
pub struct Assignment {
    target: Target,
    value: Box<Expr>,
}

impl Assignment {
    pub fn new(target: Target, value: Box<Expr>, state: &mut state::Compiler) -> Self {
        let type_def = value.type_def(state);

        match &target {
            Target::Variable(ident) => state.variable_types_mut().insert(ident.clone(), type_def),
            Target::Path(segments) => {
                let path = crate::expression::path::segments_to_path(segments);
                state.path_query_types_mut().insert(path, type_def)
            }
        };

        Self { target, value }
    }
}

impl Expression for Assignment {
    fn execute(
        &self,
        state: &mut state::Program,
        object: &mut dyn Object,
    ) -> Result<Option<Value>> {
        let value = self.value.execute(state, object)?;

        match value {
            None => Ok(None),
            Some(value) => {
                match &self.target {
                    Target::Variable(ident) => {
                        state.variables_mut().insert(ident.clone(), value.clone());
                    }
                    Target::Path(path) => object
                        .insert(&path, value.clone())
                        .map_err(|e| E::Assignment(Error::PathInsertion(e)))?,
                }

                Ok(Some(value))
            }
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        match &self.target {
            Target::Variable(ident) => state
                .variable_type(ident.clone())
                .cloned()
                .expect("variable must be assigned via Assignment::new"),
            Target::Path(segments) => {
                let path = crate::expression::path::segments_to_path(segments);
                state
                    .path_query_type(&path)
                    .cloned()
                    .expect("variable must be assigned via Assignment::new")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{expression::Literal, test_type_def, value::Constraint::*, value::Kind::*};

    test_type_def![
        variable {
            expr: |state: &mut state::Compiler| {
                let target = Target::Variable("foo".to_owned());
                let value = Box::new(Literal::from(true).into());

                Assignment::new(target, value, state)
            },
            def: TypeDef {
                constraint: Exact(Boolean),
                ..Default::default()
            },
        }

        path {
            expr: |state: &mut state::Compiler| {
                let target = Target::Path(vec![vec!["foo".to_owned()]]);
                let value = Box::new(Literal::from("foo").into());

                Assignment::new(target, value, state)
            },
            def: TypeDef {
                constraint: Exact(String),
                ..Default::default()
            },
        }
    ];
}
