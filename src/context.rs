//! What a rule can see and do while it checks one node.

use alloc::{boxed::Box, vec::Vec};
use core::fmt;

use ast_lang::{Arena, Id};
use diag_lang::{Diagnostic, Label, Severity};

use crate::Finding;

/// The view a [`Rule`](crate::Rule) gets of the node it is checking, and the
/// channel it reports findings through.
///
/// The [`Linter`](crate::Linter) builds one context per node and hands it to each
/// enabled rule in turn. Through it a rule can:
///
/// - read the tree — [`arena`](Context::arena) resolves any [`Id`], so a rule can
///   inspect a node's children or any ancestor;
/// - know where it is — [`id`](Context::id), [`parent`](Context::parent),
///   [`ancestors`](Context::ancestors), and [`depth`](Context::depth) describe the
///   node's position without the rule tracking any state of its own;
/// - report — [`report`](Context::report) records a finding at the severity the
///   rule is configured for; [`emit`](Context::emit) records a fully built
///   diagnostic.
///
/// A context only lives for the duration of one `check` call.
///
/// # Examples
///
/// A rule that flags nodes nested deeper than a limit, using only the context.
///
/// ```
/// use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
///
/// enum Tree {
///     Leaf(Span),
///     Wrap(Id<Tree>, Span),
/// }
///
/// impl Node for Tree {
///     fn span(&self) -> Span {
///         match self {
///             Tree::Leaf(s) | Tree::Wrap(_, s) => *s,
///         }
///     }
///     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
///         if let Tree::Wrap(c, _) = self {
///             f(*c);
///         }
///     }
///     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
///         match self {
///             Tree::Leaf(s) => Tree::Leaf(*s),
///             Tree::Wrap(c, s) => Tree::Wrap(f(*c), *s),
///         }
///     }
/// }
///
/// struct TooDeep(usize);
/// impl Rule<Tree> for TooDeep {
///     fn name(&self) -> &'static str { "too-deep" }
///     fn check(&self, node: &Tree, cx: &mut Context<'_, Tree>) {
///         if cx.depth() == self.0 {
///             cx.report("nested too deeply", Label::unlabelled(node.span()));
///         }
///     }
/// }
///
/// let mut arena = Arena::new();
/// let leaf = arena.alloc(Tree::Leaf(Span::new(3, 4)));
/// let inner = arena.alloc(Tree::Wrap(leaf, Span::new(2, 5)));
/// let outer = arena.alloc(Tree::Wrap(inner, Span::new(1, 6)));
/// let root = arena.alloc(Tree::Wrap(outer, Span::new(0, 7)));
///
/// let mut linter = Linter::new();
/// linter.register(TooDeep(3))?;
/// let findings = linter.run(&arena, root);
///
/// // Only the leaf sits at depth 3 (root = 0).
/// assert_eq!(findings.len(), 1);
/// assert_eq!(findings[0].diagnostic().primary().span(), Span::new(3, 4));
/// # Ok::<(), lint_lang::LintError>(())
/// ```
pub struct Context<'a, N> {
    arena: &'a Arena<N>,
    ancestors: &'a [Id<N>],
    id: Id<N>,
    rule: &'static str,
    severity: Severity,
    findings: &'a mut Vec<Finding>,
}

impl<'a, N> Context<'a, N> {
    /// Builds the context for the node `id`, whose ancestors (root first) are
    /// `ancestors`. The rule fields are filled in by [`Context::bind`] before each
    /// rule runs.
    #[inline]
    pub(crate) fn new(
        arena: &'a Arena<N>,
        ancestors: &'a [Id<N>],
        id: Id<N>,
        findings: &'a mut Vec<Finding>,
    ) -> Self {
        Self {
            arena,
            ancestors,
            id,
            rule: "",
            severity: Severity::Warning,
            findings,
        }
    }

    /// Points the context at the next rule to run, so its findings are attributed
    /// to that rule and carry its configured severity.
    #[inline]
    pub(crate) fn bind(&mut self, rule: &'static str, severity: Severity) {
        self.rule = rule;
        self.severity = severity;
    }

