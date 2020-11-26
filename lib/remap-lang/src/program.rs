use crate::{parser, state, value, Error as E, Expr, Expression, Function, RemapError, TypeDef};
use pest::Parser;
use std::fmt;

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error(transparent)]
    ResolvesTo(#[from] ResolvesToError),

    #[error("expected to be infallible, but is not")]
    Fallible,
}

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub struct ResolvesToError(TypeDef, TypeDef);

impl fmt::Display for ResolvesToError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let want = &self.0;
        let got = &self.1;

        let mut want_str = "".to_owned();
        let mut got_str = "".to_owned();

        if want.is_fallible() != got.is_fallible() {
            if want.is_fallible() {
                want_str.push_str("an error, or ");
            }

            if got.is_fallible() {
                got_str.push_str("an error, or ");
            }
        }

        want_str.push_str(&format!("{} value", want.kind));
        got_str.push_str(&format!("{} value", got.kind));

        let want_kinds: Vec<_> = want.kind.into_iter().collect();
        let got_kinds: Vec<_> = got.kind.into_iter().collect();

        if !want.kind.is_all() && want_kinds.len() > 1 {
            want_str.push('s');
        }

        if !got.kind.is_all() && got_kinds.len() > 1 {
            got_str.push('s');
        }

        write!(
            f,
            "expected to resolve to {}, but instead resolves to {}",
            want_str, got_str
        )
    }
}

/// The program to execute.
///
/// This object is passed to [`Runtime::execute`](crate::Runtime::execute).
///
/// You can create a program using [`Program::from_str`]. The provided string
/// will be parsed. If parsing fails, an [`Error`] is returned.
#[derive(Debug, Clone)]
pub struct Program {
    pub(crate) expressions: Vec<Expr>,
}

impl Program {
    pub fn new(
        source: &str,
        function_definitions: &[Box<dyn Function>],
        expected_result: TypeDef,
    ) -> Result<Self, RemapError> {
        let pairs = parser::Parser::parse(parser::Rule::program, source)
            .map_err(|s| E::Parser(s.to_string()))
            .map_err(RemapError)?;

        let compiler_state = state::Compiler::default();

        let mut parser = parser::Parser {
            function_definitions,
            compiler_state,
        };

        let expressions = parser.pairs_to_expressions(pairs).map_err(RemapError)?;

        let mut type_defs = expressions
            .iter()
            .map(|e| e.type_def(&parser.compiler_state))
            .collect::<Vec<_>>();

        let computed_result = type_defs.pop().unwrap_or(TypeDef {
            fallible: true,
            kind: value::Kind::Null,
        });

        if !expected_result.contains(&computed_result) {
            return Err(RemapError::from(E::from(Error::ResolvesTo(
                ResolvesToError(expected_result, computed_result),
            ))));
        }

        if !expected_result.is_fallible() && type_defs.iter().any(TypeDef::is_fallible) {
            return Err(RemapError::from(E::from(Error::Fallible)));
        }

        Ok(Self { expressions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value;
    use std::error::Error;

    #[test]
    fn program_test() {
        use value::Kind;

        let cases = vec![
            (".foo", TypeDef { fallible: true, ..Default::default()}, Ok(())),

            // The final expression is infallible, but the first one isn't, so
            // this isn't allowed.
            (
                ".foo\ntrue",
                TypeDef { fallible: false, ..Default::default()},
                Err("expected to be infallible, but is not".to_owned()),
            ),
            (
                ".foo",
                TypeDef::default(),
                Err("expected to resolve to any value, but instead resolves to an error, or any value".to_owned()),
            ),
            (
                ".foo",
                TypeDef {
                    fallible: false,
                    kind: Kind::Bytes,
                },
                Err("expected to resolve to string value, but instead resolves to an error, or any value".to_owned()),
            ),
            (
                "false || 2",
                TypeDef {
                    fallible: false,
                    kind: Kind::Bytes | Kind::Float,
                },
                Err("expected to resolve to string or float values, but instead resolves to integer or boolean values".to_owned()),
            ),
        ];

        for (source, expected_result, expect) in cases {
            let program = Program::new(source, &[], expected_result)
                .map(|_| ())
                .map_err(|e| {
                    e.source()
                        .and_then(|e| e.source().map(|e| e.to_string()))
                        .unwrap()
                });

            assert_eq!(program, expect);
        }
    }
}
