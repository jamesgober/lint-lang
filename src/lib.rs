//! # lint_lang
//!
//! A lint framework: named, configurable rules that run over a syntax tree and
//! report what they find as diagnostics.
//!
//! A compiler's checks fall into two groups. Some are hard errors the language
//! defines — a type mismatch, an unresolved name. Others are judgements: code
//! that is legal but almost certainly a mistake, needlessly complex, or against a
//! project's style. Those belong in a linter, where each check is a separate rule
//! the user can turn up to an error, down to a warning, or off entirely.
//! lint-lang is the machinery for that second group. It owns no rules and no
//! grammar; a language supplies both, and lint-lang runs the rules, applies the
//! user's configuration, and collects the results.
//!
//! ## The model
//!
//! - A [`Rule`] is one check. It has a stable [`name`](Rule::name), a
//!   [`default_level`](Rule::default_level), and a [`check`](Rule::check) that
//!   inspects one node and reports problems.
//! - A [`Level`] — [`Allow`](Level::Allow), [`Warn`](Level::Warn), or
//!   [`Deny`](Level::Deny) — is how strictly a rule is applied.
//! - A [`Linter`] holds the registered rules and their levels, and runs them all
//!   over a tree in a **single walk**.
//! - A [`Context`] is what a rule sees while checking a node: the arena, the
//!   node's ancestors and depth, and the [`report`](Context::report) call.
//! - A [`Finding`] is one reported problem: a [`Diagnostic`] tagged with the rule
//!   that produced it.
//!
//! Trees are the ones [`ast_lang`] defines: nodes of any type implementing
//! [`Node`], stored in an [`Arena`] and linked by [`Id`] handles. Findings are
//! [`diag_lang`] diagnostics, so they render with source context through the same
//! renderer as every other error the compiler produces.
//!
//! ## Example
//!
//! A two-variant expression type and a rule that flags adding zero.
//!
//! ```
//! use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};
//!
//! enum Expr {
//!     Int(i64, Span),
//!     Add(Id<Expr>, Id<Expr>, Span),
//! }
//!
//! impl Node for Expr {
//!     fn span(&self) -> Span {
//!         match self {
//!             Expr::Int(_, s) | Expr::Add(_, _, s) => *s,
//!         }
//!     }
//!     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
//!         if let Expr::Add(a, b, _) = self {
//!             f(*a);
//!             f(*b);
//!         }
//!     }
//!     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
//!         match self {
//!             Expr::Int(v, s) => Expr::Int(*v, *s),
//!             Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s),
//!         }
//!     }
//! }
//!
//! struct AddZero;
//! impl Rule<Expr> for AddZero {
//!     fn name(&self) -> &'static str {
//!         "add-zero"
//!     }
//!     fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
//!         let Expr::Add(a, b, span) = node else { return };
//!         let arena = cx.arena();
//!         let is_zero = |id: &Id<Expr>| matches!(arena.get(*id), Some(Expr::Int(0, _)));
//!         if is_zero(a) || is_zero(b) {
//!             cx.report("adding zero has no effect", Label::new(*span, "this addition"));
//!         }
//!     }
//! }
//!
//! // `x + 0`, with `x` = 9
//! let mut arena = Arena::new();
//! let x = arena.alloc(Expr::Int(9, Span::new(0, 1)));
//! let zero = arena.alloc(Expr::Int(0, Span::new(4, 5)));
//! let sum = arena.alloc(Expr::Add(x, zero, Span::new(0, 5)));
//!
//! let mut linter = Linter::new();
//! linter.register(AddZero)?;
//!
//! let findings = linter.run(&arena, sum);
//! assert_eq!(findings.len(), 1);
//! assert_eq!(findings[0].to_string(), "warning[add-zero]: adding zero has no effect");
//!
//! // The user decides how seriously to take it.
//! linter.set_level("add-zero", Level::Deny)?;
//! assert!(linter.run(&arena, sum)[0].is_error());
//! # Ok::<(), lint_lang::LintError>(())
//! ```
//!
//! ## Performance
//!
//! A run is one iterative depth-first walk, whatever the number of rules: each
//! node is visited once and handed to every enabled rule in turn. Disabled rules
//! are never called, a linter with nothing enabled does not walk at all, and the
//! only allocations a run makes are the walk's own stacks and the findings
//! themselves. [`Linter::run_into`] reuses a caller's buffer across runs.
//!
//! ## Features
//!
//! - `std` (default) — the standard library. Without it the crate is
//!   `#![no_std]` and needs only `alloc`. Forwards to `ast-lang/std` and
//!   `diag-lang/std`.
//! - `serde` — `Serialize` and `Deserialize` for [`Level`], as the lowercase
//!   strings `"allow"`, `"warn"`, and `"deny"`, so rule levels can be read straight
//!   from a configuration file.
//!
//! ## Stability
//!
//! The public surface is frozen and stable as of `1.0.0`: it follows Semantic
//! Versioning, with no breaking changes before `2.0`. The full surface, the
//! behavioural guarantees, and the SemVer promise are catalogued in
//! [`docs/API.md`](https://github.com/jamesgober/lint-lang/blob/main/docs/API.md#stability).

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![deny(
    warnings,
    missing_docs,
    unsafe_op_in_unsafe_fn,
    unused_must_use,
    unused_results,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable,
    clippy::dbg_macro,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::undocumented_unsafe_blocks
)]

extern crate alloc;

mod context;
mod error;
mod finding;
mod level;
mod linter;
mod rule;

// Re-exported so a downstream defining rules can name the tree and diagnostic
// types this crate's API is built on without also depending on `ast-lang` and
// `diag-lang` directly. Both crates share one `span-lang`, so a node's `Span`
// labels a diagnostic with no conversion.
pub use ast_lang::{Arena, Id, Node, Span};
pub use context::Context;
pub use diag_lang::{Diagnostic, Label, Severity};
pub use error::LintError;
pub use finding::Finding;
pub use level::Level;
pub use linter::Linter;
pub use rule::Rule;

/// Compiles and runs the `rust` code blocks in `README.md` and `docs/API.md` as
/// part of `cargo test`, so the published examples cannot drift from the API.
///
/// Present only while collecting doctests (`#[cfg(doctest)]`); it is not part of
/// the public surface and does not appear in the built library or its docs.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
#[doc = include_str!("../docs/API.md")]
pub struct MarkdownDocTests;