    /// Returns the arena the tree is stored in.
    ///
    /// Use it to look at nodes other than the one being checked — most often its
    /// children, or an ancestor from [`parent`](Context::parent) or
    /// [`ancestors`](Context::ancestors). The arena is borrowed for the whole lint
    /// run, so references obtained from it outlive the context.
    #[inline]
    #[must_use]
    pub const fn arena(&self) -> &'a Arena<N> {
        self.arena
    }

    /// Returns the handle of the node being checked.
    #[inline]
    #[must_use]
    pub const fn id(&self) -> Id<N> {
        self.id
    }

    /// Returns the handle of the node's parent, or `None` at the root of the run.
    ///
    /// "Root" means the node the run started from, which need not be the root of
    /// the whole tree: linting a subtree treats its top node as having no parent.
    #[inline]
    #[must_use]
    pub fn parent(&self) -> Option<Id<N>> {
        self.ancestors.last().copied()
    }

    /// Returns the handles of every ancestor of the node, **root first**, ending
    /// with the parent. Empty at the root of the run.
    ///
    /// The slice is the walk's own path stack, so reading it costs nothing. Iterate
    /// it in reverse (`cx.ancestors().iter().rev()`) to search outward from the
    /// node, for example to find the nearest enclosing function.
    ///
    /// # Examples
    ///
    /// ```
    /// # use lint_lang::{Arena, Context, Id, Linter, Node, Rule, Span, Label};
    /// # enum T { Leaf(Span), Wrap(Id<T>, Span) }
    /// # impl Node for T {
    /// #     fn span(&self) -> Span { match self { T::Leaf(s) | T::Wrap(_, s) => *s } }
    /// #     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) { if let T::Wrap(c, _) = self { f(*c) } }
    /// #     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
    /// #         match self { T::Leaf(s) => T::Leaf(*s), T::Wrap(c, s) => T::Wrap(f(*c), *s) }
    /// #     }
    /// # }
    /// // Flag every leaf whose nearest `Wrap` ancestor spans more than 3 bytes.
    /// struct WideWrap;
    /// impl Rule<T> for WideWrap {
    ///     fn name(&self) -> &'static str { "wide-wrap" }
    ///     fn check(&self, node: &T, cx: &mut Context<'_, T>) {
    ///         if !matches!(node, T::Leaf(_)) { return; }
    ///         let nearest = cx.ancestors().iter().rev().find_map(|&id| match cx.arena().get(id) {
    ///             Some(T::Wrap(_, s)) => Some(*s),
    ///             _ => None,
    ///         });
    ///         if nearest.is_some_and(|s| s.len() > 3) {
    ///             cx.report("leaf inside a wide wrapper", Label::unlabelled(node.span()));
    ///         }
    ///     }
    /// }
    ///
    /// let mut arena = Arena::new();
    /// let leaf = arena.alloc(T::Leaf(Span::new(1, 2)));
    /// let wrap = arena.alloc(T::Wrap(leaf, Span::new(0, 9)));
    /// let mut linter = Linter::new();
    /// linter.register(WideWrap)?;
    /// assert_eq!(linter.run(&arena, wrap).len(), 1);
    /// # Ok::<(), lint_lang::LintError>(())
    /// ```
    #[inline]
    #[must_use]
    pub const fn ancestors(&self) -> &'a [Id<N>] {
        self.ancestors
    }

    /// Returns how many ancestors the node has: `0` at the root of the run, `1`
    /// for its children, and so on.
    #[inline]
    #[must_use]
    pub const fn depth(&self) -> usize {
        self.ancestors.len()
    }

    /// Returns the name of the rule currently running.
    #[inline]
    #[must_use]
    pub const fn rule(&self) -> &'static str {
        self.rule
    }

    /// Returns the severity the running rule is configured for:
    /// [`Severity::Warning`] at [`Level::Warn`](crate::Level::Warn), or
    /// [`Severity::Error`] at [`Level::Deny`](crate::Level::Deny).
    ///
    /// [`report`](Context::report) applies it automatically. Read it directly when
    /// building a diagnostic for [`emit`](Context::emit), so the user's
    /// configuration is still honoured.
    #[inline]
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.severity
    }

    /// Reports a problem: a diagnostic headed by `message`, pointing at `label`, at
    /// the rule's configured severity.
    ///
    /// This is the call almost every rule needs. Point the label at the node with
    /// `Label::new(node.span(), "...")`, or at a narrower span inside it. For notes,
    /// help text, or secondary labels, build the diagnostic yourself and use
    /// [`emit`](Context::emit).
    ///
    /// # Examples
    ///
    /// ```
    /// # use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Severity, Span};
    /// # struct Leaf(Span);
    /// # impl Node for Leaf {
    /// #     fn span(&self) -> Span { self.0 }
    /// #     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
    /// #     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
    /// # }
    /// struct Everything;
    /// impl Rule<Leaf> for Everything {
    ///     fn name(&self) -> &'static str { "everything" }
    ///     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) {
    ///         cx.report("found a leaf", Label::new(node.span(), "here"));
    ///     }
    /// }
    ///
    /// let mut arena = Arena::new();
    /// let root = arena.alloc(Leaf(Span::new(0, 3)));
    /// let mut linter = Linter::new();
    /// linter.register(Everything)?;
    /// let findings = linter.run(&arena, root);
    /// assert_eq!(findings[0].severity(), Severity::Warning); // the default level is `warn`
    /// assert_eq!(findings[0].diagnostic().primary().message(), "here");
    /// # Ok::<(), lint_lang::LintError>(())
    /// ```
    #[inline]
    pub fn report(&mut self, message: impl Into<Box<str>>, label: Label) {
        let diagnostic = Diagnostic::new(self.severity, message, label);
        self.findings.push(Finding::new(self.rule, diagnostic));
    }

    /// Records a diagnostic built by the rule, attributed to the running rule.
    ///
    /// Use this when a finding needs more than one label, a note, or a help
    /// suggestion. The diagnostic is recorded exactly as given, so build it with
    /// [`severity`](Context::severity) to respect the configured level.
    ///
    /// # Examples
    ///
    /// ```
    /// # use lint_lang::{Arena, Context, Diagnostic, Id, Label, Linter, Node, Rule, Span};
    /// # struct Leaf(Span);
    /// # impl Node for Leaf {
    /// #     fn span(&self) -> Span { self.0 }
    /// #     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
    /// #     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
    /// # }
    /// struct WithHelp;
    /// impl Rule<Leaf> for WithHelp {
    ///     fn name(&self) -> &'static str { "with-help" }
    ///     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) {
    ///         let diag = Diagnostic::new(cx.severity(), "leaf is redundant", Label::unlabelled(node.span()))
    ///             .with_help("remove it");
    ///         cx.emit(diag);
    ///     }
    /// }
    ///
    /// let mut arena = Arena::new();
    /// let root = arena.alloc(Leaf(Span::new(0, 3)));
    /// let mut linter = Linter::new();
    /// linter.register(WithHelp)?;
    /// let findings = linter.run(&arena, root);
    /// assert_eq!(findings[0].diagnostic().help().next(), Some("remove it"));
    /// # Ok::<(), lint_lang::LintError>(())
    /// ```
    #[inline]
    pub fn emit(&mut self, diagnostic: Diagnostic) {
        self.findings.push(Finding::new(self.rule, diagnostic));
    }
}

