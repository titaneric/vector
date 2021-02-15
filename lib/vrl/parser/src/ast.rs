use crate::lex::Error;
use diagnostic::Span;
use ordered_float::NotNan;
use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::iter::IntoIterator;
use std::ops::Deref;
use std::str::FromStr;

// -----------------------------------------------------------------------------
// node
// -----------------------------------------------------------------------------

/// A wrapper type for a node, containing span details of that given node as it
/// relates to the source input from which the node was generated.
#[derive(Clone, Eq, Ord, PartialOrd)]
pub struct Node<T> {
    pub(crate) span: Span,
    pub(crate) node: T,
}

impl<T> Node<T> {
    pub fn map<R>(self, mut f: impl FnMut(T) -> R) -> Node<R> {
        let Node { span, node } = self;

        Node {
            span,
            node: f(node),
        }
    }

    pub fn new(span: Span, node: T) -> Self {
        Self { span, node }
    }

    /// Get a copy of the [`Span`] of the node.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Get the starting byte of the node within the input source.
    pub fn start(&self) -> usize {
        self.span.start()
    }

    /// Get the ending byte of the node within the input source.
    pub fn end(&self) -> usize {
        self.span.end()
    }

    /// Get a reference to the inner node type `T`.
    pub fn inner(&self) -> &T {
        &self.node
    }

    // Consume the node, taking out the [`Span`] and inner node type `T`.
    pub fn take(self) -> (Span, T) {
        (self.span, self.node)
    }

    /// Consume the node, and get the inner node type `T`.
    pub fn into_inner(self) -> T {
        self.node
    }

    /// Consume the node and return a tuple consisting of the start, node type
    /// `T` and the end position.
    pub fn into_spanned(self) -> (usize, T, usize) {
        let Self { span, node } = self;

        (span.start(), node, span.end())
    }
}

impl<T: fmt::Debug> fmt::Debug for Node<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.node.fmt(f)
    }
}

impl<T: fmt::Display> fmt::Display for Node<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.node.fmt(f)
    }
}

impl<T> Deref for Node<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

impl<T> AsRef<T> for Node<T> {
    fn as_ref(&self) -> &T {
        &self.node
    }
}

impl<T: PartialEq> PartialEq for Node<T> {
    fn eq(&self, other: &Self) -> bool {
        self.node == other.node && self.span == other.span
    }
}

impl<T: Hash> Hash for Node<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node.hash(state);
        self.span.hash(state);
    }
}

// -----------------------------------------------------------------------------
// program
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct Program(pub Vec<Node<RootExpr>>);

impl fmt::Debug for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for expr in &self.0 {
            writeln!(f, "{:?}", expr)?;
        }

        Ok(())
    }
}

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for expr in &self.0 {
            writeln!(f, "{}", expr)?;
        }

        Ok(())
    }
}

impl Deref for Program {
    type Target = [Node<RootExpr>];

    fn deref(&self) -> &Self::Target {
        self.0.as_slice()
    }
}

impl IntoIterator for Program {
    type Item = Node<RootExpr>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

// -----------------------------------------------------------------------------
// root expression
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub enum RootExpr {
    Expr(Node<Expr>),

    /// A special expression that is returned if a given expression could not be
    /// parsed. This allows the parser to continue on to the next expression.
    Error(Error),
}

impl fmt::Debug for RootExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use RootExpr::*;

        let value = match self {
            Expr(v) => format!("{:?}", v),
            Error(v) => format!("{:?}", v),
        };

        write!(f, "RootExpr({})", value)
    }
}

impl fmt::Display for RootExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use RootExpr::*;

        match self {
            Expr(v) => v.fmt(f),
            Error(v) => v.fmt(f),
        }
    }
}

// -----------------------------------------------------------------------------
// expression
// -----------------------------------------------------------------------------

#[allow(clippy::large_enum_variant)]
#[derive(PartialEq)]
pub enum Expr {
    Literal(Node<Literal>),
    Container(Node<Container>),
    IfStatement(Node<IfStatement>),
    Op(Node<Op>),
    Assignment(Node<Assignment>),
    Query(Node<Query>),
    FunctionCall(Node<FunctionCall>),
    Variable(Node<Ident>),
    Unary(Node<Unary>),
}

