//! One problem a rule found: the rule's name paired with its diagnostic.

use core::fmt;

use diag_lang::{Diagnostic, Severity};

/// A single problem reported by a rule during a lint run.
///
/// A finding is a [`Diagnostic`] tagged with the name of the [`Rule`](crate::Rule)
/// that produced it. The diagnostic is an ordinary `diag-lang` diagnostic — a
/// severity, a message, a labelled span, and any notes or help — so it renders
/// through [`diag_lang::Renderer`] exactly like a parse or type error. The rule
/// name is kept beside it so a driver can group findings, print the rule that
/// fired, or point the user at the setting that controls it.
///
/// [`Linter::run`](crate::Linter::run) returns findings in a fixed order: by node
/// in depth-first pre-order, and within one node by rule registration order. The
/// same tree and configuration always produce the same list.
///
/// # Examples
///
/// ```
/// use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Severity, Span};
///
/// struct Leaf(Span);
/// impl Node for Leaf {
///     fn span(&self) -> Span { self.0 }
///     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
///     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
/// }
///
/// struct Flag;
/// impl Rule<Leaf> for Flag {
///     fn name(&self) -> &'static str { "flag" }
///     fn default_level(&self) -> Level { Level::Deny }
///     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) {
///         cx.report("flagged", Label::unlabelled(node.span()));
///     }
/// }
///
/// let mut arena = Arena::new();
/// let root = arena.alloc(Leaf(Span::new(0, 4)));
/// let mut linter = Linter::new();
/// linter.register(Flag)?;
///
/// let findings = linter.run(&arena, root);
/// let finding = &findings[0];
/// assert_eq!(finding.rule(), "flag");
/// assert_eq!(finding.severity(), Severity::Error);
/// assert!(finding.is_error());
/// assert_eq!(finding.diagnostic().message(), "flagged");
/// assert_eq!(finding.to_string(), "error[flag]: flagged");
/// # Ok::<(), lint_lang::LintError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub struct Finding {
    rule: &'static str,
    diagnostic: Diagnostic,
}

impl Finding {
    /// Pairs a diagnostic with the rule that produced it.
    #[inline]
    pub(crate) const fn new(rule: &'static str, diagnostic: Diagnostic) -> Self {
        Self { rule, diagnostic }
    }

    /// Returns the name of the rule that reported this finding.
    ///
    /// This is the same name the rule returns from [`Rule::name`](crate::Rule::name)
    /// and the name [`Linter::set_level`](crate::Linter::set_level) accepts, so it
    /// tells a user exactly which setting silences or escalates the finding.
    #[inline]
    #[must_use]
    pub const fn rule(&self) -> &'static str {
        self.rule
    }

    /// Returns the diagnostic describing the problem.
    ///
    /// Hand it to [`diag_lang::Renderer`] to print it with source context.
    #[inline]
    #[must_use]
    pub const fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }

    /// Returns the diagnostic's severity.
    ///
    /// For a finding reported through [`Context::report`](crate::Context::report)
    /// this is the severity of the level the rule was configured at:
    /// [`Severity::Warning`] for [`Warn`](crate::Level::Warn) and
    /// [`Severity::Error`] for [`Deny`](crate::Level::Deny).
    #[inline]
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.diagnostic.severity()
    }

    /// Returns `true` if the finding is an error — the signal a driver uses to fail
    /// a build after linting.
    ///
    /// # Examples
    ///
    /// ```
    /// # use lint_lang::Finding;
    /// fn build_failed(findings: &[Finding]) -> bool {
    ///     findings.iter().any(Finding::is_error)
    /// }
    /// assert!(!build_failed(&[]));
    /// ```
    #[inline]
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.diagnostic.severity() == Severity::Error
    }

    /// Consumes the finding and returns its diagnostic, without copying it.
    ///
    /// Use this to move findings into a compiler's general diagnostic stream once
    /// the rule name is no longer needed.
    #[inline]
    #[must_use]
    pub fn into_diagnostic(self) -> Diagnostic {
        self.diagnostic
    }
}

impl fmt::Display for Finding {
    /// Formats a one-line summary: `severity[rule]: message`, for example
    /// `warning[double-negation]: negation of a negation`.
    ///
    /// For a full report with source context, render
    /// [`diagnostic`](Finding::diagnostic) with [`diag_lang::Renderer`] instead.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}[{}]: {}",
            self.diagnostic.severity(),
            self.rule,
            self.diagnostic.message()
        )
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use diag_lang::{Label, Span};

    use super::*;

    fn finding(severity: Severity) -> Finding {
        Finding::new(
            "no-op",
            Diagnostic::new(severity, "does nothing", Label::unlabelled(Span::new(2, 5))),
        )
    }

    #[test]
    fn test_accessors_expose_rule_and_diagnostic() {
        let f = finding(Severity::Warning);
        assert_eq!(f.rule(), "no-op");
        assert_eq!(f.severity(), Severity::Warning);
        assert_eq!(f.diagnostic().message(), "does nothing");
        assert_eq!(f.diagnostic().primary().span(), Span::new(2, 5));
    }

    #[test]
    fn test_is_error_only_for_error_severity() {
        assert!(finding(Severity::Error).is_error());
        assert!(!finding(Severity::Warning).is_error());
        assert!(!finding(Severity::Note).is_error());
    }

    #[test]
    fn test_into_diagnostic_returns_the_same_diagnostic() {
        let f = finding(Severity::Error);
        let expected = f.diagnostic().clone();
        assert_eq!(f.into_diagnostic(), expected);
    }

    #[test]
    fn test_display_is_severity_rule_message() {
        assert_eq!(
            finding(Severity::Warning).to_string(),
            "warning[no-op]: does nothing"
        );
    }
}
