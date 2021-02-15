use crate::expression::{Expr, Expression, FunctionArgument, Literal, Query};
use crate::parser::Node;
use crate::value::Kind;
use crate::{Span, Value};
use diagnostic::{DiagnosticError, Label};
use std::collections::HashMap;
use std::fmt;

pub type Compiled = Result<Box<dyn Expression>, Box<dyn DiagnosticError>>;

pub trait Function: Sync + fmt::Debug {
    /// The identifier by which the function can be called.
    fn identifier(&self) -> &'static str;

    /// A brief single-line description explaining what this function does.
    fn summary(&self) -> &'static str {
        "TODO"
    }

    /// A more elaborate multi-paragraph description on how to use the function.
    fn usage(&self) -> &'static str {
        "TODO"
    }

    /// One or more examples demonstrating usage of the function in VRL source
    /// code.
    fn examples(&self) -> &'static [Example];
    // fn examples(&self) -> &'static [Example] {
    //     &[/* ODO */]
    // }

    /// Compile a [`Function`] into a type that can be resolved to an
    /// [`Expression`].
    ///
    /// This function is called at compile-time for any `Function` used in the
    /// program.
    ///
    /// At runtime, the `Expression` returned by this function is executed and
    /// resolved to its final [`Value`].
    fn compile(&self, arguments: ArgumentList) -> Compiled;

    /// An optional list of parameters the function accepts.
    ///
    /// This list is used at compile-time to check function arity, keyword names
    /// and argument type definition.
    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Example {
    pub title: &'static str,
    pub source: &'static str,
    pub result: Result<&'static str, &'static str>,
}

// -----------------------------------------------------------------------------

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Parameter {
    /// The keyword of the parameter.
    ///
    /// Arguments can be passed in both using the keyword, or as a positional
    /// argument.
    pub keyword: &'static str,

    /// The type kind(s) this parameter expects to receive.
    ///
    /// If an invalid kind is provided, the compiler will return a compile-time
    /// error.
    pub kind: u16,

    /// Whether or not this is a required parameter.
    ///
    /// If it isn't, the function can be called without errors, even if the
    /// argument matching this parameter is missing.
    pub required: bool,
}

