//! The rule registry and the runner that drives every enabled rule over a tree.

use alloc::{boxed::Box, vec::Vec};
use core::fmt;

use ast_lang::{Arena, Flow, Id, Node, Visitor, walk};

use crate::{Context, Finding, Level, LintError, Rule};

/// A configured set of lint rules, and the runner that applies them to a tree.
///
/// Build a linter by [registering](Linter::register) rules, adjust their levels
/// with [`set_level`](Linter::set_level), then [`run`](Linter::run) it on any
/// number of trees. Each rule starts at its own
/// [default level](Rule::default_level).
///
/// # One walk, every rule
///
/// A run is a single depth-first walk of the tree. At each node the linter calls
/// every enabled rule in registration order, so adding a rule adds one call per
/// node — never another traversal. Rules at [`Level::Allow`] are skipped outright,
/// and when every rule is allowed the run returns without touching the tree.
///
/// The walk is iterative (it is [`ast_lang::walk`]), so a tree of any depth is
/// linted without risk of overflowing the call stack. A `root` handle that names
/// no node in the arena produces no findings rather than a panic.
///
/// # Sharing
///
/// [`run`](Linter::run) takes `&self` and rules are `Send + Sync`, so a linter is
/// `Send + Sync` too: configure it once and lint many files in parallel from a
/// shared reference.
///
/// # Examples
///
/// ```
/// use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Severity, Span};
///
/// enum Expr {
///     Int(i64, Span),
///     Neg(Id<Expr>, Span),
/// }
///
/// impl Node for Expr {
///     fn span(&self) -> Span {
///         match self {
///             Expr::Int(_, s) | Expr::Neg(_, s) => *s,
///         }
///     }
///     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
///         if let Expr::Neg(inner, _) = self {
///             f(*inner);
///         }
///     }
///     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
///         match self {
///             Expr::Int(v, s) => Expr::Int(*v, *s),
///             Expr::Neg(inner, s) => Expr::Neg(f(*inner), *s),
///         }
///     }
/// }
///
/// // `--x` is the same as `x`.
/// struct DoubleNegation;
/// impl Rule<Expr> for DoubleNegation {
///     fn name(&self) -> &'static str {
///         "double-negation"
///     }
///     fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
///         if let Expr::Neg(inner, span) = node {
///             if let Some(Expr::Neg(..)) = cx.arena().get(*inner) {
///                 cx.report("negation of a negation", Label::new(*span, "this cancels out"));
///             }
///         }
///     }
/// }
///
/// let mut arena = Arena::new();
/// let x = arena.alloc(Expr::Int(5, Span::new(2, 3)));
/// let neg = arena.alloc(Expr::Neg(x, Span::new(1, 3)));
/// let neg_neg = arena.alloc(Expr::Neg(neg, Span::new(0, 3)));
///
/// let mut linter = Linter::new();
/// linter.register(DoubleNegation)?;
///
/// let findings = linter.run(&arena, neg_neg);
/// assert_eq!(findings.len(), 1);
/// assert_eq!(findings[0].severity(), Severity::Warning);
///
/// // Escalate the rule to an error, or turn it off entirely.
/// linter.set_level("double-negation", Level::Deny)?;
/// assert!(linter.run(&arena, neg_neg)[0].is_error());
/// linter.set_level("double-negation", Level::Allow)?;
/// assert!(linter.run(&arena, neg_neg).is_empty());
/// # Ok::<(), lint_lang::LintError>(())
/// ```
pub struct Linter<N> {
    rules: Vec<Registered<N>>,
    /// How many rules are not at `Level::Allow`; a run with none skips the walk.
    enabled: usize,
}

/// A registered rule and the level it is currently configured at.
struct Registered<N> {
    rule: Box<dyn Rule<N>>,
    name: &'static str,
    level: Level,
}

