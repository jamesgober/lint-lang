//! A miniature arithmetic language shared by the examples, the integration tests,
//! and the benchmarks: an expression tree, a parser that records byte spans, and
//! four lint rules written against lint-lang's public API.
//!
//! The grammar is integers, identifiers, `+ - * /`, unary `-`, and parentheses:
//!
//! ```text
//! expr  := term (('+' | '-') term)*
//! term  := unary (('*' | '/') unary)*
//! unary := '-' unary | atom
//! atom  := INT | IDENT | '(' expr ')'
//! ```

// Each example uses a different slice of this module; items one example does not
// touch would otherwise trip `dead_code` under `-D warnings`.
#![allow(dead_code)]

use lint_lang::{
    Arena, Context, Diagnostic, Id, Label, Level, LintError, Linter, Node, Rule, Span,
};

/// A binary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl BinOp {
    pub fn symbol(self) -> char {
        match self {
            BinOp::Add => '+',
            BinOp::Sub => '-',
            BinOp::Mul => '*',
            BinOp::Div => '/',
        }
    }
}

/// One node of the expression tree. Children are arena handles.
#[derive(Debug)]
pub enum Expr {
    Int(i64, Span),
    Var(Box<str>, Span),
    Neg(Id<Expr>, Span),
    Binary(BinOp, Id<Expr>, Id<Expr>, Span),
}

impl Node for Expr {
    fn span(&self) -> Span {
        match self {
            Expr::Int(_, s) | Expr::Var(_, s) | Expr::Neg(_, s) | Expr::Binary(_, _, _, s) => *s,
        }
    }

    fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
        match self {
            Expr::Int(..) | Expr::Var(..) => {}
            Expr::Neg(inner, _) => f(*inner),
            Expr::Binary(_, lhs, rhs, _) => {
                f(*lhs);
                f(*rhs);
            }
        }
    }

    fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
        match self {
            Expr::Int(v, s) => Expr::Int(*v, *s),
            Expr::Var(name, s) => Expr::Var(name.clone(), *s),
            Expr::Neg(inner, s) => Expr::Neg(f(*inner), *s),
            Expr::Binary(op, lhs, rhs, s) => Expr::Binary(*op, f(*lhs), f(*rhs), *s),
        }
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// A parsed program: the arena holding every node, and the root expression.
pub struct Program {
    pub arena: Arena<Expr>,
    pub root: Id<Expr>,
}

/// Why a source failed to parse, and where.
#[derive(Debug)]
pub struct ParseError {
    pub message: &'static str,
    pub span: Span,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message,
            self.span.start().to_u32(),
            self.span.end().to_u32()
        )
    }
}

impl std::error::Error for ParseError {}

/// Parses `src` into an expression tree whose spans are byte offsets into `src`.
pub fn parse(src: &str) -> Result<Program, ParseError> {
    if u32::try_from(src.len()).is_err() {
        return Err(ParseError {
            message: "source exceeds 4 GiB",
            span: Span::empty(0),
        });
    }
    let mut parser = Parser {
        src: src.as_bytes(),
        pos: 0,
        arena: Arena::new(),
    };
    let root = parser.expr()?;
    parser.skip_ws();
    if parser.pos < parser.src.len() {
        return Err(parser.error("unexpected input after expression"));
    }
    Ok(Program {
        arena: parser.arena,
        root,
    })
}

struct Parser<'s> {
    src: &'s [u8],
    pos: usize,
    arena: Arena<Expr>,
}