impl fmt::Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Expr::*;

        let value = match self {
            Literal(v) => format!("{:?}", v),
            Container(v) => format!("{:?}", v),
            Op(v) => format!("{:?}", v),
            IfStatement(v) => format!("{:?}", v),
            Assignment(v) => format!("{:?}", v),
            Query(v) => format!("{:?}", v),
            FunctionCall(v) => format!("{:?}", v),
            Variable(v) => format!("{:?}", v),
            Unary(v) => format!("{:?}", v),
        };

        write!(f, "Expr({})", value)
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Expr::*;

        match self {
            Literal(v) => v.fmt(f),
            Container(v) => v.fmt(f),
            Op(v) => v.fmt(f),
            IfStatement(v) => v.fmt(f),
            Assignment(v) => v.fmt(f),
            Query(v) => v.fmt(f),
            FunctionCall(v) => v.fmt(f),
            Variable(v) => v.fmt(f),
            Unary(v) => v.fmt(f),
        }
    }
}

// -----------------------------------------------------------------------------
// ident
// -----------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Ident(pub(crate) String);

impl Ident {
    pub fn new(ident: impl Into<String>) -> Self {
        Self(ident.into())
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl AsRef<str> for Ident {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ident({})", self.0)
    }
}

// -----------------------------------------------------------------------------
// literals
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub enum Literal {
    String(String),
    Integer(i64),
    Float(NotNan<f64>),
    Boolean(bool),
    Regex(String),
    Timestamp(String),
    Null,
}

impl fmt::Display for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Literal::*;

        match self {
            String(v) => write!(f, r#""{}""#, v),
            Integer(v) => v.fmt(f),
            Float(v) => v.fmt(f),
            Boolean(v) => v.fmt(f),
            Regex(v) => write!(f, "r'{}'", v),
            Timestamp(v) => write!(f, "t'{}'", v),
            Null => f.write_str("null"),
        }
    }
}

impl fmt::Debug for Literal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Literal({})", self)
    }
}

// -----------------------------------------------------------------------------
// container
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub enum Container {
    Group(Box<Node<Group>>),
    Block(Node<Block>),
    Array(Node<Array>),
    Object(Node<Object>),
}

impl fmt::Display for Container {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Container::*;

        match self {
            Group(v) => v.fmt(f),
            Block(v) => v.fmt(f),
            Array(v) => v.fmt(f),
            Object(v) => v.fmt(f),
        }
    }
}

impl fmt::Debug for Container {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Container::*;

        let value = match self {
            Group(v) => format!("{:?}", v),
            Block(v) => format!("{:?}", v),
            Array(v) => format!("{:?}", v),
            Object(v) => format!("{:?}", v),
        };

        write!(f, "Container({})", value)
    }
}

// -----------------------------------------------------------------------------
// block
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct Block(pub Vec<Node<Expr>>);

impl Block {
    pub fn into_inner(self) -> Vec<Node<Expr>> {
        self.0
    }
}

impl IntoIterator for Block {
    type Item = Node<Expr>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{\n")?;

        let mut iter = self.0.iter().peekable();
        while let Some(expr) = iter.next() {
            f.write_str("\t")?;
            expr.fmt(f)?;
            if iter.peek().is_some() {
                f.write_str("\n")?;
            }
        }

        f.write_str("\n}")
    }
}

impl fmt::Debug for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Block(")?;

        let mut iter = self.0.iter().peekable();
        while let Some(expr) = iter.next() {
            expr.fmt(f)?;

            if iter.peek().is_some() {
                f.write_str("; ")?;
            }
        }

        f.write_str(")")
    }
}

// -----------------------------------------------------------------------------
// group
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct Group(pub Node<Expr>);

impl Group {
    pub fn into_inner(self) -> Node<Expr> {
        self.0
    }
}

impl fmt::Display for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, r#"({})"#, self.0)
    }
}

impl fmt::Debug for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, r#"Group({:?})"#, self.0)
    }
}

// -----------------------------------------------------------------------------
// array
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct Array(pub(crate) Vec<Node<Expr>>);

impl fmt::Display for Array {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let exprs = self
            .0
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        write!(f, "[{}]", exprs)
    }
}