impl<N> Linter<N> {
    /// Creates a linter with no rules.
    ///
    /// Running an empty linter returns no findings and does not walk the tree.
    ///
    /// # Examples
    ///
    /// ```
    /// use lint_lang::Linter;
    /// # use lint_lang::{Id, Node, Span};
    /// # struct Leaf;
    /// # impl Node for Leaf {
    /// #     fn span(&self) -> Span { Span::new(0, 0) }
    /// #     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
    /// #     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
    /// # }
    ///
    /// let linter: Linter<Leaf> = Linter::new();
    /// assert!(linter.is_empty());
    /// ```
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rules: Vec::new(),
            enabled: 0,
        }
    }

    /// Adds a rule, at its [default level](Rule::default_level).
    ///
    /// Rules run in the order they are registered, which is also the order their
    /// findings appear for any one node.
    ///
    /// # Errors
    ///
    /// Returns [`LintError::DuplicateRule`] if a rule with the same
    /// [name](Rule::name) is already registered. The linter is left unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// use lint_lang::{Context, Linter, LintError, Rule};
    /// # use lint_lang::{Id, Node, Span};
    /// # struct Leaf;
    /// # impl Node for Leaf {
    /// #     fn span(&self) -> Span { Span::new(0, 0) }
    /// #     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
    /// #     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
    /// # }
    ///
    /// struct Quiet;
    /// impl Rule<Leaf> for Quiet {
    ///     fn name(&self) -> &'static str { "quiet" }
    ///     fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
    /// }
    ///
    /// let mut linter = Linter::new();
    /// linter.register(Quiet)?;
    /// assert_eq!(linter.register(Quiet), Err(LintError::DuplicateRule { name: "quiet" }));
    /// assert_eq!(linter.len(), 1);
    /// # Ok::<(), LintError>(())
    /// ```
    pub fn register<R>(&mut self, rule: R) -> Result<(), LintError>
    where
        R: Rule<N> + 'static,
    {
        let name = rule.name();
        if self.position(name).is_some() {
            return Err(LintError::DuplicateRule { name });
        }
        let level = rule.default_level();
        if level != Level::Allow {
            self.enabled += 1;
        }
        self.rules.push(Registered {
            rule: Box::new(rule),
            name,
            level,
        });
        Ok(())
    }

    /// Sets the level of the rule called `name`, replacing its current level.
    ///
    /// This is how user configuration is applied: from a configuration file, a
    /// `-W`/`-D`/`-A` style flag, or an editor setting. Setting
    /// [`Level::Allow`] disables the rule so it no longer runs at all.
    ///
    /// # Errors
    ///
    /// Returns [`LintError::UnknownRule`] if no registered rule has that name. The
    /// linter is left unchanged.
    ///
    /// # Examples
    ///
    /// Applying `name = level` lines from a configuration file:
    ///
    /// ```
    /// use lint_lang::{Context, Level, Linter, LintError, Rule};
    /// # use lint_lang::{Id, Node, Span};
    /// # struct Leaf;
    /// # impl Node for Leaf {
    /// #     fn span(&self) -> Span { Span::new(0, 0) }
    /// #     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
    /// #     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
    /// # }
    /// # struct Named(&'static str);
    /// # impl Rule<Leaf> for Named {
    /// #     fn name(&self) -> &'static str { self.0 }
    /// #     fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
    /// # }
    ///
    /// let mut linter = Linter::new();
    /// linter.register(Named("unused-value"))?;
    /// linter.register(Named("shadowing"))?;
    ///
    /// let config = "unused-value = deny\nshadowing = allow";
    /// for line in config.lines() {
    ///     let (name, level) = line.split_once('=').expect("well-formed line");
    ///     linter.set_level(name.trim(), level.trim().parse()?)?;
    /// }
    ///
    /// assert_eq!(linter.level("unused-value"), Some(Level::Deny));
    /// assert_eq!(linter.level("shadowing"), Some(Level::Allow));
    /// assert!(linter.set_level("typo", Level::Warn).is_err());
    /// # Ok::<(), LintError>(())
    /// ```
    pub fn set_level(&mut self, name: &str, level: Level) -> Result<(), LintError> {
        let Some(index) = self.position(name) else {
            return Err(LintError::UnknownRule { name: name.into() });
        };
        if let Some(entry) = self.rules.get_mut(index) {
            match (entry.level == Level::Allow, level == Level::Allow) {
                (true, false) => self.enabled += 1,
                (false, true) => self.enabled -= 1,
                _ => {}
            }
            entry.level = level;
        }
        Ok(())
    }

    /// Returns the current level of the rule called `name`, or `None` if no
    /// registered rule has that name.
    ///
    /// # Examples
    ///
    /// ```
    /// use lint_lang::{Context, Level, Linter, Rule};
    /// # use lint_lang::{Id, Node, Span};
    /// # struct Leaf;
    /// # impl Node for Leaf {
    /// #     fn span(&self) -> Span { Span::new(0, 0) }
    /// #     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
    /// #     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
    /// # }
    ///
    /// struct Pedantic;
    /// impl Rule<Leaf> for Pedantic {
    ///     fn name(&self) -> &'static str { "pedantic" }
    ///     fn default_level(&self) -> Level { Level::Allow }
    ///     fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
    /// }
    ///
    /// let mut linter = Linter::new();
    /// linter.register(Pedantic)?;
    /// assert_eq!(linter.level("pedantic"), Some(Level::Allow));
    /// assert_eq!(linter.level("missing"), None);
    /// # Ok::<(), lint_lang::LintError>(())
    /// ```
    #[must_use]
    pub fn level(&self, name: &str) -> Option<Level> {
        self.position(name)
            .and_then(|index| self.rules.get(index))
            .map(|entry| entry.level)
    }

    /// Returns every registered rule's name and current level, in registration
    /// order.
    ///
    /// Useful for listing the available rules in a `--help` screen, validating a
    /// configuration file, or dumping the effective configuration.
    ///
    /// # Examples
    ///
    /// ```
    /// use lint_lang::{Context, Level, Linter, Rule};
    /// # use lint_lang::{Id, Node, Span};
    /// # struct Leaf;
    /// # impl Node for Leaf {
    /// #     fn span(&self) -> Span { Span::new(0, 0) }
    /// #     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
    /// #     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
    /// # }
    /// # struct Named(&'static str, Level);
    /// # impl Rule<Leaf> for Named {
    /// #     fn name(&self) -> &'static str { self.0 }
    /// #     fn default_level(&self) -> Level { self.1 }
    /// #     fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
    /// # }
    ///
    /// let mut linter = Linter::new();
    /// linter.register(Named("a", Level::Warn))?;
    /// linter.register(Named("b", Level::Deny))?;
    ///
    /// let listed: Vec<_> = linter.rules().collect();
    /// assert_eq!(listed, [("a", Level::Warn), ("b", Level::Deny)]);
    /// # Ok::<(), lint_lang::LintError>(())
    /// ```
    pub fn rules(&self) -> impl ExactSizeIterator<Item = (&'static str, Level)> + '_ {
        self.rules.iter().map(|entry| (entry.name, entry.level))
    }

    /// Returns the number of registered rules, enabled or not.
    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Returns `true` if no rules are registered.
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Finds the index of the rule called `name`.
    ///
    /// A linear scan: rule sets are small (tens to low hundreds), the scan runs only
    /// while configuring, and a contiguous vector keeps the run loop's rule list
    /// dense in memory.
    fn position(&self, name: &str) -> Option<usize> {
        self.rules.iter().position(|entry| entry.name == name)
    }
}

