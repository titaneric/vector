use crate::expression::{Expr, Noop, Resolved};
use crate::parser::Node;
use crate::{value::Kind, Context, Expression, Span, State, TypeDef};
use diagnostic::{DiagnosticError, Label, Note};
use std::fmt;

pub type Result = std::result::Result<Not, Error>;

#[derive(Debug, Clone, PartialEq)]
pub struct Not {
    inner: Box<Expr>,
}

impl Not {
    pub fn new(node: Node<Expr>, not_span: Span, state: &State) -> Result {
        let (expr_span, expr) = node.take();
        let type_def = expr.type_def(state);

        if !type_def.is_boolean() {
            return Err(Error {
                variant: ErrorVariant::NonBoolean(type_def.kind()),
                not_span,
                expr_span,
            });
        }

        Ok(Self {
            inner: Box::new(expr),
        })
    }

    pub fn noop() -> Self {
        Not {
            inner: Box::new(Noop.into()),
        }
    }
}

impl Expression for Not {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        Ok((!self.inner.resolve(ctx)?.unwrap_boolean()).into())
    }

    fn type_def(&self, state: &State) -> TypeDef {
        self.inner.type_def(state).boolean()
    }
}

impl fmt::Display for Not {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, r#"!{}"#, self.inner)
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug)]
pub struct Error {
    pub(crate) variant: ErrorVariant,

    not_span: Span,
    expr_span: Span,
}

#[derive(thiserror::Error, Debug)]
pub enum ErrorVariant {
    #[error("non-boolean negation")]
    NonBoolean(Kind),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#}", self.variant)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.variant)
    }
}

impl DiagnosticError for Error {
    fn code(&self) -> usize {
        use ErrorVariant::*;

        match &self.variant {
            NonBoolean(..) => 660,
        }
    }

    fn labels(&self) -> Vec<Label> {
        use ErrorVariant::*;

        match &self.variant {
            NonBoolean(kind) => vec![
                Label::primary("negation only works on boolean values", self.not_span),
                Label::context(
                    format!("this expression resolves to {}", kind),
                    self.expr_span,
                ),
            ],
        }
    }

    fn notes(&self) -> Vec<Note> {
        use ErrorVariant::*;

        match &self.variant {
            NonBoolean(..) => vec![Note::CoerceValue],
        }
    }
}