impl<N> fmt::Debug for Context<'_, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Context")
            .field("id", &self.id)
            .field("depth", &self.depth())
            .field("rule", &self.rule)
            .field("severity", &self.severity)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use alloc::format;

    use ast_lang::{Node, Span};

    use super::*;

    struct Leaf(Span);
    impl Node for Leaf {
        fn span(&self) -> Span {
            self.0
        }
        fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
        fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
            Leaf(self.0)
        }
    }

    fn setup() -> (Arena<Leaf>, Id<Leaf>, Id<Leaf>, Id<Leaf>) {
        let mut arena = Arena::new();
        let a = arena.alloc(Leaf(Span::new(0, 1)));
        let b = arena.alloc(Leaf(Span::new(1, 2)));
        let c = arena.alloc(Leaf(Span::new(2, 3)));
        (arena, a, b, c)
    }

    #[test]
    fn test_position_at_root_has_no_parent() {
        let (arena, a, _, _) = setup();
        let mut out = Vec::new();
        let cx = Context::new(&arena, &[], a, &mut out);
        assert_eq!(cx.id(), a);
        assert_eq!(cx.parent(), None);
        assert!(cx.ancestors().is_empty());
        assert_eq!(cx.depth(), 0);
    }

    #[test]
    fn test_position_nested_reports_parent_and_depth() {
        let (arena, a, b, c) = setup();
        let path = [a, b];
        let mut out = Vec::new();
        let cx = Context::new(&arena, &path, c, &mut out);
        assert_eq!(cx.parent(), Some(b));
        assert_eq!(cx.ancestors(), &[a, b]);
        assert_eq!(cx.depth(), 2);
        assert!(cx.arena().get(b).is_some());
    }

    #[test]
    fn test_report_uses_bound_rule_and_severity() {
        let (arena, a, _, _) = setup();
        let mut out = Vec::new();
        let mut cx = Context::new(&arena, &[], a, &mut out);
        cx.bind("strict", Severity::Error);
        assert_eq!(cx.rule(), "strict");
        assert_eq!(cx.severity(), Severity::Error);
        cx.report("bad", Label::unlabelled(Span::new(0, 1)));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule(), "strict");
        assert_eq!(out[0].severity(), Severity::Error);
    }

    #[test]
    fn test_emit_records_diagnostic_verbatim() {
        let (arena, a, _, _) = setup();
        let mut out = Vec::new();
        let mut cx = Context::new(&arena, &[], a, &mut out);
        cx.bind("soft", Severity::Warning);
        let diag = Diagnostic::new(Severity::Note, "fyi", Label::unlabelled(Span::new(0, 1)));
        cx.emit(diag.clone());
        assert_eq!(out[0].rule(), "soft");
        assert_eq!(out[0].diagnostic(), &diag);
    }

    #[test]
    fn test_debug_shows_position_and_rule() {
        let (arena, a, _, _) = setup();
        let mut out = Vec::new();
        let mut cx = Context::new(&arena, &[], a, &mut out);
        cx.bind("dbg", Severity::Warning);
        let text = format!("{cx:?}");
        assert!(text.contains("depth: 0"));
        assert!(text.contains("\"dbg\""));
    }
}