impl<N: Node> Linter<N> {
    /// Lints the tree rooted at `root` and returns every finding.
    ///
    /// Findings are ordered by node in depth-first pre-order and, within a node, by
    /// rule registration order. To reuse a buffer across many runs, use
    /// [`run_into`](Linter::run_into).
    ///
    /// # Examples
    ///
    /// ```
    /// use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
    ///
    /// struct Leaf(Span);
    /// impl Node for Leaf {
    ///     fn span(&self) -> Span { self.0 }
    ///     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
    ///     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
    /// }
    ///
    /// struct EmptySpan;
    /// impl Rule<Leaf> for EmptySpan {
    ///     fn name(&self) -> &'static str { "empty-span" }
    ///     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) {
    ///         if node.span().is_empty() {
    ///             cx.report("node covers no source", Label::unlabelled(node.span()));
    ///         }
    ///     }
    /// }
    ///
    /// let mut arena = Arena::new();
    /// let root = arena.alloc(Leaf(Span::empty(4)));
    /// let mut linter = Linter::new();
    /// linter.register(EmptySpan)?;
    ///
    /// let findings = linter.run(&arena, root);
    /// assert_eq!(findings.len(), 1);
    /// assert_eq!(findings[0].rule(), "empty-span");
    /// # Ok::<(), lint_lang::LintError>(())
    /// ```
    #[must_use]
    pub fn run(&self, arena: &Arena<N>, root: Id<N>) -> Vec<Finding> {
        let mut findings = Vec::new();
        self.run_into(arena, root, &mut findings);
        findings
    }

