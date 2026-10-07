<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br><b>lint-lang</b><br>
    <sub><sup>API REFERENCE</sup></sub>
</h1>
<div align="center">
    <sup>
        <a href="../README.md" title="Project Home"><b>HOME</b></a>
        <span>&nbsp;│&nbsp;</span>
        <span>API</span>
        <span>&nbsp;│&nbsp;</span>
        <a href="../CHANGELOG.md" title="Changelog"><b>CHANGELOG</b></a>
    </sup>
</div>
<br>

> Complete reference for every public item in `lint-lang` **0.2**, with examples.
> **Status: pre-1.0.** The surface is designed across the 0.x series and frozen at
> `1.0.0`; see [`../dev/ROADMAP.md`](../dev/ROADMAP.md).

<h4 id="example-pointers">Example Pointers</h4>

- Quick Start: `examples/quickstart.rs` — parse, lint, and render findings with source context.
- Configure: `examples/configure.rs` — defaults, a project config file, and `-D`/`-W`/`-A` flags layered in order; configuration errors reported.
- Parallel: `examples/parallel.rs` — one shared `Linter` across every core, a reused buffer per worker.
- Shared language: `examples/common/mod.rs` — the miniature arithmetic language, its parser, and four rules (`division-by-zero`, `double-negation`, `identity-op`, `deep-nesting`).

<br>

## Table of Contents