impl fmt::Debug for Array {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let exprs = self
            .0
            .iter()
            .map(|e| format!("{:?}", e))
            .collect::<Vec<_>>()
            .join(", ");

        write!(f, "Array([{}])", exprs)
    }
}

impl IntoIterator for Array {
    type Item = Node<Expr>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

// -----------------------------------------------------------------------------
// object
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct Object(pub(crate) BTreeMap<Node<String>, Node<Expr>>);

impl fmt::Display for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let exprs = self
            .0
            .iter()
            .map(|(k, v)| format!(r#""{}": {}"#, k, v))
            .collect::<Vec<_>>()
            .join(", ");

        write!(f, "{{ {} }}", exprs)
    }
}

impl fmt::Debug for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let exprs = self
            .0
            .iter()
            .map(|(k, v)| format!(r#""{}": {:?}"#, k, v))
            .collect::<Vec<_>>()
            .join(", ");

        write!(f, "{{ {} }}", exprs)
    }
}

impl IntoIterator for Object {
    type Item = (Node<String>, Node<Expr>);
    type IntoIter = std::collections::btree_map::IntoIter<Node<String>, Node<Expr>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

// -----------------------------------------------------------------------------
// if statement
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct IfStatement {
    pub predicate: Node<Predicate>,
    pub consequent: Node<Block>,
    pub alternative: Option<Node<Block>>,
}

impl fmt::Debug for IfStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.alternative {
            Some(alt) => write!(
                f,
                "{:?} ? {:?} : {:?}",
                self.predicate, self.consequent, alt
            ),
            None => write!(f, "{:?} ? {:?}", self.predicate, self.consequent),
        }
    }
}

impl fmt::Display for IfStatement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("if ")?;
        self.predicate.fmt(f)?;
        f.write_str(" ")?;
        self.consequent.fmt(f)?;

        if let Some(alt) = &self.alternative {
            f.write_str(" else")?;
            alt.fmt(f)?;
        }

        Ok(())
    }
}

#[derive(PartialEq)]
pub enum Predicate {
    One(Box<Node<Expr>>),
    Many(Vec<Node<Expr>>),
}

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Predicate::One(expr) => expr.fmt(f),
            Predicate::Many(exprs) => {
                f.write_str("(")?;

                let mut iter = exprs.iter().peekable();
                while let Some(expr) = iter.next() {
                    expr.fmt(f)?;

                    if iter.peek().is_some() {
                        f.write_str("; ")?;
                    }
                }

                f.write_str(")")
            }
        }
    }
}

impl fmt::Debug for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Predicate::One(expr) => write!(f, "Predicate({:?})", expr),
            Predicate::Many(exprs) => {
                f.write_str("Predicate(")?;

                let mut iter = exprs.iter().peekable();
                while let Some(expr) = iter.next() {
                    expr.fmt(f)?;

                    if iter.peek().is_some() {
                        f.write_str("; ")?;
                    }
                }

                f.write_str(")")
            }
        }
    }
}

// -----------------------------------------------------------------------------
// operation
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct Op(pub Box<Node<Expr>>, pub Node<Opcode>, pub Box<Node<Expr>>);

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.0, self.1, self.2)
    }
}

impl fmt::Debug for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Op({:?} {} {:?})", self.0, self.1, self.2)
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Opcode {
    Mul,
    Div,
    Add,
    Sub,
    Rem,
    Or,
    And,
    Err,
    Ne,
    Eq,
    Ge,
    Gt,
    Le,
    Lt,
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl Opcode {
    pub fn as_str(self) -> &'static str {
        use Opcode::*;

        match self {
            Mul => "*",
            Div => "/",
            Add => "+",
            Sub => "-",
            Rem => "%",

            Or => "||",
            And => "&&",

            Err => "??",

            Ne => "!=",
            Eq => "==",

            Ge => ">=",
            Gt => ">",
            Le => "<=",
            Lt => "<",
        }
    }
}

impl FromStr for Opcode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        use Opcode::*;

        let op = match s {
            "*" => Mul,
            "/" => Div,
            "+" => Add,
            "-" => Sub,
            "%" => Rem,

            "||" => Or,
            "&&" => And,

            "??" => Err,

            "!=" => Ne,
            "==" => Eq,

            ">=" => Ge,
            ">" => Gt,
            "<=" => Le,
            "<" => Lt,

            _ => return std::result::Result::Err(()),
        };

        Ok(op)
    }
}

