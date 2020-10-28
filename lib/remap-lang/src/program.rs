use crate::{parser, Error, Expr, Function, Result};
use pest::Parser;

/// The program to execute.
///
/// This object is passed to [`Runtime::execute`](crate::Runtime::execute).
///
/// You can create a program using [`Program::from_str`]. The provided string
/// will be parsed. If parsing fails, an [`Error`] is returned.
#[derive(Debug)]
pub struct Program {
    pub(crate) expressions: Vec<Expr>,
}

impl Program {
    pub fn new(source: &str, function_definitions: Vec<Box<dyn Function>>) -> Result<Self> {
        let pairs = parser::Parser::parse(parser::Rule::program, source)
            .map_err(|s| Error::Parser(s.to_string()))?;

        let parser = parser::Parser {
            function_definitions,
        };
        let expressions = parser.pairs_to_expressions(pairs)?;

        Ok(Self { expressions })
    }
}