impl Parser<'_> {
    fn expr(&mut self) -> Result<Id<Expr>, ParseError> {
        let mut lhs = self.term()?;
        while let Some(op) = self.binary_op(b"+-") {
            let rhs = self.term()?;
            lhs = self.binary(op, lhs, rhs);
        }
        Ok(lhs)
    }

    fn term(&mut self) -> Result<Id<Expr>, ParseError> {
        let mut lhs = self.unary()?;
        while let Some(op) = self.binary_op(b"*/") {
            let rhs = self.unary()?;
            lhs = self.binary(op, lhs, rhs);
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Id<Expr>, ParseError> {
        self.skip_ws();
        if self.peek() == Some(b'-') {
            let start = self.offset();
            self.pos += 1;
            let inner = self.unary()?;
            let span = Span::new(start, self.span_of(inner).end().to_u32());
            return Ok(self.arena.alloc(Expr::Neg(inner, span)));
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<Id<Expr>, ParseError> {
        self.skip_ws();
        let start = self.pos;
        match self.peek() {
            Some(b'0'..=b'9') => {
                let mut value: i64 = 0;
                while let Some(digit @ b'0'..=b'9') = self.peek() {
                    value = value
                        .checked_mul(10)
                        .and_then(|v| v.checked_add(i64::from(digit - b'0')))
                        .ok_or_else(|| self.error("integer literal is too large"))?;
                    self.pos += 1;
                }
                let span = self.span_from(start);
                Ok(self.arena.alloc(Expr::Int(value, span)))
            }
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_') => {
                while matches!(
                    self.peek(),
                    Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
                ) {
                    self.pos += 1;
                }
                let name = String::from_utf8_lossy(&self.src[start..self.pos]).into();
                let span = self.span_from(start);
                Ok(self.arena.alloc(Expr::Var(name, span)))
            }
            Some(b'(') => {
                self.pos += 1;
                let inner = self.expr()?;
                self.skip_ws();
                if self.peek() != Some(b')') {
                    return Err(self.error("expected `)`"));
                }
                self.pos += 1;
                Ok(inner)
            }
            _ => Err(self.error("expected a number, a name, or `(`")),
        }
    }

    fn binary_op(&mut self, accepted: &[u8]) -> Option<BinOp> {
        self.skip_ws();
        let byte = self.peek().filter(|b| accepted.contains(b))?;
        self.pos += 1;
        Some(match byte {
            b'+' => BinOp::Add,
            b'-' => BinOp::Sub,
            b'*' => BinOp::Mul,
            _ => BinOp::Div,
        })
    }

    fn binary(&mut self, op: BinOp, lhs: Id<Expr>, rhs: Id<Expr>) -> Id<Expr> {
        let span = self.span_of(lhs).merge(self.span_of(rhs));
        self.arena.alloc(Expr::Binary(op, lhs, rhs, span))
    }

    fn span_of(&self, id: Id<Expr>) -> Span {
        self.arena.get(id).map_or(Span::empty(0), Node::span)
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    // `parse` rejects sources longer than `u32::MAX`, so every offset fits.
    fn offset(&self) -> u32 {
        self.pos as u32
    }

    fn span_from(&self, start: usize) -> Span {
        Span::new(start as u32, self.offset())
    }

    fn error(&self, message: &'static str) -> ParseError {
        let end = (self.pos + 1).min(self.src.len()) as u32;
        ParseError {
            message,
            span: Span::new(self.offset().min(end), end),
        }
    }
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

fn int_at(arena: &Arena<Expr>, id: Id<Expr>) -> Option<i64> {
    match arena.get(id) {
        Some(Expr::Int(value, _)) => Some(*value),
        _ => None,
    }
}

/// `x / 0` with a literal zero divisor: always a runtime fault, so an error by
/// default.
pub struct DivisionByZero;

impl Rule<Expr> for DivisionByZero {
    fn name(&self) -> &'static str {
        "division-by-zero"
    }

    fn default_level(&self) -> Level {
        Level::Deny
    }

    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        let Expr::Binary(BinOp::Div, _, rhs, span) = node else {
            return;
        };
        if int_at(cx.arena(), *rhs) != Some(0) {
            return;
        }
        let divisor = cx.arena().get(*rhs).map_or(*span, Node::span);
        let diag = Diagnostic::new(
            cx.severity(),
            "this division always fails",
            Label::new(divisor, "divisor is zero"),
        )
        .with_secondary(Label::new(*span, "in this expression"))
        .with_note("dividing by zero is a runtime error");
        cx.emit(diag);
    }
}

/// `--x`: two negations cancel out.
pub struct DoubleNegation;

impl Rule<Expr> for DoubleNegation {
    fn name(&self) -> &'static str {
        "double-negation"
    }

    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        if let Expr::Neg(inner, span) = node {
            if let Some(Expr::Neg(..)) = cx.arena().get(*inner) {
                cx.report(
                    "negation of a negation",
                    Label::new(*span, "these cancel out"),
                );
            }
        }
    }
}

/// `x + 0`, `x * 1`, and friends: arithmetic that does nothing. Opinionated, so it
/// is off unless the user enables it.
pub struct IdentityOp;

impl Rule<Expr> for IdentityOp {
    fn name(&self) -> &'static str {
        "identity-op"
    }

    fn default_level(&self) -> Level {
        Level::Allow
    }

    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        let Expr::Binary(op, lhs, rhs, span) = node else {
            return;
        };
        let (l, r) = (int_at(cx.arena(), *lhs), int_at(cx.arena(), *rhs));
        let identity = match op {
            BinOp::Add => l == Some(0) || r == Some(0),
            BinOp::Sub => r == Some(0),
            BinOp::Mul => l == Some(1) || r == Some(1),
            BinOp::Div => r == Some(1),
        };
        if identity {
            cx.report(
                "this operation has no effect",
                Label::new(*span, "simplifies to the other operand"),
            );
        }
    }
}

/// Expressions nested deeper than `limit`. Reports once, at the first node that
/// crosses the limit on each path, rather than at every node below it.
pub struct DeepNesting {
    pub limit: usize,
}

impl Rule<Expr> for DeepNesting {
    fn name(&self) -> &'static str {
        "deep-nesting"
    }

    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        if cx.depth() == self.limit + 1 {
            cx.report(
                "expression is nested too deeply",
                Label::new(node.span(), "consider naming part of this"),
            );
        }
    }
}

/// A linter with every rule in this module registered at its default level.
pub fn linter() -> Result<Linter<Expr>, LintError> {
    let mut linter = Linter::new();
    linter.register(DivisionByZero)?;
    linter.register(DoubleNegation)?;
    linter.register(IdentityOp)?;
    linter.register(DeepNesting { limit: 4 })?;
    Ok(linter)
}