// -----------------------------------------------------------------------------
// assignment
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub enum Assignment {
    Single {
        target: Node<AssignmentTarget>,
        expr: Box<Node<Expr>>,
    },
    Infallible {
        ok: Node<AssignmentTarget>,
        err: Node<AssignmentTarget>,
        expr: Box<Node<Expr>>,
    },
    // TODO
    // Compound {
    //     target: Node<AssignmentTarget>,
    //     op: Opcode,
    //     expr: Box<Node<Expr>>,
    // }
}

impl fmt::Display for Assignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Assignment::*;

        match self {
            Single { target, expr } => write!(f, "{} = {}", target, expr),
            Infallible { ok, err, expr } => write!(f, "{}, {} = {}", ok, err, expr),
        }
    }
}

impl fmt::Debug for Assignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Assignment::*;

        match self {
            Single { target, expr } => write!(f, "{:?} = {:?}", target, expr),
            Infallible { ok, err, expr } => write!(f, "Ok({:?}), Err({:?}) = {:?}", ok, err, expr),
        }
    }
}

#[derive(PartialEq)]
pub enum AssignmentTarget {
    Noop,
    Query(Query),
    Internal(Ident, Option<Path>),
    External(Option<Path>),
}

impl fmt::Display for AssignmentTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use AssignmentTarget::*;

        match self {
            Noop => f.write_str("_"),
            Query(query) => query.fmt(f),
            Internal(ident, Some(path)) => write!(f, "{}{}", ident, path),
            Internal(ident, _) => ident.fmt(f),
            External(Some(path)) => path.fmt(f),
            External(_) => f.write_str("."),
        }
    }
}

impl fmt::Debug for AssignmentTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use AssignmentTarget::*;

        match self {
            Noop => f.write_str("Noop"),
            Query(query) => query.fmt(f),
            Internal(ident, Some(path)) => write!(f, "Internal({}{})", ident, path),
            Internal(ident, _) => write!(f, "Internal({})", ident),
            External(Some(path)) => write!(f, "External({})", path),
            External(_) => f.write_str("External(.)"),
        }
    }
}

// -----------------------------------------------------------------------------
// query
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct Query {
    pub target: Node<QueryTarget>,
    pub path: Node<Path>,
}

impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.target, self.path)
    }
}

impl fmt::Debug for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Query({:?}, {:?})", self.target, self.path)
    }
}

#[derive(PartialEq)]
pub enum QueryTarget {
    Internal(Ident),
    External,
    FunctionCall(FunctionCall),
    Container(Container),
}

impl fmt::Display for QueryTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use QueryTarget::*;

        match self {
            Internal(v) => v.fmt(f),
            External => Ok(()),
            FunctionCall(v) => v.fmt(f),
            Container(v) => v.fmt(f),
        }
    }
}

impl fmt::Debug for QueryTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use QueryTarget::*;

        match self {
            Internal(v) => write!(f, "Internal({:?})", v),
            External => f.write_str("External"),
            FunctionCall(v) => v.fmt(f),
            Container(v) => v.fmt(f),
        }
    }
}

// -----------------------------------------------------------------------------
// path
// -----------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Path(pub Vec<PathSegment>);

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for segment in &self.0 {
            segment.fmt(f)?;
        }

        Ok(())
    }
}

impl fmt::Debug for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Path({})", self)
    }
}

impl IntoIterator for Path {
    type Item = PathSegment;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum PathSegment {
    Field(Field),
    Index(i64),
    Coalesce(Vec<Field>),
}

impl fmt::Display for PathSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use PathSegment::*;

        match &self {
            Field(field) => write!(f, ".{}", field),
            Index(index) => write!(f, "[{}]", index),
            Coalesce(fields) => write!(
                f,
                "({})",
                fields
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
        }
    }
}

impl fmt::Debug for PathSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PathSegment({})", self)
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Field {
    Regular(Ident),
    Quoted(String),
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Field::Regular(ident) => ident.fmt(f),
            Field::Quoted(string) => write!(f, r#""{}""#, string),
        }
    }
}