    /// Lints the tree rooted at `root`, appending every finding to `findings`.
    ///
    /// Existing contents of `findings` are kept; new findings are pushed after
    /// them in the same order [`run`](Linter::run) returns. Clearing and reusing
    /// one vector across a batch of files avoids reallocating it for each file.
    ///
    /// # Examples
    ///
    /// ```
    /// use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
    /// # struct Leaf(Span);
    /// # impl Node for Leaf {
    /// #     fn span(&self) -> Span { self.0 }
    /// #     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
    /// #     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
    /// # }
    /// # struct Always;
    /// # impl Rule<Leaf> for Always {
    /// #     fn name(&self) -> &'static str { "always" }
    /// #     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) {
    /// #         cx.report("hit", Label::unlabelled(node.span()));
    /// #     }
    /// # }
    ///
    /// let mut linter = Linter::new();
    /// linter.register(Always)?;
    ///
    /// // Lint a batch of single-node "files" through one reused buffer.
    /// let mut buffer = Vec::new();
    /// let mut total = 0;
    /// for len in 1..=3 {
    ///     let mut arena = Arena::new();
    ///     let root = arena.alloc(Leaf(Span::new(0, len)));
    ///     buffer.clear();
    ///     linter.run_into(&arena, root, &mut buffer);
    ///     total += buffer.len();
    /// }
    /// assert_eq!(total, 3);
    /// # Ok::<(), lint_lang::LintError>(())
    /// ```
    pub fn run_into(&self, arena: &Arena<N>, root: Id<N>, findings: &mut Vec<Finding>) {
        if self.enabled == 0 {
            return;
        }
        let mut driver = Driver {
            rules: &self.rules,
            path: Vec::new(),
            findings,
        };
        walk(arena, root, &mut driver);
    }
}

impl<N> Default for Linter<N> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<N> fmt::Debug for Linter<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Linter")
            .field("rules", &RuleList(&self.rules))
            .field("enabled", &self.enabled)
            .finish()
    }
}

/// Formats the registered rules as a `name: level` map for `Linter`'s `Debug`.
struct RuleList<'a, N>(&'a [Registered<N>]);

impl<N> fmt::Debug for RuleList<'_, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.0.iter().map(|entry| (entry.name, entry.level)))
            .finish()
    }
}

/// The visitor a run drives over the tree: it calls each enabled rule at every
/// node and keeps the path of ancestors that [`Context`] exposes.
struct Driver<'r, N> {
    rules: &'r [Registered<N>],
    /// Handles from the run's root down to the parent of the current node.
    path: Vec<Id<N>>,
    findings: &'r mut Vec<Finding>,
}

