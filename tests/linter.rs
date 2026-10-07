//! End-to-end behaviour: parse real source with the example language, lint it,
//! and check the findings, their order, their levels, and how they render.

#![allow(clippy::unwrap_used)] // test setup: a failed parse or registration must fail loudly

#[path = "../examples/common/mod.rs"]
mod common;

use diag_lang::{Renderer, SourceMap};
use lint_lang::{Level, LintError, Linter, Severity, Span};

fn lint(src: &str, linter: &Linter<common::Expr>) -> Vec<(String, Severity, Span)> {
    let program = common::parse(src).unwrap();
    linter
        .run(&program.arena, program.root)
        .into_iter()
        .map(|f| {
            (
                f.rule().to_owned(),
                f.severity(),
                f.diagnostic().primary().span(),
            )
        })
        .collect()
}

#[test]
fn test_clean_source_produces_no_findings() {
    let linter = common::linter().unwrap();
    assert!(lint("a * (b + 2) - c / 4", &linter).is_empty());
}

#[test]
fn test_default_levels_apply_per_rule() {
    let linter = common::linter().unwrap();
    let got = lint("x / 0 + --y", &linter);
    assert_eq!(
        got,
        [
            (
                "division-by-zero".to_owned(),
                Severity::Error,
                Span::new(4, 5)
            ),
            (
                "double-negation".to_owned(),
                Severity::Warning,
                Span::new(8, 11)
            ),
        ]
    );
}

#[test]
fn test_allowed_by_default_rule_is_silent_until_enabled() {
    let mut linter = common::linter().unwrap();
    assert!(lint("x * 1", &linter).is_empty());
    linter.set_level("identity-op", Level::Warn).unwrap();
    let got = lint("x * 1", &linter);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "identity-op");
}

#[test]
fn test_deny_escalates_and_allow_silences() {
    let mut linter = common::linter().unwrap();
    linter.set_level("double-negation", Level::Deny).unwrap();
    assert_eq!(lint("--z", &linter)[0].1, Severity::Error);
    linter.set_level("double-negation", Level::Allow).unwrap();
    assert!(lint("--z", &linter).is_empty());
}

#[test]
fn test_everything_allowed_finds_nothing() {
    let mut linter = common::linter().unwrap();
    let names: Vec<_> = linter.rules().map(|(name, _)| name).collect();
    for name in names {
        linter.set_level(name, Level::Allow).unwrap();
    }
    assert!(lint("x / 0 + --y * 1", &linter).is_empty());
}

#[test]
fn test_findings_follow_source_preorder() {
    let mut linter = common::linter().unwrap();
    linter.set_level("identity-op", Level::Warn).unwrap();
    let rules: Vec<_> = lint("--a + b / 0 * 1", &linter)
        .into_iter()
        .map(|(rule, _, _)| rule)
        .collect();
    // Root `+` first, then its left subtree (`--a`), then its right (`b / 0 * 1`):
    // the `*` node is entered before the `/` node beneath it.
    assert_eq!(
        rules,
        ["double-negation", "identity-op", "division-by-zero"]
    );
}

#[test]
fn test_triple_negation_reports_each_nested_pair() {
    let linter = common::linter().unwrap();
    let spans: Vec<_> = lint("---q", &linter).into_iter().map(|f| f.2).collect();
    assert_eq!(spans, [Span::new(0, 4), Span::new(1, 4)]);
}

#[test]
fn test_deep_nesting_reports_once_per_path() {
    let linter = common::linter().unwrap();
    // Additions nested five deep: the innermost `5 + 6` sits at depth 4, so each of
    // its operands (depth 5) crosses the limit of 4 — one report per path, not one
    // per node below the limit.
    let got = lint("1 + (2 + (3 + (4 + (5 + 6))))", &linter);
    let deep: Vec<_> = got.iter().filter(|f| f.0 == "deep-nesting").collect();
    assert_eq!(deep.len(), 2); // the two operands of the innermost `5 + 6`
}

#[test]
fn test_very_deep_tree_does_not_overflow_the_stack() {
    let mut linter = common::linter().unwrap();
    linter.set_level("deep-nesting", Level::Allow).unwrap();
    // Built without the recursive-descent parser so the depth is not limited by it.
    let mut arena = lint_lang::Arena::new();
    let mut node = arena.alloc(common::Expr::Int(1, Span::new(0, 1)));
    for _ in 0..200_000 {
        node = arena.alloc(common::Expr::Neg(node, Span::new(0, 1)));
    }
    let findings = linter.run(&arena, node);
    assert_eq!(findings.len(), 199_999);
}

#[test]
fn test_multi_label_diagnostic_renders_with_source() {
    let linter = common::linter().unwrap();
    let src = "total / 0";
    let program = common::parse(src).unwrap();
    let findings = linter.run(&program.arena, program.root);
    let mut map = SourceMap::new();
    let _id = map.add("t.expr", src).unwrap();
    let text = Renderer::new().render(findings[0].diagnostic(), &map);
    assert!(text.contains("error: this division always fails"), "{text}");
    assert!(text.contains("^ divisor is zero"), "{text}");
    assert!(text.contains("in this expression"), "{text}");
    assert!(text.contains("note: dividing by zero"), "{text}");
}

#[test]
fn test_configuration_errors_leave_linter_unchanged() {
    let mut linter = common::linter().unwrap();
    let before: Vec<_> = linter.rules().collect();
    assert!(matches!(
        linter.set_level("no-such-rule", Level::Deny),
        Err(LintError::UnknownRule { .. })
    ));
    assert_eq!(
        linter.register(common::DoubleNegation),
        Err(LintError::DuplicateRule {
            name: "double-negation"
        })
    );
    assert_eq!(linter.rules().collect::<Vec<_>>(), before);
}

#[test]
fn test_shared_linter_across_threads_matches_serial_run() {
    let linter = common::linter().unwrap();
    let sources = ["x / 0", "--y", "a + b", "1 / 0 + --2"];
    let serial: Vec<_> = sources.iter().map(|s| lint(s, &linter)).collect();
    let parallel: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = sources
            .iter()
            .map(|s| scope.spawn(|| lint(s, &linter)))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(serial, parallel);
}