impl fmt::Debug for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Field({})", self)
    }
}

// -----------------------------------------------------------------------------
// function call
// -----------------------------------------------------------------------------

/// A function call expression.
///
/// It contains the identifier of the function, and any arguments passed into
/// the function call.
#[derive(PartialEq)]
pub struct FunctionCall {
    pub ident: Node<Ident>,
    pub abort_on_error: bool,
    pub arguments: Vec<Node<FunctionArgument>>,
}

impl fmt::Display for FunctionCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.ident.fmt(f)?;
        f.write_str("(")?;

        let mut iter = self.arguments.iter().peekable();
        while let Some(arg) = iter.next() {
            arg.fmt(f)?;

            if iter.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str(")")
    }
}

impl fmt::Debug for FunctionCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FunctionCall(")?;
        self.ident.fmt(f)?;

        f.write_str("(")?;

        let mut iter = self.arguments.iter().peekable();
        while let Some(arg) = iter.next() {
            arg.fmt(f)?;

            if iter.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str("))")
    }
}

/// An argument passed to a function call.
///
/// The first value is an optional identifier provided for the argument, making
/// it a _keyword argument_ as opposed to a _positional argument_.
///
/// The second value is the expression provided as the argument.
#[derive(PartialEq)]
pub struct FunctionArgument {
    pub ident: Option<Node<Ident>>,
    pub expr: Node<Expr>,
}

impl fmt::Display for FunctionArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ident) = &self.ident {
            write!(f, "{}: ", ident)?;
        }

        self.expr.fmt(f)
    }
}

impl fmt::Debug for FunctionArgument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ident) = &self.ident {
            write!(f, "Argument({:?}: {:?})", ident, self.expr)
        } else {
            write!(f, "Argument({:?})", self.expr)
        }
    }
}

// -----------------------------------------------------------------------------
// unary
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub enum Unary {
    Not(Node<Not>),
}

impl fmt::Display for Unary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Unary::*;

        match self {
            Not(v) => v.fmt(f),
        }
    }
}

impl fmt::Debug for Unary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Unary::*;

        let value = match self {
            Not(v) => format!("{:?}", v),
        };

        write!(f, "Unary({})", value)
    }
}

// -----------------------------------------------------------------------------
// not
// -----------------------------------------------------------------------------

#[derive(PartialEq)]
pub struct Not(pub(crate) Node<()>, pub(crate) Box<Node<Expr>>);

impl Not {
    pub fn take(self) -> (Node<()>, Box<Node<Expr>>) {
        (self.0, self.1)
    }
}

impl fmt::Display for Not {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "!{}", self.1)
    }
}

impl fmt::Debug for Not {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Not({:?})", self.1)
    }
}

// -----------------------------------------------------------------------------
// testing utilities
// -----------------------------------------------------------------------------

macro_rules! test_enum {
    ($(($variant:tt, $func:expr, $ret:ty)),+ $(,)*) => {
        /// A "test" node used to expose individual non-root nodes for
        /// unit-testing purposes.
        ///
        /// This node should **only be used for testing** and is allowed to be
        /// changed in **backward incompatible ways**.
        #[derive(Debug, PartialEq)]
        pub enum Test {
            $($variant($ret)),+
        }

        impl Test {
            $(paste::paste! {
                /// Quickly get the relevant variant under test from the enum.
                ///
                /// This function panics if the variant does not match the
                /// expectation.
                pub fn [<$func>](self) -> $ret {
                    match self {
                        Test::$variant(v) => v,
                        v => panic!(v),
                    }
                }
            })+
        }
    };
}

test_enum![
    // root
    (Expr, expr, Node<Expr>),
    // expression
    (Literal, literal, Literal),
    (Container, container, Container),
    // arithmetic
    (Arithmetic, arithmetic, Node<Expr>),
    // atoms
    (String, string, String),
    (Integer, integer, i64),
    (Float, float, NotNan<f64>),
    (Boolean, boolean, bool),
    (Null, null, ()),
    (Regex, regex, String),
    // containers
    (Block, block, Block),
    (Array, array, Array),
    (Object, object, Object),
    // other
    (Assignment, assignment, Node<Assignment>),
    (FunctionCall, function_call, FunctionCall),
    (Query, query, Query),
];
