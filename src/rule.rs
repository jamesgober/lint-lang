//! The trait every lint rule implements.

use crate::{Context, Level};

/// One lint check: a named rule that inspects tree nodes and reports problems.
///
/// A rule is the unit a linter is built from. It has a stable
/// [`name`](Rule::name) (the handle a user configures it by), a
/// [`default_level`](Rule::default_level), and a [`check`](Rule::check) that the
/// [`Linter`](crate::Linter) calls once for every node of the tree being linted.
/// `check` looks at the node — and, through the [`Context`], at its children,
/// ancestors, and depth — and calls [`Context::report`] for each problem it finds.
///
/// `N` is the language's node type, the same type stored in the
/// [`Arena`](crate::Arena) and implementing [`Node`](crate::Node). A rule usually
/// matches on the variants it cares about and returns immediately for the rest;
/// that early return is the common case, so keep it cheap.
///
/// # Rules are stateless
///
/// `check` takes `&self`. A rule's fields hold configuration — a threshold, a
/// list of banned names — not per-run state, and everything it needs about the
/// node's position comes from the [`Context`]. That is what lets one configured
/// [`Linter`](crate::Linter) be run on many trees, including from several threads
/// at once, which is why the trait requires `Send + Sync`.
///
/// # Examples
///
/// A rule that flags integer literals above a configurable limit.
///
/// ```
/// use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};
///
/// enum Expr {
///     Int(u64, Span),
///     Add(Id<Expr>, Id<Expr>, Span),
/// }
///
/// impl Node for Expr {
///     fn span(&self) -> Span {
///         match self {
///             Expr::Int(_, s) | Expr::Add(_, _, s) => *s,
///         }
///     }
///     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
///         if let Expr::Add(a, b, _) = self {
///             f(*a);
///             f(*b);
///         }
///     }
///     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
///         match self {
///             Expr::Int(v, s) => Expr::Int(*v, *s),
///             Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s),
///         }
///     }
/// }
///
/// struct LargeLiteral {
///     limit: u64,
/// }
///
/// impl Rule<Expr> for LargeLiteral {
///     fn name(&self) -> &'static str {
///         "large-literal"
///     }
///
///     fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
///         if let Expr::Int(value, span) = node {
///             if *value > self.limit {
///                 cx.report(
///                     "integer literal exceeds the configured limit",
///                     Label::new(*span, "this literal"),
///                 );
///             }
///         }
///     }
/// }
///
/// let mut arena = Arena::new();
/// let small = arena.alloc(Expr::Int(7, Span::new(0, 1)));
/// let big = arena.alloc(Expr::Int(70_000, Span::new(4, 10)));
/// let sum = arena.alloc(Expr::Add(small, big, Span::new(0, 10)));
///
/// let mut linter = Linter::new();
/// linter.register(LargeLiteral { limit: 1_000 })?;
///
/// let findings = linter.run(&arena, sum);
/// assert_eq!(findings.len(), 1);
/// assert_eq!(findings[0].diagnostic().primary().span(), Span::new(4, 10));
/// # Ok::<(), lint_lang::LintError>(())
/// ```
pub trait Rule<N>: Send + Sync {
    /// Returns the rule's unique, stable name, such as `"double-negation"`.
    ///
    /// The name identifies the rule everywhere a user sees or configures it: in
    /// [`Linter::set_level`](crate::Linter::set_level), in configuration files, and
    /// on every [`Finding`](crate::Finding) the rule reports. Two rules registered
    /// in the same linter must not share a name. Prefer lowercase words joined by
    /// hyphens, and treat a published name as part of your compatibility surface.
    fn name(&self) -> &'static str;

    /// Returns the level the rule runs at until the user configures it otherwise.
    ///
    /// The default is [`Level::Warn`]. Return [`Level::Deny`] for rules that catch
    /// real bugs, and [`Level::Allow`] for opinionated or expensive rules a user
    /// should opt into.
    fn default_level(&self) -> Level {
        Level::Warn
    }

    /// Inspects one node and reports any problems through `cx`.
    ///
    /// Called once per node, in depth-first pre-order, for every rule that is not
    /// at [`Level::Allow`]. `node` is the node being visited; [`Context::id`] is its
    /// handle, and [`Context::arena`] reaches its children. A rule that finds
    /// nothing simply returns.
    fn check(&self, node: &N, cx: &mut Context<'_, N>);
}