- **[Installation](#installation)**
- **[Quick Start](#quick-start)**
- **[The Model](#the-model)**
- **[Public APIs](#public-apis)**
  - [`Rule`](#rule)
    - [`Rule::name`](#rulename)
    - [`Rule::default_level`](#ruledefault_level)
    - [`Rule::check`](#rulecheck)
  - [`Linter`](#linter)
    - [`Linter::new`](#linternew)
    - [`Linter::register`](#linterregister)
    - [`Linter::set_level`](#linterset_level)
    - [`Linter::level`](#linterlevel)
    - [`Linter::rules`](#linterrules)
    - [`Linter::len` / `Linter::is_empty`](#linterlen--linteris_empty)
    - [`Linter::run`](#linterrun)
    - [`Linter::run_into`](#linterrun_into)
  - [`Context`](#context)
    - [`Context::arena`](#contextarena)
    - [`Context::id`](#contextid)
    - [`Context::parent`](#contextparent)
    - [`Context::ancestors`](#contextancestors)
    - [`Context::depth`](#contextdepth)
    - [`Context::rule`](#contextrule)
    - [`Context::severity`](#contextseverity)
    - [`Context::report`](#contextreport)
    - [`Context::emit`](#contextemit)
  - [`Level`](#level)
    - [`Level::severity`](#levelseverity)
    - [`Level::as_str`](#levelas_str)
    - [Parsing and formatting](#parsing-and-formatting)
  - [`Finding`](#finding)
    - [`Finding::rule`](#findingrule)
    - [`Finding::diagnostic`](#findingdiagnostic)
    - [`Finding::severity`](#findingseverity)
    - [`Finding::is_error`](#findingis_error)
    - [`Finding::into_diagnostic`](#findinginto_diagnostic)
    - [`Display` for `Finding`](#display-for-finding)
  - [`LintError`](#linterror)
  - [Re-exports](#re-exports)
- **[Feature Flags](#feature-flags)**
- **[Patterns](#patterns)**
  - [Failing a build on `deny` findings](#failing-a-build-on-deny-findings)
  - [Loading levels from a configuration file](#loading-levels-from-a-configuration-file)
  - [Searching outward through ancestors](#searching-outward-through-ancestors)
  - [Configurable rules](#configurable-rules)
  - [Linting many files in parallel](#linting-many-files-in-parallel)
- **[Performance Notes](#performance-notes)**
- **[Guarantees](#guarantees)**

<br>

---

## Installation

```toml
[dependencies]
lint-lang = "0.2"
```

Or from the terminal:

```bash
cargo add lint-lang
```

MSRV: Rust 1.85 (Rust 2024 edition). With the `serde` feature:

```toml
[dependencies]
lint-lang = { version = "0.2", features = ["serde"] }
```

---

## Quick Start

Define a node type, write a rule, register it, run it. The user decides the
rule's level.

```rust
use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};

enum Expr {
    Int(i64, Span),
    Add(Id<Expr>, Id<Expr>, Span),
}

impl Node for Expr {
    fn span(&self) -> Span {
        match self {
            Expr::Int(_, s) | Expr::Add(_, _, s) => *s,
        }
    }
    fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
        if let Expr::Add(a, b, _) = self {
            f(*a);
            f(*b);
        }
    }
    fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
        match self {
            Expr::Int(v, s) => Expr::Int(*v, *s),
            Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s),
        }
    }
}

/// Flags `x + 0` and `0 + x`.
struct AddZero;
impl Rule<Expr> for AddZero {
    fn name(&self) -> &'static str {
        "add-zero"
    }
    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        let Expr::Add(a, b, span) = node else { return };
        let arena = cx.arena();
        let zero = |id: &Id<Expr>| matches!(arena.get(*id), Some(Expr::Int(0, _)));
        if zero(a) || zero(b) {
            cx.report("adding zero has no effect", Label::new(*span, "this addition"));
        }
    }
}

let mut arena = Arena::new();
let x = arena.alloc(Expr::Int(9, Span::new(0, 1)));
let zero = arena.alloc(Expr::Int(0, Span::new(4, 5)));
let sum = arena.alloc(Expr::Add(x, zero, Span::new(0, 5)));

let mut linter = Linter::new();
linter.register(AddZero)?;
assert_eq!(linter.run(&arena, sum).len(), 1);

linter.set_level("add-zero", Level::Allow)?;
assert!(linter.run(&arena, sum).is_empty());
# Ok::<(), lint_lang::LintError>(())
```

---

## The Model

| Item | Role |
|---|---|
| [`Rule`](#rule) | The trait you implement once per check. |
| [`Linter`](#linter) | Registry of rules + their levels; runs them over a tree in one walk. |
| [`Context`](#context) | A rule's view of the current node, and where it reports findings. |
| [`Level`](#level) | `Allow` / `Warn` / `Deny` — how strictly a rule is applied. |
| [`Finding`](#finding) | One reported problem: a rule name plus a `diag-lang` diagnostic. |
| [`LintError`](#linterror) | A configuration mistake caught at setup time. |

A **run** walks the tree once, depth-first, starting at the root handle you pass.
At every node it calls `check` on each rule that is not at `Level::Allow`, in
registration order. Findings accumulate in that same order: node pre-order first,
rule registration order second.

Trees are [`ast-lang`](https://crates.io/crates/ast-lang) trees: nodes of one type
`N: Node` in an `Arena<N>`, linked by `Id<N>` handles. The `Node` trait is three
methods — `span`, `each_child`, `map_children` — and a linter only ever calls
`each_child` (to walk) and nothing that mutates. Findings carry
[`diag-lang`](https://crates.io/crates/diag-lang) diagnostics.

All examples below share a small expression type, hidden from the rendered page
for brevity:

```rust
# use lint_lang::{Id, Node, Span};
enum Expr {
    Int(i64, Span),
    Neg(Id<Expr>, Span),
    Add(Id<Expr>, Id<Expr>, Span),
}
# impl Node for Expr {
#     fn span(&self) -> Span {
#         match self { Expr::Int(_, s) | Expr::Neg(_, s) | Expr::Add(_, _, s) => *s }
#     }
#     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
#         match self {
#             Expr::Int(..) => {}
#             Expr::Neg(a, _) => f(*a),
#             Expr::Add(a, b, _) => { f(*a); f(*b); }
#         }
#     }
#     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
#         match self {
#             Expr::Int(v, s) => Expr::Int(*v, *s),
#             Expr::Neg(a, s) => Expr::Neg(f(*a), *s),
#             Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s),
#         }
#     }
# }
```

---

## Public APIs

## `Rule`

```rust,ignore
pub trait Rule<N>: Send + Sync {
    fn name(&self) -> &'static str;
    fn default_level(&self) -> Level { Level::Warn }
    fn check(&self, node: &N, cx: &mut Context<'_, N>);
}
```

One lint check. `N` is your node type. A rule is a value — usually a unit struct,
or a struct whose fields are configuration such as a threshold — registered with a
[`Linter`](#linter).

**Rules are stateless.** `check` takes `&self`; position information comes from the
[`Context`](#context), not from fields mutated during the walk. That is why the
trait requires `Send + Sync`, and why one linter can be run on many trees, from
many threads, without coordination.

### `Rule::name`

```rust,ignore
fn name(&self) -> &'static str;
```

The rule's unique, stable identifier — the string users write in configuration
(`"double-negation"`), and the string every [`Finding`](#finding) carries. Two
rules in one linter must not share a name ([`LintError::DuplicateRule`](#linterror)).
Prefer lowercase words joined by hyphens. Once published, treat a rule name as
part of your compatibility surface: renaming it breaks users' configuration.

```rust
use lint_lang::{Context, Linter, Rule};
# use lint_lang::{Id, Node, Span};
# enum Expr { Int(i64, Span) }
# impl Node for Expr {
#     fn span(&self) -> Span { match self { Expr::Int(_, s) => *s } }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
#         match self { Expr::Int(v, s) => Expr::Int(*v, *s) }
#     }
# }

struct NoMagicNumbers;
impl Rule<Expr> for NoMagicNumbers {
    fn name(&self) -> &'static str {
        "no-magic-numbers"
    }
    fn check(&self, _node: &Expr, _cx: &mut Context<'_, Expr>) {}
}

let mut linter = Linter::new();
linter.register(NoMagicNumbers)?;
assert_eq!(linter.rules().next().map(|(name, _)| name), Some("no-magic-numbers"));
# Ok::<(), lint_lang::LintError>(())
```

### `Rule::default_level`

```rust,ignore
fn default_level(&self) -> Level { Level::Warn }
```

The level the rule runs at until configured otherwise. The provided default is
[`Level::Warn`](#level). Override it to return:

- `Level::Deny` — for rules that catch real bugs (code that will fail at run time).
- `Level::Allow` — for opinionated, pedantic, or expensive rules users opt into.

```rust
use lint_lang::{Context, Level, Linter, Rule};
# use lint_lang::{Id, Node, Span};
# struct Leaf;
# impl Node for Leaf {
#     fn span(&self) -> Span { Span::new(0, 0) }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
# }

struct Pedantic;
impl Rule<Leaf> for Pedantic {
    fn name(&self) -> &'static str { "pedantic" }
    fn default_level(&self) -> Level { Level::Allow } // opt-in
    fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
}

struct Correctness;
impl Rule<Leaf> for Correctness {
    fn name(&self) -> &'static str { "correctness" }
    fn default_level(&self) -> Level { Level::Deny } // a real bug
    fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
}

let mut linter = Linter::new();
linter.register(Pedantic)?;
linter.register(Correctness)?;
assert_eq!(linter.level("pedantic"), Some(Level::Allow));
assert_eq!(linter.level("correctness"), Some(Level::Deny));
# Ok::<(), lint_lang::LintError>(())
```

### `Rule::check`

```rust,ignore
fn check(&self, node: &N, cx: &mut Context<'_, N>);
```

Inspects one node and reports problems through `cx`. Called once per node, in
depth-first pre-order, for every enabled rule.

| Parameter | Meaning |
|---|---|
| `node` | The node being visited. Match on the variants the rule cares about; return early for the rest. |
| `cx` | The node's [`Context`](#context): its handle, the arena, ancestors, depth, and the `report`/`emit` calls. |

Most nodes are not interesting to most rules, so the early return is the common
path — keep it to a `match` or `let ... else`.

A rule that looks at a node's children through the arena:

```rust
use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
# enum Expr { Int(i64, Span), Neg(Id<Expr>, Span), Add(Id<Expr>, Id<Expr>, Span) }
# impl Node for Expr {
#     fn span(&self) -> Span { match self { Expr::Int(_, s) | Expr::Neg(_, s) | Expr::Add(_, _, s) => *s } }
#     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
#         match self { Expr::Int(..) => {} Expr::Neg(a, _) => f(*a), Expr::Add(a, b, _) => { f(*a); f(*b); } }
#     }
#     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
#         match self { Expr::Int(v, s) => Expr::Int(*v, *s), Expr::Neg(a, s) => Expr::Neg(f(*a), *s),
#                      Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s) }
#     }
# }

/// `-(literal)` should be written as a negative literal.
struct NegatedLiteral;
impl Rule<Expr> for NegatedLiteral {
    fn name(&self) -> &'static str { "negated-literal" }
    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        let Expr::Neg(inner, span) = node else { return };
        if let Some(Expr::Int(value, _)) = cx.arena().get(*inner) {
            if *value > 0 {
                cx.report("negation of a literal", Label::new(*span, "write the literal as negative"));
            }
        }
    }
}

let mut arena = Arena::new();
let lit = arena.alloc(Expr::Int(3, Span::new(1, 2)));
let neg = arena.alloc(Expr::Neg(lit, Span::new(0, 2)));

let mut linter = Linter::new();
linter.register(NegatedLiteral)?;
assert_eq!(linter.run(&arena, neg).len(), 1);
# Ok::<(), lint_lang::LintError>(())
```

---

## `Linter`

```rust,ignore
pub struct Linter<N> { /* private */ }
```

A configured set of rules and the runner that applies them. Build it with
[`new`](#linternew) and [`register`](#linterregister), configure it with
[`set_level`](#linterset_level), and lint with [`run`](#linterrun) or
[`run_into`](#linterrun_into).

- **One walk per run**, whatever the number of rules.
- **Disabled rules cost nothing**: they are never called, and a linter with no
  enabled rules does not walk at all.
- **`Send + Sync`**: share one configured linter across threads by reference.
- Implements `Default` (same as `new`) and `Debug` (lists each rule's name and
  level).

### `Linter::new`

```rust,ignore
pub const fn new() -> Linter<N>
```

Creates a linter with no rules. `const`, so a linter can be built in a `const`
context before rules are added at run time. An empty linter returns no findings.

```rust
use lint_lang::Linter;
# use lint_lang::{Arena, Id, Node, Span};
# struct Leaf;
# impl Node for Leaf {
#     fn span(&self) -> Span { Span::new(0, 0) }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
# }

let linter: Linter<Leaf> = Linter::new();
assert!(linter.is_empty());

let mut arena = Arena::new();
let root = arena.alloc(Leaf);
assert!(linter.run(&arena, root).is_empty());

// `Default` is the same thing.
let also_empty: Linter<Leaf> = Linter::default();
assert_eq!(also_empty.len(), 0);
```

### `Linter::register`

```rust,ignore
pub fn register<R>(&mut self, rule: R) -> Result<(), LintError>
where
    R: Rule<N> + 'static,
```

Adds `rule` at its [default level](#ruledefault_level). Rules run, and their
findings appear for any one node, in registration order.

| Parameter | Meaning |
|---|---|
| `rule` | The rule value, moved into the linter. Must be `'static` (own its configuration). |

**Errors:** [`LintError::DuplicateRule`](#linterror) if a registered rule already
has the same name. The linter is left unchanged — the first registration wins.

```rust
use lint_lang::{Context, Level, Linter, LintError, Rule};
# use lint_lang::{Id, Node, Span};
# struct Leaf;
# impl Node for Leaf {
#     fn span(&self) -> Span { Span::new(0, 0) }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
# }

struct Shadowing;
impl Rule<Leaf> for Shadowing {
    fn name(&self) -> &'static str { "shadowing" }
    fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
}

let mut linter = Linter::new();
linter.register(Shadowing)?;

let again = linter.register(Shadowing);
assert_eq!(again, Err(LintError::DuplicateRule { name: "shadowing" }));
assert_eq!(linter.len(), 1);
assert_eq!(linter.level("shadowing"), Some(Level::Warn));
# Ok::<(), LintError>(())
```

Registering a rule set from a function, propagating any error with `?`:

```rust
use lint_lang::{Context, Linter, LintError, Rule};
# use lint_lang::{Id, Node, Span};
# struct Leaf;
# impl Node for Leaf {
#     fn span(&self) -> Span { Span::new(0, 0) }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
# }
# struct A; struct B;
# impl Rule<Leaf> for A { fn name(&self) -> &'static str { "a" } fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {} }
# impl Rule<Leaf> for B { fn name(&self) -> &'static str { "b" } fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {} }

fn standard_rules() -> Result<Linter<Leaf>, LintError> {
    let mut linter = Linter::new();
    linter.register(A)?;
    linter.register(B)?;
    Ok(linter)
}

assert_eq!(standard_rules()?.len(), 2);
# Ok::<(), LintError>(())
```

### `Linter::set_level`

```rust,ignore
pub fn set_level(&mut self, name: &str, level: Level) -> Result<(), LintError>
```

Sets the level of the rule called `name`, replacing its current level. This is how
user configuration is applied — from a config file, a CLI flag, an editor setting.

| Parameter | Meaning |
|---|---|
| `name` | The rule's [`name`](#rulename). Matched exactly (case-sensitive). |
| `level` | The new level. `Level::Allow` disables the rule; it stops being called. |

**Errors:** [`LintError::UnknownRule`](#linterror) if no registered rule has that
name. The linter is left unchanged.

```rust
use lint_lang::{Context, Level, Linter, LintError, Rule};
# use lint_lang::{Id, Node, Span};
# struct Leaf;
# impl Node for Leaf {
#     fn span(&self) -> Span { Span::new(0, 0) }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
# }
# struct Named(&'static str);
# impl Rule<Leaf> for Named {
#     fn name(&self) -> &'static str { self.0 }
#     fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
# }

let mut linter = Linter::new();
linter.register(Named("unused-value"))?;

linter.set_level("unused-value", Level::Deny)?;
assert_eq!(linter.level("unused-value"), Some(Level::Deny));

linter.set_level("unused-value", Level::Allow)?;
assert_eq!(linter.level("unused-value"), Some(Level::Allow));

let err = linter.set_level("Unused-Value", Level::Warn).unwrap_err();
assert_eq!(err, LintError::UnknownRule { name: "Unused-Value".into() });
# Ok::<(), LintError>(())
```

Applying `-D name` / `-W name` / `-A name` flags:

```rust
use lint_lang::{Context, Level, Linter, LintError, Rule};
# use lint_lang::{Id, Node, Span};
# struct Leaf;
# impl Node for Leaf {
#     fn span(&self) -> Span { Span::new(0, 0) }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
# }
# struct Named(&'static str);
# impl Rule<Leaf> for Named {
#     fn name(&self) -> &'static str { self.0 }
#     fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
# }

let mut linter = Linter::new();
linter.register(Named("shadowing"))?;
linter.register(Named("long-line"))?;

let args = ["-D", "shadowing", "-A", "long-line"];
for pair in args.chunks(2) {
    let level = match pair[0] {
        "-D" => Level::Deny,
        "-W" => Level::Warn,
        _ => Level::Allow,
    };
    linter.set_level(pair[1], level)?;
}
assert_eq!(linter.level("shadowing"), Some(Level::Deny));
assert_eq!(linter.level("long-line"), Some(Level::Allow));
# Ok::<(), LintError>(())
```

### `Linter::level`

```rust,ignore
pub fn level(&self, name: &str) -> Option<Level>
```

Returns the current level of the rule called `name`, or `None` if no registered
rule has that name. Use it to validate configuration keys before applying them, or
to display the effective configuration.

```rust
use lint_lang::{Context, Level, Linter, Rule};
# use lint_lang::{Id, Node, Span};
# struct Leaf;
# impl Node for Leaf {
#     fn span(&self) -> Span { Span::new(0, 0) }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
# }
# struct Named(&'static str);
# impl Rule<Leaf> for Named {
#     fn name(&self) -> &'static str { self.0 }
#     fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
# }

let mut linter = Linter::new();
linter.register(Named("shadowing"))?;

assert_eq!(linter.level("shadowing"), Some(Level::Warn));
assert_eq!(linter.level("nonexistent"), None);
# Ok::<(), lint_lang::LintError>(())
```

### `Linter::rules`

```rust,ignore
pub fn rules(&self) -> impl ExactSizeIterator<Item = (&'static str, Level)> + '_
```

Every registered rule's name and current level, in registration order — for a
`--help` listing, for validating configuration, or for dumping the effective
configuration.

```rust
use lint_lang::{Context, Level, Linter, Rule};
# use lint_lang::{Id, Node, Span};
# struct Leaf;
# impl Node for Leaf {
#     fn span(&self) -> Span { Span::new(0, 0) }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
# }
# struct Named(&'static str, Level);
# impl Rule<Leaf> for Named {
#     fn name(&self) -> &'static str { self.0 }
#     fn default_level(&self) -> Level { self.1 }
#     fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
# }

let mut linter = Linter::new();
linter.register(Named("division-by-zero", Level::Deny))?;
linter.register(Named("identity-op", Level::Allow))?;

// A `--list-lints` screen.
let mut listing = String::new();
for (name, level) in linter.rules() {
    listing.push_str(&format!("{name:<20}{level}\n"));
}
assert_eq!(listing, "division-by-zero    deny\nidentity-op         allow\n");

// Only the rules that will actually run.
let enabled: Vec<_> = linter.rules().filter(|(_, l)| *l != Level::Allow).collect();
assert_eq!(enabled, [("division-by-zero", Level::Deny)]);
# Ok::<(), lint_lang::LintError>(())
```

### `Linter::len` / `Linter::is_empty`

```rust,ignore
pub fn len(&self) -> usize
pub fn is_empty(&self) -> bool
```

The number of registered rules — enabled or not — and whether there are none.

```rust
use lint_lang::{Context, Level, Linter, Rule};
# use lint_lang::{Id, Node, Span};
# struct Leaf;
# impl Node for Leaf {
#     fn span(&self) -> Span { Span::new(0, 0) }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
# }
# struct Named(&'static str);
# impl Rule<Leaf> for Named {
#     fn name(&self) -> &'static str { self.0 }
#     fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
# }

let mut linter = Linter::new();
assert!(linter.is_empty());
linter.register(Named("a"))?;
linter.set_level("a", Level::Allow)?; // disabled rules still count
assert_eq!(linter.len(), 1);
assert!(!linter.is_empty());
# Ok::<(), lint_lang::LintError>(())
```

### `Linter::run`

```rust,ignore
pub fn run(&self, arena: &Arena<N>, root: Id<N>) -> Vec<Finding>
where
    N: Node,
```

Lints the tree rooted at `root` and returns every finding, ordered by node in
depth-first pre-order and, within a node, by rule registration order.

| Parameter | Meaning |
|---|---|
| `arena` | The arena holding the tree. Borrowed for the run; rules read it through [`Context::arena`](#contextarena). |
| `root` | Where the run starts. Any node works — lint a whole file or one subtree. A handle that names no node in `arena` yields no findings. |

The walk is iterative, so trees of any depth are safe.

```rust
use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
# enum Expr { Int(i64, Span), Neg(Id<Expr>, Span), Add(Id<Expr>, Id<Expr>, Span) }
# impl Node for Expr {
#     fn span(&self) -> Span { match self { Expr::Int(_, s) | Expr::Neg(_, s) | Expr::Add(_, _, s) => *s } }
#     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
#         match self { Expr::Int(..) => {} Expr::Neg(a, _) => f(*a), Expr::Add(a, b, _) => { f(*a); f(*b); } }
#     }
#     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
#         match self { Expr::Int(v, s) => Expr::Int(*v, *s), Expr::Neg(a, s) => Expr::Neg(f(*a), *s),
#                      Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s) }
#     }
# }

struct EveryInt;
impl Rule<Expr> for EveryInt {
    fn name(&self) -> &'static str { "every-int" }
    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        if let Expr::Int(..) = node {
            cx.report("integer", Label::unlabelled(node.span()));
        }
    }
}

// (1 + 2) + 3
let mut arena = Arena::new();
let one = arena.alloc(Expr::Int(1, Span::new(0, 1)));
let two = arena.alloc(Expr::Int(2, Span::new(4, 5)));
let inner = arena.alloc(Expr::Add(one, two, Span::new(0, 5)));
let three = arena.alloc(Expr::Int(3, Span::new(8, 9)));
let root = arena.alloc(Expr::Add(inner, three, Span::new(0, 9)));

let mut linter = Linter::new();
linter.register(EveryInt)?;

// Whole tree: pre-order, so 1, 2, then 3.
let starts: Vec<u32> = linter
    .run(&arena, root)
    .iter()
    .map(|f| f.diagnostic().primary().span().start().to_u32())
    .collect();
assert_eq!(starts, [0, 4, 8]);

// Just the left subtree.
assert_eq!(linter.run(&arena, inner).len(), 2);
# Ok::<(), lint_lang::LintError>(())
```

### `Linter::run_into`

```rust,ignore
pub fn run_into(&self, arena: &Arena<N>, root: Id<N>, findings: &mut Vec<Finding>)
where
    N: Node,
```

Same as [`run`](#linterrun), but **appends** to a caller-owned vector instead of
allocating a new one. Existing contents are kept. Clear and reuse one buffer across
a batch to avoid reallocating per file.

| Parameter | Meaning |
|---|---|
| `arena`, `root` | As for [`run`](#linterrun). |
| `findings` | The output buffer. New findings are pushed after whatever it already holds. |

```rust
use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }
# struct Always;
# impl Rule<Leaf> for Always {
#     fn name(&self) -> &'static str { "always" }
#     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) {
#         cx.report("hit", Label::unlabelled(node.span()));
#     }
# }

let mut linter = Linter::new();
linter.register(Always)?;

// Several "files", one reused buffer.
let files: Vec<(Arena<Leaf>, Id<Leaf>)> = (0..3)
    .map(|i| {
        let mut arena = Arena::new();
        let root = arena.alloc(Leaf(Span::new(i, i + 1)));
        (arena, root)
    })
    .collect();

let mut buffer = Vec::new();
let mut per_file = Vec::new();
for (arena, root) in &files {
    buffer.clear();
    linter.run_into(arena, *root, &mut buffer);
    per_file.push(buffer.len());
}
assert_eq!(per_file, [1, 1, 1]);

// Or accumulate everything into one vector.
let mut all = Vec::new();
for (arena, root) in &files {
    linter.run_into(arena, *root, &mut all);
}
assert_eq!(all.len(), 3);
# Ok::<(), lint_lang::LintError>(())
```

---

## `Context`

```rust,ignore
pub struct Context<'a, N> { /* private */ }
```

A rule's view of the node it is checking, and the channel it reports through. The
linter builds one per node and passes it to each enabled rule. It lives only for
the duration of one `check` call. Implements `Debug` (node id, depth, rule,
severity).

### `Context::arena`

```rust,ignore
pub const fn arena(&self) -> &'a Arena<N>
```

The arena the tree lives in. Resolve any handle with `arena.get(id)` — most often
a child of the current node, or an ancestor. The reference is valid for the whole
run (`'a`), so values read from it can be held across a `report` call.

```rust
use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
# enum Expr { Int(i64, Span), Neg(Id<Expr>, Span), Add(Id<Expr>, Id<Expr>, Span) }
# impl Node for Expr {
#     fn span(&self) -> Span { match self { Expr::Int(_, s) | Expr::Neg(_, s) | Expr::Add(_, _, s) => *s } }
#     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
#         match self { Expr::Int(..) => {} Expr::Neg(a, _) => f(*a), Expr::Add(a, b, _) => { f(*a); f(*b); } }
#     }
#     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
#         match self { Expr::Int(v, s) => Expr::Int(*v, *s), Expr::Neg(a, s) => Expr::Neg(f(*a), *s),
#                      Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s) }
#     }
# }

/// `a + a` where both sides are the same literal: suggest `2 * a`.
struct SameOperands;
impl Rule<Expr> for SameOperands {
    fn name(&self) -> &'static str { "same-operands" }
    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        let Expr::Add(a, b, span) = node else { return };
        let arena = cx.arena();
        if let (Some(Expr::Int(x, _)), Some(Expr::Int(y, _))) = (arena.get(*a), arena.get(*b)) {
            if x == y {
                cx.report("both operands are equal", Label::new(*span, "consider `2 * …`"));
            }
        }
    }
}

let mut arena = Arena::new();
let a = arena.alloc(Expr::Int(4, Span::new(0, 1)));
let b = arena.alloc(Expr::Int(4, Span::new(4, 5)));
let sum = arena.alloc(Expr::Add(a, b, Span::new(0, 5)));
let mut linter = Linter::new();
linter.register(SameOperands)?;
assert_eq!(linter.run(&arena, sum).len(), 1);
# Ok::<(), lint_lang::LintError>(())
```

### `Context::id`

```rust,ignore
pub const fn id(&self) -> Id<N>
```

The handle of the node being checked — the same node `check` received by
reference. Useful when a rule wants to compare it with handles it finds elsewhere
(for example, "is this node the left operand of its parent?").

```rust
use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
# enum Expr { Int(i64, Span), Neg(Id<Expr>, Span), Add(Id<Expr>, Id<Expr>, Span) }
# impl Node for Expr {
#     fn span(&self) -> Span { match self { Expr::Int(_, s) | Expr::Neg(_, s) | Expr::Add(_, _, s) => *s } }
#     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
#         match self { Expr::Int(..) => {} Expr::Neg(a, _) => f(*a), Expr::Add(a, b, _) => { f(*a); f(*b); } }
#     }
#     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
#         match self { Expr::Int(v, s) => Expr::Int(*v, *s), Expr::Neg(a, s) => Expr::Neg(f(*a), *s),
#                      Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s) }
#     }
# }

/// Flags integers that are the *right* operand of an addition.
struct RightOperand;
impl Rule<Expr> for RightOperand {
    fn name(&self) -> &'static str { "right-operand" }
    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        if !matches!(node, Expr::Int(..)) { return; }
        let Some(parent) = cx.parent() else { return };
        if let Some(Expr::Add(_, rhs, _)) = cx.arena().get(parent) {
            if *rhs == cx.id() {
                cx.report("right operand", Label::unlabelled(node.span()));
            }
        }
    }
}

let mut arena = Arena::new();
let l = arena.alloc(Expr::Int(1, Span::new(0, 1)));
let r = arena.alloc(Expr::Int(2, Span::new(4, 5)));
let sum = arena.alloc(Expr::Add(l, r, Span::new(0, 5)));
let mut linter = Linter::new();
linter.register(RightOperand)?;
let findings = linter.run(&arena, sum);
assert_eq!(findings.len(), 1);
assert_eq!(findings[0].diagnostic().primary().span(), Span::new(4, 5));
# Ok::<(), lint_lang::LintError>(())
```

### `Context::parent`

```rust,ignore
pub fn parent(&self) -> Option<Id<N>>
```

The parent's handle, or `None` at the root of the run. "Root of the run" is the
`root` passed to [`run`](#linterrun) — when linting a subtree, its top node has no
parent even if the full tree continues above it. See [`Context::id`](#contextid)
for an example.

### `Context::ancestors`

```rust,ignore
pub const fn ancestors(&self) -> &'a [Id<N>]
```

Every ancestor of the node, **root first**, ending with the parent; empty at the
root. The slice is the walk's own path stack, so reading it is free. Iterate in
reverse to search outward from the node.

```rust
use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
# enum Expr { Int(i64, Span), Neg(Id<Expr>, Span), Add(Id<Expr>, Id<Expr>, Span) }
# impl Node for Expr {
#     fn span(&self) -> Span { match self { Expr::Int(_, s) | Expr::Neg(_, s) | Expr::Add(_, _, s) => *s } }
#     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
#         match self { Expr::Int(..) => {} Expr::Neg(a, _) => f(*a), Expr::Add(a, b, _) => { f(*a); f(*b); } }
#     }
#     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
#         match self { Expr::Int(v, s) => Expr::Int(*v, *s), Expr::Neg(a, s) => Expr::Neg(f(*a), *s),
#                      Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s) }
#     }
# }

/// Flags integers with an odd number of enclosing negations (net negative).
struct NetNegative;
impl Rule<Expr> for NetNegative {
    fn name(&self) -> &'static str { "net-negative" }
    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        if !matches!(node, Expr::Int(..)) { return; }
        let arena = cx.arena();
        let negations = cx
            .ancestors()
            .iter()
            .filter(|&&id| matches!(arena.get(id), Some(Expr::Neg(..))))
            .count();
        if negations % 2 == 1 {
            cx.report("value is negated overall", Label::unlabelled(node.span()));
        }
    }
}

// -(1 + -(-2)): `1` is under one negation, `2` under three.
let mut arena = Arena::new();
let two = arena.alloc(Expr::Int(2, Span::new(9, 10)));
let n1 = arena.alloc(Expr::Neg(two, Span::new(8, 10)));
let n2 = arena.alloc(Expr::Neg(n1, Span::new(6, 11)));
let one = arena.alloc(Expr::Int(1, Span::new(2, 3)));
let sum = arena.alloc(Expr::Add(one, n2, Span::new(2, 11)));
let root = arena.alloc(Expr::Neg(sum, Span::new(0, 12)));

let mut linter = Linter::new();
linter.register(NetNegative)?;
assert_eq!(linter.run(&arena, root).len(), 2);
# Ok::<(), lint_lang::LintError>(())
```

### `Context::depth`

```rust,ignore
pub const fn depth(&self) -> usize
```

The number of ancestors: `0` at the root of the run, `1` for its children, and so
on. Equal to `cx.ancestors().len()`.

```rust
use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
# enum Expr { Int(i64, Span), Neg(Id<Expr>, Span), Add(Id<Expr>, Id<Expr>, Span) }
# impl Node for Expr {
#     fn span(&self) -> Span { match self { Expr::Int(_, s) | Expr::Neg(_, s) | Expr::Add(_, _, s) => *s } }
#     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
#         match self { Expr::Int(..) => {} Expr::Neg(a, _) => f(*a), Expr::Add(a, b, _) => { f(*a); f(*b); } }
#     }
#     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
#         match self { Expr::Int(v, s) => Expr::Int(*v, *s), Expr::Neg(a, s) => Expr::Neg(f(*a), *s),
#                      Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s) }
#     }
# }

/// Reports once per path, at the first node deeper than `max`.
struct MaxDepth { max: usize }
impl Rule<Expr> for MaxDepth {
    fn name(&self) -> &'static str { "max-depth" }
    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        if cx.depth() == self.max + 1 {
            cx.report("nested too deeply", Label::unlabelled(node.span()));
        }
    }
}

// ----1: five nodes, depths 0..=4.
let mut arena = Arena::new();
let mut node = arena.alloc(Expr::Int(1, Span::new(4, 5)));
for start in (0..4).rev() {
    node = arena.alloc(Expr::Neg(node, Span::new(start, 5)));
}

let mut linter = Linter::new();
linter.register(MaxDepth { max: 2 })?;
let findings = linter.run(&arena, node);
assert_eq!(findings.len(), 1); // only the depth-3 node, not everything below it
assert_eq!(findings[0].diagnostic().primary().span(), Span::new(3, 5));
# Ok::<(), lint_lang::LintError>(())
```

### `Context::rule`

```rust,ignore
pub const fn rule(&self) -> &'static str
```

The name of the rule currently running — the same value as that rule's
[`name`](#rulename). Handy inside shared helper functions that several rules call.

### `Context::severity`

```rust,ignore
pub const fn severity(&self) -> Severity
```

The severity the running rule is configured for: `Severity::Warning` at
`Level::Warn`, `Severity::Error` at `Level::Deny`. (A rule at `Level::Allow` never
runs, so `check` never sees it.) [`report`](#contextreport) applies this
automatically; read it when building a diagnostic for [`emit`](#contextemit).

```rust
use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Severity, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }

/// Words its message differently when the user has made it an error.
struct Tone;
impl Rule<Leaf> for Tone {
    fn name(&self) -> &'static str { "tone" }
    fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) {
        let message = match cx.severity() {
            Severity::Error => "this is not allowed",
            _ => "this is discouraged",
        };
        cx.report(message, Label::unlabelled(node.span()));
    }
}

let mut arena = Arena::new();
let root = arena.alloc(Leaf(Span::new(0, 1)));
let mut linter = Linter::new();
linter.register(Tone)?;
assert_eq!(linter.run(&arena, root)[0].diagnostic().message(), "this is discouraged");
linter.set_level("tone", Level::Deny)?;
assert_eq!(linter.run(&arena, root)[0].diagnostic().message(), "this is not allowed");
# Ok::<(), lint_lang::LintError>(())
```

### `Context::report`

```rust,ignore
pub fn report(&mut self, message: impl Into<Box<str>>, label: Label)
```

Records a finding: a diagnostic headed by `message`, pointing at `label`, at the
rule's configured severity. This is the call almost every rule needs.

| Parameter | Meaning |
|---|---|
| `message` | The headline — what is wrong. A `&'static str` or a `String` built with `format!`. |
| `label` | Where it is wrong: `Label::new(span, "text")` underlines `span` with text, `Label::unlabelled(span)` just underlines it. |

```rust
use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Severity, Span};
# enum Expr { Int(i64, Span), Neg(Id<Expr>, Span), Add(Id<Expr>, Id<Expr>, Span) }
# impl Node for Expr {
#     fn span(&self) -> Span { match self { Expr::Int(_, s) | Expr::Neg(_, s) | Expr::Add(_, _, s) => *s } }
#     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
#         match self { Expr::Int(..) => {} Expr::Neg(a, _) => f(*a), Expr::Add(a, b, _) => { f(*a); f(*b); } }
#     }
#     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
#         match self { Expr::Int(v, s) => Expr::Int(*v, *s), Expr::Neg(a, s) => Expr::Neg(f(*a), *s),
#                      Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s) }
#     }
# }

struct LargeInt { limit: i64 }
impl Rule<Expr> for LargeInt {
    fn name(&self) -> &'static str { "large-int" }
    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        if let Expr::Int(v, span) = node {
            if *v > self.limit {
                // A dynamic message, built only when the rule fires.
                cx.report(
                    format!("literal {v} exceeds the limit of {}", self.limit),
                    Label::new(*span, "too large"),
                );
            }
        }
    }
}

let mut arena = Arena::new();
let root = arena.alloc(Expr::Int(5_000, Span::new(0, 4)));
let mut linter = Linter::new();
linter.register(LargeInt { limit: 255 })?;

let f = &linter.run(&arena, root)[0];
assert_eq!(f.diagnostic().message(), "literal 5000 exceeds the limit of 255");
assert_eq!(f.diagnostic().primary().message(), "too large");
assert_eq!(f.severity(), Severity::Warning);
# Ok::<(), lint_lang::LintError>(())
```

### `Context::emit`

```rust,ignore
pub fn emit(&mut self, diagnostic: Diagnostic)
```

Records a diagnostic the rule built itself, attributed to the running rule. Use it
for secondary labels, notes, or help text. The diagnostic is recorded **as given** —
build it with [`cx.severity()`](#contextseverity) so the user's configured level is
honoured.

| Parameter | Meaning |
|---|---|
| `diagnostic` | A complete `diag_lang::Diagnostic`. |

```rust
use lint_lang::{Arena, Context, Diagnostic, Id, Label, Linter, Node, Rule, Span};
# enum Expr { Int(i64, Span), Neg(Id<Expr>, Span), Add(Id<Expr>, Id<Expr>, Span) }
# impl Node for Expr {
#     fn span(&self) -> Span { match self { Expr::Int(_, s) | Expr::Neg(_, s) | Expr::Add(_, _, s) => *s } }
#     fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
#         match self { Expr::Int(..) => {} Expr::Neg(a, _) => f(*a), Expr::Add(a, b, _) => { f(*a); f(*b); } }
#     }
#     fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
#         match self { Expr::Int(v, s) => Expr::Int(*v, *s), Expr::Neg(a, s) => Expr::Neg(f(*a), *s),
#                      Expr::Add(a, b, s) => Expr::Add(f(*a), f(*b), *s) }
#     }
# }

struct DoubleNegation;
impl Rule<Expr> for DoubleNegation {
    fn name(&self) -> &'static str { "double-negation" }
    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        let Expr::Neg(inner, outer_span) = node else { return };
        let Some(Expr::Neg(_, inner_span)) = cx.arena().get(*inner) else { return };
        let diag = Diagnostic::new(
            cx.severity(),
            "negation of a negation",
            Label::new(*outer_span, "outer negation"),
        )
        .with_secondary(Label::new(*inner_span, "cancels this one"))
        .with_help("remove both `-` signs");
        cx.emit(diag);
    }
}

let mut arena = Arena::new();
let x = arena.alloc(Expr::Int(1, Span::new(2, 3)));
let n = arena.alloc(Expr::Neg(x, Span::new(1, 3)));
let nn = arena.alloc(Expr::Neg(n, Span::new(0, 3)));
let mut linter = Linter::new();
linter.register(DoubleNegation)?;

let f = &linter.run(&arena, nn)[0];
assert_eq!(f.rule(), "double-negation");
assert_eq!(f.diagnostic().secondary().len(), 1);
assert_eq!(f.diagnostic().help().next(), Some("remove both `-` signs"));
# Ok::<(), lint_lang::LintError>(())
```

---

## `Level`

```rust,ignore
pub enum Level {
    Allow,
    Warn,
    Deny,
}
```

How strictly a rule is applied — the `rustc`/Clippy model.

| Variant | Effect | Finding severity |
|---|---|---|
| `Allow` | Rule is disabled and **never called**. | — |
| `Warn` | Rule runs; findings are warnings. | `Severity::Warning` |
| `Deny` | Rule runs; findings are errors (a driver typically fails the build). | `Severity::Error` |

Derives `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`.
Ordered by strictness, `Allow < Warn < Deny`, so `max` picks the stricter level:

```rust
use lint_lang::Level;

assert!(Level::Allow < Level::Warn && Level::Warn < Level::Deny);

// A project config says `warn`, a CI override says "at least deny".
let effective = Level::Warn.max(Level::Deny);
assert_eq!(effective, Level::Deny);
```

### `Level::severity`

```rust,ignore
pub const fn severity(self) -> Option<Severity>
```

The `diag-lang` severity findings at this level carry, or `None` for `Allow`.

```rust
use lint_lang::{Level, Severity};

assert_eq!(Level::Allow.severity(), None);
assert_eq!(Level::Warn.severity(), Some(Severity::Warning));
assert_eq!(Level::Deny.severity(), Some(Severity::Error));
```

### `Level::as_str`

```rust,ignore
pub const fn as_str(self) -> &'static str
```

The lowercase name: `"allow"`, `"warn"`, or `"deny"` — the spelling `FromStr`
accepts and the `serde` feature writes.

```rust
use lint_lang::Level;

assert_eq!(Level::Deny.as_str(), "deny");
let names: Vec<_> = [Level::Allow, Level::Warn, Level::Deny].map(Level::as_str).into();
assert_eq!(names, ["allow", "warn", "deny"]);
```

### Parsing and formatting

`Level` implements `Display` (the [`as_str`](#levelas_str) name) and
`FromStr<Err = LintError>`. Parsing accepts `allow`, `warn`, and `deny`, **ignoring
ASCII case**; it does not trim whitespace. Anything else is
[`LintError::InvalidLevel`](#linterror) carrying the input.

```rust
use lint_lang::{Level, LintError};

assert_eq!("warn".parse::<Level>()?, Level::Warn);
assert_eq!("DENY".parse::<Level>()?, Level::Deny);
assert_eq!(Level::Allow.to_string(), "allow");

assert_eq!(
    "warning".parse::<Level>(),
    Err(LintError::InvalidLevel { input: "warning".into() })
);
assert!(" warn".parse::<Level>().is_err()); // trim first
# Ok::<(), LintError>(())
```

With the `serde` feature, `Level` serializes to and deserializes from the same
lowercase strings (exact case only; `"Warn"` is rejected by `serde`).

---

## `Finding`

```rust,ignore
pub struct Finding { /* private */ }
```

One problem a rule reported: a `diag_lang::Diagnostic` tagged with the rule's name.
Findings are produced only by a [`Linter`](#linter) run. Derives `Clone`, `Debug`,
`PartialEq`, `Eq`; implements `Display`; marked `#[must_use]`.

All examples in this section use this setup (hidden on the rendered page):

```rust
# use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }
struct Flag(&'static str, Level);
impl Rule<Leaf> for Flag {
    fn name(&self) -> &'static str { self.0 }
    fn default_level(&self) -> Level { self.1 }
    fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) {
        cx.report("flagged", Label::new(node.span(), "here"));
    }
}
# let mut arena = Arena::new();
# let root = arena.alloc(Leaf(Span::new(0, 4)));
# let mut linter = Linter::new();
# linter.register(Flag("soft", Level::Warn))?;
# linter.register(Flag("hard", Level::Deny))?;
# assert_eq!(linter.run(&arena, root).len(), 2);
# Ok::<(), lint_lang::LintError>(())
```

### `Finding::rule`

```rust,ignore
pub const fn rule(&self) -> &'static str
```

The name of the rule that reported the finding — exactly the string
[`Linter::set_level`](#linterset_level) accepts, so it tells the user which setting
controls it.

```rust
# use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }
# struct Flag(&'static str, Level);
# impl Rule<Leaf> for Flag {
#     fn name(&self) -> &'static str { self.0 }
#     fn default_level(&self) -> Level { self.1 }
#     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) { cx.report("flagged", Label::new(node.span(), "here")); }
# }
# let mut arena = Arena::new();
# let root = arena.alloc(Leaf(Span::new(0, 4)));
# let mut linter = Linter::new();
# linter.register(Flag("soft", Level::Warn))?;
# linter.register(Flag("hard", Level::Deny))?;
let findings = linter.run(&arena, root);
let rules: Vec<_> = findings.iter().map(|f| f.rule()).collect();
assert_eq!(rules, ["soft", "hard"]); // registration order within a node

// Silence whatever fired, by name.
for f in &findings {
    linter.set_level(f.rule(), Level::Allow)?;
}
assert!(linter.run(&arena, root).is_empty());
# Ok::<(), lint_lang::LintError>(())
```

### `Finding::diagnostic`

```rust,ignore
pub const fn diagnostic(&self) -> &Diagnostic
```

The diagnostic: severity, message, primary and secondary labels, notes, help. Pass
it to `diag_lang::Renderer` to print with source context.

```rust
use diag_lang::{Renderer, SourceMap};
# use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }
# struct Flag(&'static str, Level);
# impl Rule<Leaf> for Flag {
#     fn name(&self) -> &'static str { self.0 }
#     fn default_level(&self) -> Level { self.1 }
#     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) { cx.report("flagged", Label::new(node.span(), "here")); }
# }
# let mut arena = Arena::new();
# let root = arena.alloc(Leaf(Span::new(0, 4)));
# let mut linter = Linter::new();
# linter.register(Flag("soft", Level::Warn))?;
let findings = linter.run(&arena, root);
let diag = findings[0].diagnostic();
assert_eq!(diag.message(), "flagged");
assert_eq!(diag.primary().span(), Span::new(0, 4));

let mut map = SourceMap::new();
let _file = map.add("demo.src", "word").expect("fits");
let text = Renderer::new().render(diag, &map);
assert!(text.starts_with("warning: flagged"));
assert!(text.contains("^^^^ here"));
# Ok::<(), lint_lang::LintError>(())
```

### `Finding::severity`

```rust,ignore
pub const fn severity(&self) -> Severity
```

The diagnostic's severity. For findings reported with
[`Context::report`](#contextreport), this follows the rule's level:
`Warning` for `Warn`, `Error` for `Deny`.

```rust
use lint_lang::Severity;
# use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }
# struct Flag(&'static str, Level);
# impl Rule<Leaf> for Flag {
#     fn name(&self) -> &'static str { self.0 }
#     fn default_level(&self) -> Level { self.1 }
#     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) { cx.report("flagged", Label::new(node.span(), "here")); }
# }
# let mut arena = Arena::new();
# let root = arena.alloc(Leaf(Span::new(0, 4)));
# let mut linter = Linter::new();
# linter.register(Flag("soft", Level::Warn))?;
# linter.register(Flag("hard", Level::Deny))?;
let mut findings = linter.run(&arena, root);
// Severity orders errors first, so sorting puts the most serious on top.
findings.sort_by_key(|f| f.severity());
assert_eq!(findings[0].severity(), Severity::Error);
assert_eq!(findings[1].severity(), Severity::Warning);
# Ok::<(), lint_lang::LintError>(())
```

### `Finding::is_error`

```rust,ignore
pub fn is_error(&self) -> bool
```

`true` when the severity is `Severity::Error` — the signal a driver uses to fail a
build after linting.

```rust
# use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }
# struct Flag(&'static str, Level);
# impl Rule<Leaf> for Flag {
#     fn name(&self) -> &'static str { self.0 }
#     fn default_level(&self) -> Level { self.1 }
#     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) { cx.report("flagged", Label::new(node.span(), "here")); }
# }
# let mut arena = Arena::new();
# let root = arena.alloc(Leaf(Span::new(0, 4)));
# let mut linter = Linter::new();
# linter.register(Flag("soft", Level::Warn))?;
# linter.register(Flag("hard", Level::Deny))?;
use lint_lang::Finding;

let findings = linter.run(&arena, root);
let errors = findings.iter().filter(|f| f.is_error()).count();
assert_eq!(errors, 1);
assert!(findings.iter().any(Finding::is_error)); // the build fails
# Ok::<(), lint_lang::LintError>(())
```

### `Finding::into_diagnostic`

```rust,ignore
pub fn into_diagnostic(self) -> Diagnostic
```

Consumes the finding and returns its diagnostic without copying — for merging lint
results into a compiler's general diagnostic stream once the rule name is no longer
needed.

```rust
use lint_lang::Diagnostic;
# use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }
# struct Flag(&'static str, Level);
# impl Rule<Leaf> for Flag {
#     fn name(&self) -> &'static str { self.0 }
#     fn default_level(&self) -> Level { self.1 }
#     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) { cx.report("flagged", Label::new(node.span(), "here")); }
# }
# let mut arena = Arena::new();
# let root = arena.alloc(Leaf(Span::new(0, 4)));
# let mut linter = Linter::new();
# linter.register(Flag("soft", Level::Warn))?;

// Diagnostics already collected from parsing and type checking...
let mut all: Vec<Diagnostic> = Vec::new();
// ...plus the lint results.
all.extend(linter.run(&arena, root).into_iter().map(|f| f.into_diagnostic()));
assert_eq!(all.len(), 1);
# Ok::<(), lint_lang::LintError>(())
```

### `Display` for `Finding`

Formats a one-line summary, `severity[rule]: message` — suitable for logs, terse
CLI output, or test assertions. For a full report with source context, render
[`diagnostic`](#findingdiagnostic) instead.

```rust
# use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }
# struct Flag(&'static str, Level);
# impl Rule<Leaf> for Flag {
#     fn name(&self) -> &'static str { self.0 }
#     fn default_level(&self) -> Level { self.1 }
#     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) { cx.report("flagged", Label::new(node.span(), "here")); }
# }
# let mut arena = Arena::new();
# let root = arena.alloc(Leaf(Span::new(0, 4)));
# let mut linter = Linter::new();
# linter.register(Flag("soft", Level::Warn))?;
# linter.register(Flag("hard", Level::Deny))?;
let lines: Vec<String> = linter.run(&arena, root).iter().map(ToString::to_string).collect();
assert_eq!(lines, ["warning[soft]: flagged", "error[hard]: flagged"]);
# Ok::<(), lint_lang::LintError>(())
```

---

## `LintError`

```rust,ignore
#[non_exhaustive]
pub enum LintError {
    DuplicateRule { name: &'static str },
    UnknownRule { name: Box<str> },
    InvalidLevel { input: Box<str> },
}
```

A configuration mistake, caught before any tree is linted. Running a configured
linter never fails, so this only appears at setup. Implements `Display`,
`core::error::Error` (and therefore `std::error::Error`), `Clone`, `Debug`,
`PartialEq`, `Eq`. `#[non_exhaustive]`: match with a wildcard arm.

| Variant | Returned by | Meaning | What to do |
|---|---|---|---|
| `DuplicateRule { name }` | [`Linter::register`](#linterregister) | A rule with this name is already registered. The linter is unchanged. | Rename one rule, or do not register the duplicate. |
| `UnknownRule { name }` | [`Linter::set_level`](#linterset_level) | No registered rule has this name — usually a typo in configuration. The linter is unchanged. | Report the name to the user; offer the list from [`Linter::rules`](#linterrules). |
| `InvalidLevel { input }` | `Level::from_str` | The text is not `allow`, `warn`, or `deny`. | Report the value and the accepted spellings. |

The `Display` output is a complete sentence suitable for showing to a user:

```rust
use lint_lang::{Context, Level, Linter, LintError, Rule};
# use lint_lang::{Id, Node, Span};
# struct Leaf;
# impl Node for Leaf {
#     fn span(&self) -> Span { Span::new(0, 0) }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
# }
# struct Named(&'static str);
# impl Rule<Leaf> for Named {
#     fn name(&self) -> &'static str { self.0 }
#     fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
# }

let mut linter = Linter::new();
linter.register(Named("shadowing"))?;

let dup = linter.register(Named("shadowing")).unwrap_err();
assert_eq!(dup.to_string(), "a lint rule named `shadowing` is already registered");

let unknown = linter.set_level("shadowign", Level::Deny).unwrap_err();
assert_eq!(unknown.to_string(), "no lint rule named `shadowign` is registered");

let invalid = "strict".parse::<Level>().unwrap_err();
assert_eq!(
    invalid.to_string(),
    "invalid lint level `strict` (expected `allow`, `warn`, or `deny`)"
);
# Ok::<(), LintError>(())
```

Handling each case, with a wildcard for future variants:

```rust
use lint_lang::LintError;

fn explain(err: &LintError) -> String {
    match err {
        LintError::UnknownRule { name } => format!("check the spelling of `{name}`"),
        LintError::InvalidLevel { input } => format!("`{input}` must be allow, warn, or deny"),
        LintError::DuplicateRule { name } => format!("`{name}` is registered twice"),
        other => other.to_string(),
    }
}

let err = LintError::UnknownRule { name: "x".into() };
assert_eq!(explain(&err), "check the spelling of `x`");

// It is a standard error, so it composes with `?` into `Box<dyn Error>`.
fn configure() -> Result<(), Box<dyn std::error::Error>> {
    let _level: lint_lang::Level = "warn".parse()?;
    Ok(())
}
assert!(configure().is_ok());
```

---

## Re-exports

lint-lang re-exports the types its API is built on, so one dependency is enough
to write and run rules:

| Item | From | Used for |
|---|---|---|
| `Node` | `ast-lang` | The trait your node type implements (`span`, `each_child`, `map_children`). |
| `Arena` | `ast-lang` (`arena-lang`) | Storage for the tree; passed to `run`. |
| `Id` | `ast-lang` (`arena-lang`) | A typed handle to a node; the `root` of a run. |
| `Span` | `ast-lang` (`span-lang` 0.4) | A node's source range; what a `Label` points at. |
| `Diagnostic` | `diag-lang` | The diagnostic inside every `Finding`; built by hand for `emit`. |
| `Label` | `diag-lang` | A span plus text, passed to `report`. |
| `Severity` | `diag-lang` | `Error` / `Warning` / …; what a `Level` maps to. |

`ast-lang` and `diag-lang` resolve to the same `span-lang`, so a node's `Span` is
the very type a `Label` takes. To render findings, use `diag_lang::Renderer` and
`diag_lang::SourceMap` from `diag-lang` directly.

```rust
// Everything a rule needs, from one crate.
use lint_lang::{Arena, Diagnostic, Id, Label, Node, Severity, Span};

struct Leaf(Span);
impl Node for Leaf {
    fn span(&self) -> Span { self.0 }
    fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
    fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
}

let mut arena = Arena::new();
let id = arena.alloc(Leaf(Span::new(3, 7)));
let span = arena.get(id).map(Node::span).expect("live handle");
let diag = Diagnostic::new(Severity::Warning, "example", Label::unlabelled(span));
assert_eq!(diag.primary().span(), Span::new(3, 7));
```

---

## Feature Flags

| Feature | Default | Effect |
|---|---|---|
| `std` | yes | Links the standard library. Without it the crate is `#![no_std]` and needs only `alloc`; nothing is lost. Forwards to `ast-lang/std` and `diag-lang/std`. |
| `serde` | no | `Serialize` / `Deserialize` for `Level` as `"allow"`, `"warn"`, `"deny"`. |

`no_std` build:

```toml
[dependencies]
lint-lang = { version = "0.2", default-features = false }
```

---

## Patterns

### Failing a build on `deny` findings

```rust
use lint_lang::{Arena, Context, Finding, Id, Label, Level, Linter, Node, Rule, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }
# struct Flag;
# impl Rule<Leaf> for Flag {
#     fn name(&self) -> &'static str { "flag" }
#     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) { cx.report("flagged", Label::unlabelled(node.span())); }
# }

/// Returns the process exit code a lint step should produce.
fn exit_code(findings: &[Finding]) -> i32 {
    if findings.iter().any(Finding::is_error) { 1 } else { 0 }
}

let mut arena = Arena::new();
let root = arena.alloc(Leaf(Span::new(0, 1)));
let mut linter = Linter::new();
linter.register(Flag)?;

assert_eq!(exit_code(&linter.run(&arena, root)), 0); // warnings only
linter.set_level("flag", Level::Deny)?;
assert_eq!(exit_code(&linter.run(&arena, root)), 1); // now an error
# Ok::<(), lint_lang::LintError>(())
```

### Loading levels from a configuration file

Any format that yields `name → level` pairs works. A minimal `name = level` reader
that reports both kinds of mistake:

```rust
use lint_lang::{Context, Linter, LintError, Rule};
# use lint_lang::{Id, Node, Span};
# struct Leaf;
# impl Node for Leaf {
#     fn span(&self) -> Span { Span::new(0, 0) }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf }
# }
# struct Named(&'static str);
# impl Rule<Leaf> for Named {
#     fn name(&self) -> &'static str { self.0 }
#     fn check(&self, _: &Leaf, _: &mut Context<'_, Leaf>) {}
# }

fn apply(linter: &mut Linter<Leaf>, text: &str) -> Result<(), LintError> {
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, level) = line.split_once('=').unwrap_or((line, ""));
        linter.set_level(name.trim(), level.trim().parse()?)?;
    }
    Ok(())
}

let mut linter = Linter::new();
linter.register(Named("shadowing"))?;

apply(&mut linter, "# project lints\nshadowing = deny")?;
assert!(matches!(apply(&mut linter, "shadowng = deny"), Err(LintError::UnknownRule { .. })));
assert!(matches!(apply(&mut linter, "shadowing = loud"), Err(LintError::InvalidLevel { .. })));
# Ok::<(), LintError>(())
```

With `serde`, deserialize a `BTreeMap<String, Level>` from TOML or JSON and feed
each entry to `set_level` in the same way.

### Searching outward through ancestors

`cx.ancestors()` is root-first; reverse it to find the *nearest* enclosing node of
some kind. See [`Context::ancestors`](#contextancestors) for a complete example.

```rust,ignore
let nearest_fn = cx
    .ancestors()
    .iter()
    .rev()
    .find(|&&id| matches!(cx.arena().get(id), Some(Node::Function { .. })));
```

### Configurable rules

A rule's fields are its configuration. Construct it with the user's settings
before registering it:

```rust
use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }

struct MaxLength { max: u32 }
impl Rule<Leaf> for MaxLength {
    fn name(&self) -> &'static str { "max-length" }
    fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) {
        if node.span().len() > self.max {
            cx.report(format!("longer than {} bytes", self.max), Label::unlabelled(node.span()));
        }
    }
}

let mut arena = Arena::new();
let root = arena.alloc(Leaf(Span::new(0, 120)));

let strict = { let mut l = Linter::new(); l.register(MaxLength { max: 80 })?; l };
let relaxed = { let mut l = Linter::new(); l.register(MaxLength { max: 200 })?; l };
assert_eq!(strict.run(&arena, root).len(), 1);
assert!(relaxed.run(&arena, root).is_empty());
# Ok::<(), lint_lang::LintError>(())
```

### Linting many files in parallel

```rust
use std::thread;
use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};
# struct Leaf(Span);
# impl Node for Leaf {
#     fn span(&self) -> Span { self.0 }
#     fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
#     fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Leaf(self.0) }
# }
# struct Empty;
# impl Rule<Leaf> for Empty {
#     fn name(&self) -> &'static str { "empty" }
#     fn check(&self, node: &Leaf, cx: &mut Context<'_, Leaf>) {
#         if node.span().is_empty() { cx.report("empty", Label::unlabelled(node.span())); }
#     }
# }

let files: Vec<(Arena<Leaf>, Id<Leaf>)> = (0..8u32)
    .map(|i| {
        let mut arena = Arena::new();
        let root = arena.alloc(Leaf(Span::new(0, i % 2))); // every other file is empty
        (arena, root)
    })
    .collect();

let mut linter = Linter::new();
linter.register(Empty)?;
let linter = &linter; // shared by every thread

let total: usize = thread::scope(|scope| {
    let handles: Vec<_> = files
        .chunks(2)
        .map(|batch| scope.spawn(move || {
            batch.iter().map(|(arena, root)| linter.run(arena, *root).len()).sum::<usize>()
        }))
        .collect();
    handles.into_iter().map(|h| h.join().unwrap_or(0)).sum()
});
assert_eq!(total, 4);
# Ok::<(), lint_lang::LintError>(())
```

---

## Performance Notes

- **Cost model.** A run is `O(nodes × enabled rules)` calls of `check`, over a
  single walk. Measured on x86_64 (see the README's table): the bare walk is
  ~1.9 ns/node; one enabled no-op rule brings it to ~4.7 ns/node; each further
  enabled rule adds ~1 ns/node (one dynamic call); everything allowed is a constant
  ~1.4 ns regardless of tree size.
- **Disabled means free.** `Allow` rules are skipped in the inner loop, and with
  no enabled rules the walk is skipped entirely. Prefer `default_level =
  Level::Allow` for expensive opt-in rules over checking a flag inside `check`.
- **Keep the miss path short.** Most nodes do not interest most rules. Return
  from `check` with a single `match` / `let … else` before doing any work.
- **Build messages only when firing.** `report` takes `impl Into<Box<str>>`: a
  `&'static str` message costs one small allocation, a `format!` costs its own —
  both only on the firing path.
- **Reuse output buffers.** `run_into` appends to a caller's vector; clear and
  reuse it across files.
- **Allocation.** A run allocates the walk's stacks (proportional to tree depth
  and fan-out) and the findings it produces — nothing per node otherwise.

---

## Guarantees

- **Deterministic order.** Findings are in depth-first pre-order by node, then rule
  registration order. Identical tree + configuration ⇒ identical output.
- **No panics from the library.** The crate's own code has no `unwrap`, `expect`,
  `panic!`, or slice indexing (the first three are denied by lint), and a stale
  `root` handle yields no findings. A panic inside a user's `check` propagates as
  usual.
- **Stack-safe.** The walk is iterative; tree depth is limited only by memory.
- **Thread-safe sharing.** `Linter<N>: Send + Sync` for every `N`; `run` takes
  `&self`.
- **`#![forbid(unsafe_code)]`** and `no_std`-compatible.

<br>

<sub>Copyright &copy; 2026 <strong>James Gober</strong>.</sub>