impl<N: Node> Visitor<N> for Driver<'_, N> {
    fn enter(&mut self, arena: &Arena<N>, id: Id<N>, node: &N) -> Flow {
        let mut cx = Context::new(arena, &self.path, id, self.findings);
        for entry in self.rules {
            if let Some(severity) = entry.level.severity() {
                cx.bind(entry.name, severity);
                entry.rule.check(node, &mut cx);
            }
        }
        self.path.push(id);
        Flow::Continue
    }

    fn leave(&mut self, _arena: &Arena<N>, _id: Id<N>, _node: &N) {
        // `leave` runs exactly once for every node `enter` pushed, so this pop always
        // removes the node being left; the returned handle itself is not needed.
        let _left = self.path.pop();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // test setup: a failed registration must fail the test loudly
mod tests {
    use alloc::{format, vec};

    use ast_lang::Span;
    use diag_lang::{Label, Severity};

    use super::*;

    /// A small n-ary tree: each node carries a tag and its children.
    struct T {
        tag: u8,
        kids: Vec<Id<T>>,
        span: Span,
    }

    impl Node for T {
        fn span(&self) -> Span {
            self.span
        }
        fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
            self.kids.iter().copied().for_each(f);
        }
        fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
            T {
                tag: self.tag,
                kids: self.kids.iter().map(|&k| f(k)).collect(),
                span: self.span,
            }
        }
    }

    /// Reports every node whose tag equals `self.1`.
    struct Tag(&'static str, u8, Level);
    impl Rule<T> for Tag {
        fn name(&self) -> &'static str {
            self.0
        }
        fn default_level(&self) -> Level {
            self.2
        }
        fn check(&self, node: &T, cx: &mut Context<'_, T>) {
            if node.tag == self.1 {
                cx.report("tag", Label::unlabelled(node.span));
            }
        }
    }

    /// Reports every node, recording its depth in the message length.
    struct Every;
    impl Rule<T> for Every {
        fn name(&self) -> &'static str {
            "every"
        }
        fn check(&self, node: &T, cx: &mut Context<'_, T>) {
            cx.report(format!("{}", cx.depth()), Label::unlabelled(node.span));
        }
    }

    fn leaf(arena: &mut Arena<T>, tag: u8, at: u32) -> Id<T> {
        arena.alloc(T {
            tag,
            kids: Vec::new(),
            span: Span::new(at, at + 1),
        })
    }

    /// root(0) -> [a(1) -> [c(2)], b(1)]
    fn sample() -> (Arena<T>, Id<T>) {
        let mut arena = Arena::new();
        let c = leaf(&mut arena, 2, 2);
        let a = arena.alloc(T {
            tag: 1,
            kids: vec![c],
            span: Span::new(1, 3),
        });
        let b = leaf(&mut arena, 1, 4);
        let root = arena.alloc(T {
            tag: 0,
            kids: vec![a, b],
            span: Span::new(0, 5),
        });
        (arena, root)
    }

    #[test]
    fn test_new_linter_is_empty_and_finds_nothing() {
        let (arena, root) = sample();
        let linter: Linter<T> = Linter::default();
        assert!(linter.is_empty());
        assert_eq!(linter.len(), 0);
        assert!(linter.run(&arena, root).is_empty());
    }

    #[test]
    fn test_register_duplicate_name_returns_err_and_keeps_first() {
        let mut linter = Linter::new();
        assert_eq!(linter.register(Tag("t", 1, Level::Warn)), Ok(()));
        assert_eq!(
            linter.register(Tag("t", 2, Level::Deny)),
            Err(LintError::DuplicateRule { name: "t" })
        );
        assert_eq!(linter.len(), 1);
        assert_eq!(linter.level("t"), Some(Level::Warn));
    }

    #[test]
    fn test_register_uses_default_level() {
        let mut linter = Linter::new();
        linter.register(Every).unwrap();
        assert_eq!(linter.level("every"), Some(Level::Warn));
    }

    #[test]
    fn test_set_level_unknown_rule_returns_err() {
        let mut linter: Linter<T> = Linter::new();
        assert_eq!(
            linter.set_level("ghost", Level::Deny),
            Err(LintError::UnknownRule {
                name: "ghost".into()
            })
        );
    }

    #[test]
    fn test_level_unknown_rule_is_none() {
        let linter: Linter<T> = Linter::new();
        assert_eq!(linter.level("ghost"), None);
    }

    #[test]
    fn test_run_reports_matching_nodes_with_configured_severity() {
        let (arena, root) = sample();
        let mut linter = Linter::new();
        linter.register(Tag("ones", 1, Level::Deny)).unwrap();
        let findings = linter.run(&arena, root);
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().all(|f| f.severity() == Severity::Error));
        assert!(findings.iter().all(|f| f.rule() == "ones"));
    }

    #[test]
    fn test_run_orders_by_preorder_then_registration() {
        let (arena, root) = sample();
        let mut linter = Linter::new();
        linter.register(Every).unwrap();
        linter.register(Tag("ones", 1, Level::Warn)).unwrap();
        let got: Vec<_> = linter
            .run(&arena, root)
            .iter()
            .map(|f| (f.rule(), f.diagnostic().primary().span().start().to_u32()))
            .collect();
        assert_eq!(
            got,
            [
                ("every", 0),
                ("every", 1),
                ("ones", 1),
                ("every", 2),
                ("every", 4),
                ("ones", 4),
            ]
        );
    }

    #[test]
    fn test_run_exposes_depth_through_context() {
        let (arena, root) = sample();
        let mut linter = Linter::new();
        linter.register(Every).unwrap();
        let depths: Vec<_> = linter
            .run(&arena, root)
            .iter()
            .map(|f| f.diagnostic().message().parse::<usize>().unwrap())
            .collect();
        assert_eq!(depths, [0, 1, 2, 1]);
    }

    #[test]
    fn test_allow_skips_rule_and_reenable_restores_it() {
        let (arena, root) = sample();
        let mut linter = Linter::new();
        linter.register(Every).unwrap();
        linter.set_level("every", Level::Allow).unwrap();
        assert!(linter.run(&arena, root).is_empty());
        linter.set_level("every", Level::Warn).unwrap();
        assert_eq!(linter.run(&arena, root).len(), 4);
    }

    #[test]
    fn test_enabled_count_tracks_level_changes() {
        let mut linter = Linter::new();
        linter.register(Tag("a", 0, Level::Allow)).unwrap();
        linter.register(Tag("b", 0, Level::Warn)).unwrap();
        assert_eq!(linter.enabled, 1);
        linter.set_level("a", Level::Deny).unwrap();
        assert_eq!(linter.enabled, 2);
        linter.set_level("a", Level::Warn).unwrap();
        assert_eq!(linter.enabled, 2);
        linter.set_level("b", Level::Allow).unwrap();
        linter.set_level("b", Level::Allow).unwrap();
        assert_eq!(linter.enabled, 1);
    }

    #[test]
    fn test_run_into_appends_after_existing_findings() {
        let (arena, root) = sample();
        let mut linter = Linter::new();
        linter.register(Every).unwrap();
        let mut out = linter.run(&arena, root);
        linter.run_into(&arena, root, &mut out);
        assert_eq!(out.len(), 8);
        assert_eq!(out[..4], out[4..]);
    }

    #[test]
    fn test_run_on_subtree_treats_it_as_root() {
        let mut arena = Arena::new();
        let c = leaf(&mut arena, 2, 2);
        let a = arena.alloc(T {
            tag: 1,
            kids: vec![c],
            span: Span::new(1, 3),
        });
        let mut linter = Linter::new();
        linter.register(Every).unwrap();
        let msgs: Vec<_> = linter
            .run(&arena, a)
            .iter()
            .map(|f| f.diagnostic().message().into())
            .collect::<Vec<Box<str>>>();
        assert_eq!(msgs, [Box::from("0"), Box::from("1")]);
    }

    #[test]
    fn test_run_with_stale_root_finds_nothing() {
        let (arena, _) = sample();
        let mut other = Arena::new();
        for i in 0..10 {
            let _id = leaf(&mut other, 0, i);
        }
        let foreign = leaf(&mut other, 0, 99);
        let mut linter = Linter::new();
        linter.register(Every).unwrap();
        assert!(linter.run(&arena, foreign).is_empty());
    }

    #[test]
    fn test_rules_lists_names_and_levels_in_order() {
        let mut linter = Linter::new();
        linter.register(Tag("x", 0, Level::Deny)).unwrap();
        linter.register(Every).unwrap();
        let listed: Vec<_> = linter.rules().collect();
        assert_eq!(listed, [("x", Level::Deny), ("every", Level::Warn)]);
        assert_eq!(linter.rules().len(), 2);
    }

    #[test]
    fn test_debug_lists_rules() {
        let mut linter = Linter::new();
        linter.register(Every).unwrap();
        let text = format!("{linter:?}");
        assert!(text.contains("\"every\": Warn"));
    }

    #[test]
    fn test_linter_is_send_and_sync() {
        fn assert_send_sync<S: Send + Sync>() {}
        assert_send_sync::<Linter<T>>();
    }
}