impl Parameter {
    pub fn kind(&self) -> Kind {
        Kind::new(self.kind)
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct ArgumentList(HashMap<&'static str, Expr>);

impl ArgumentList {
    pub fn optional(&mut self, keyword: &'static str) -> Option<Box<dyn Expression>> {
        self.optional_expr(keyword).map(|v| Box::new(v) as _)
    }

    pub fn required(&mut self, keyword: &'static str) -> Box<dyn Expression> {
        Box::new(self.required_expr(keyword)) as _
    }

    pub fn optional_literal(&mut self, keyword: &'static str) -> Result<Option<Literal>, Error> {
        self.optional_expr(keyword)
            .map(|expr| match expr {
                Expr::Literal(literal) => Ok(literal),
                Expr::Variable(var) if var.value().is_some() => {
                    match var.value().unwrap().clone().into_expr() {
                        Expr::Literal(literal) => Ok(literal),
                        expr => Err(Error::UnexpectedExpression {
                            keyword,
                            expected: "literal",
                            expr,
                        }),
                    }
                }
                expr => Err(Error::UnexpectedExpression {
                    keyword,
                    expected: "literal",
                    expr,
                }),
            })
            .transpose()
    }

    pub fn required_literal(&mut self, keyword: &'static str) -> Result<Literal, Error> {
        Ok(required(self.optional_literal(keyword)?))
    }

    pub fn optional_enum(
        &mut self,
        keyword: &'static str,
        variants: &[Value],
    ) -> Result<Option<Value>, Error> {
        self.optional_literal(keyword)?
            .map(|literal| literal.to_value())
            .map(|value| {
                variants
                    .iter()
                    .find(|v| *v == &value)
                    .cloned()
                    .ok_or(Error::InvalidEnumVariant {
                        keyword,
                        value,
                        variants: variants.to_vec(),
                    })
            })
            .transpose()
    }

    pub fn required_enum(
        &mut self,
        keyword: &'static str,
        variants: &[Value],
    ) -> Result<Value, Error> {
        Ok(required(self.optional_enum(keyword, variants)?))
    }

    pub fn optional_query(&mut self, keyword: &'static str) -> Result<Option<Query>, Error> {
        self.optional_expr(keyword)
            .map(|expr| match expr {
                Expr::Query(query) => Ok(query),
                expr => Err(Error::UnexpectedExpression {
                    keyword,
                    expected: "query",
                    expr,
                }),
            })
            .transpose()
    }

    pub fn required_query(&mut self, keyword: &'static str) -> Result<Query, Error> {
        Ok(required(self.optional_query(keyword)?))
    }

    pub fn optional_regex(&mut self, keyword: &'static str) -> Result<Option<regex::Regex>, Error> {
        self.optional_expr(keyword)
            .map(|expr| match expr {
                Expr::Literal(Literal::Regex(regex)) => Ok((*regex).clone()),
                expr => Err(Error::UnexpectedExpression {
                    keyword,
                    expected: "regex",
                    expr,
                }),
            })
            .transpose()
    }

    pub fn required_regex(&mut self, keyword: &'static str) -> Result<regex::Regex, Error> {
        Ok(required(self.optional_regex(keyword)?))
    }

    pub(crate) fn keywords(&self) -> Vec<&'static str> {
        self.0.keys().copied().collect::<Vec<_>>()
    }

    pub(crate) fn insert(&mut self, k: &'static str, v: Expr) {
        self.0.insert(k, v);
    }

    fn optional_expr(&mut self, keyword: &'static str) -> Option<Expr> {
        self.0.remove(keyword)
    }

    fn required_expr(&mut self, keyword: &'static str) -> Expr {
        required(self.optional_expr(keyword))
    }
}

fn required<T>(argument: Option<T>) -> T {
    argument.expect("invalid function signature")
}

impl From<HashMap<&'static str, Value>> for ArgumentList {
    fn from(map: HashMap<&'static str, Value>) -> Self {
        Self(
            map.into_iter()
                .map(|(k, v)| (k, v.into_expr()))
                .collect::<HashMap<_, _>>(),
        )
    }
}

impl From<Vec<Node<FunctionArgument>>> for ArgumentList {
    fn from(arguments: Vec<Node<FunctionArgument>>) -> Self {
        let arguments = arguments
            .into_iter()
            .map(|arg| {
                let arg = arg.into_inner();
                // TODO: find a better API design that doesn't require unwrapping.
                let key = arg.parameter().expect("exists").keyword;
                let expr = arg.into_inner();

                (key, expr)
            })
            .collect::<HashMap<_, _>>();

        Self(arguments)
    }
}

// -----------------------------------------------------------------------------

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("unexpected expression type")]
    UnexpectedExpression {
        keyword: &'static str,
        expected: &'static str,
        expr: Expr,
    },

    #[error(r#"invalid enum variant""#)]
    InvalidEnumVariant {
        keyword: &'static str,
        value: Value,
        variants: Vec<Value>,
    },
}

impl diagnostic::DiagnosticError for Error {
    fn code(&self) -> usize {
        use Error::*;

        match self {
            UnexpectedExpression { .. } => 400,
            InvalidEnumVariant { .. } => 401,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use Error::*;

        match self {
            UnexpectedExpression {
                keyword,
                expected,
                expr,
            } => vec![
                Label::primary(
                    format!(r#"unexpected expression for argument "{}""#, keyword),
                    Span::default(),
                ),
                Label::context(format!("received: {}", expr.as_str()), Span::default()),
                Label::context(format!("expected: {}", expected), Span::default()),
            ],

            InvalidEnumVariant {
                keyword,
                value,
                variants,
            } => vec![
                Label::primary(
                    format!(r#"invalid enum variant for argument "{}""#, keyword),
                    Span::default(),
                ),
                Label::context(format!("received: {}", value.to_string()), Span::default()),
                Label::context(
                    format!(
                        "expected one of: {}",
                        variants
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    Span::default(),
                ),
            ],
        }
    }
}

impl From<Error> for Box<dyn diagnostic::DiagnosticError> {
    fn from(error: Error) -> Self {
        Box::new(error) as _
    }
}
