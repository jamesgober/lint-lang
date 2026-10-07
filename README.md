<h1 align="center">
    <img width="99" alt="Rust logo" src="https://raw.githubusercontent.com/jamesgober/rust-collection/72baabd71f00e14aa9184efcb16fa3deddda3a0a/assets/rust-logo.svg">
    <br>
    <b>lint-lang</b>
    <br>
    <sub><sup>LINT FRAMEWORK</sup></sub>
</h1>

<div align="center">
    <a href="https://crates.io/crates/lint-lang"><img alt="Crates.io" src="https://img.shields.io/crates/v/lint-lang"></a>
    <a href="https://crates.io/crates/lint-lang"><img alt="Downloads" src="https://img.shields.io/crates/d/lint-lang?color=%230099ff"></a>
    <a href="https://docs.rs/lint-lang"><img alt="docs.rs" src="https://img.shields.io/docsrs/lint-lang"></a>
    <a href="https://github.com/jamesgober/lint-lang/actions"><img alt="CI" src="https://github.com/jamesgober/lint-lang/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://github.com/rust-lang/rfcs/blob/master/text/2495-min-rust-version.md"><img alt="MSRV" src="https://img.shields.io/badge/MSRV-1.85%2B-blue"></a>
</div>

<br>

<div align="left">
    <p>
        <strong>lint-lang</strong> is a lint framework for language tooling: named, configurable rules that run over a syntax tree and report what they find as diagnostics. You write the rules for your language; lint-lang runs them in a single pass over the tree, applies the user's <code>allow</code> / <code>warn</code> / <code>deny</code> configuration, and hands back findings that render with source context like any other compiler error.
    </p>
    <p>
        Trees come from <a href="https://crates.io/crates/ast-lang"><code>ast-lang</code></a> — any node type implementing its <code>Node</code> trait, stored in an arena — and findings are <a href="https://crates.io/crates/diag-lang"><code>diag-lang</code></a> diagnostics, so a lint warning and a type error print through the same renderer. lint-lang owns no rules and no grammar of its own. It is the FEAT-tier lint crate of the <code>-lang</code> language-construction family.
    </p>
    <br>
    <hr>
    <p>
        <strong>MSRV is 1.85+</strong> (Rust 2024 edition). <code>no_std</code>-compatible (needs only <code>alloc</code>) and <code>#![forbid(unsafe_code)]</code>.
    </p>
    <blockquote>
        <strong>1.0.0 is the API freeze.</strong> The public surface is stable and follows Semantic Versioning &mdash; no breaking changes before <code>2.0</code>. See <a href="./docs/API.md#stability"><code>docs/API.md</code></a> for the frozen-surface list and the SemVer promise, and <a href="./CHANGELOG.md"><code>CHANGELOG.md</code></a>.
    </blockquote>
</div>

<hr>
<br>

## The model

Five pieces, and the surface fits in your head:

- A **[`Rule`](./docs/API.md#rule)** is one check: a stable `name`, a `default_level`, and a `check` that inspects one node and reports problems.
- A **[`Level`](./docs/API.md#level)** — `Allow`, `Warn`, or `Deny` — is how strictly a rule is applied. `Allow` turns a rule off entirely; it is never called.
- A **[`Linter`](./docs/API.md#linter)** holds the registered rules and their levels, and runs them all over a tree in **one walk**.
- A **[`Context`](./docs/API.md#context)** is what a rule sees at each node: the arena, the node's ancestors and depth, and the `report` call.
- A **[`Finding`](./docs/API.md#finding)** is one reported problem: a `diag-lang` diagnostic tagged with the rule that produced it.

Configuration errors — a duplicate rule name, a level set on a rule that does not exist, a level string that does not parse — are returned as a [`LintError`](./docs/API.md#linterror) at setup time. Running a configured linter never fails.

<hr>
<br>

## Installation

```toml
[dependencies]
lint-lang = "1"
```

Or from the terminal:

```bash
cargo add lint-lang
```

MSRV: Rust 1.85 (Rust 2024 edition).

<hr>
<br>

## Quick start

An expression type, one rule that flags `--x`, and a linter that runs it. The user can escalate the rule to an error or switch it off.

```rust
use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};

enum Expr {
    Int(i64, Span),
    Neg(Id<Expr>, Span),
}

impl Node for Expr {
    fn span(&self) -> Span {
        match self {
            Expr::Int(_, s) | Expr::Neg(_, s) => *s,
        }
    }
    fn each_child(&self, f: &mut dyn FnMut(Id<Self>)) {
        if let Expr::Neg(inner, _) = self {
            f(*inner);
        }
    }
    fn map_children(&self, f: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self {
        match self {
            Expr::Int(v, s) => Expr::Int(*v, *s),
            Expr::Neg(inner, s) => Expr::Neg(f(*inner), *s),
        }
    }
}

struct DoubleNegation;
impl Rule<Expr> for DoubleNegation {
    fn name(&self) -> &'static str {
        "double-negation"
    }
    fn check(&self, node: &Expr, cx: &mut Context<'_, Expr>) {
        if let Expr::Neg(inner, span) = node {
            if let Some(Expr::Neg(..)) = cx.arena().get(*inner) {
                cx.report("negation of a negation", Label::new(*span, "these cancel out"));
            }
        }
    }
}

// `--5`
let mut arena = Arena::new();
let five = arena.alloc(Expr::Int(5, Span::new(2, 3)));
let neg = arena.alloc(Expr::Neg(five, Span::new(1, 3)));
let root = arena.alloc(Expr::Neg(neg, Span::new(0, 3)));

let mut linter = Linter::new();
linter.register(DoubleNegation)?;

let findings = linter.run(&arena, root);
assert_eq!(findings.len(), 1);
assert_eq!(findings[0].to_string(), "warning[double-negation]: negation of a negation");

// Configuration is per rule, by name.
linter.set_level("double-negation", Level::Deny)?;
assert!(linter.run(&arena, root)[0].is_error());
# Ok::<(), lint_lang::LintError>(())
```

<br>

### Rendering findings

Every finding carries a `diag-lang` diagnostic. Register the source in a `SourceMap` and render it for a caret-underlined report:

```rust
use diag_lang::{Renderer, SourceMap};
use lint_lang::{Arena, Context, Id, Label, Linter, Node, Rule, Span};

struct Word(Span);
impl Node for Word {
    fn span(&self) -> Span { self.0 }
    fn each_child(&self, _: &mut dyn FnMut(Id<Self>)) {}
    fn map_children(&self, _: &mut dyn FnMut(Id<Self>) -> Id<Self>) -> Self { Word(self.0) }
}

struct NoTodo;
impl Rule<Word> for NoTodo {
    fn name(&self) -> &'static str { "no-todo" }
    fn check(&self, node: &Word, cx: &mut Context<'_, Word>) {
        cx.report("unfinished work left in source", Label::new(node.span(), "marked here"));
    }
}

let source = "let x = TODO;";
let mut arena = Arena::new();
let root = arena.alloc(Word(Span::new(8, 12)));

let mut linter = Linter::new();
linter.register(NoTodo)?;

let mut map = SourceMap::new();
let _file = map.add("main.src", source).expect("source fits");
let report = Renderer::new().render(linter.run(&arena, root)[0].diagnostic(), &map);
assert!(report.contains("^^^^ marked here"));
# Ok::<(), lint_lang::LintError>(())
```

<br>

### Configuration from text

`Level` parses from `allow` / `warn` / `deny` (ignoring ASCII case), so applying a configuration file or a command-line flag is a loop over `set_level`. Typos are errors, not silent no-ops:

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
linter.register(Named("shadowing"))?;

for line in "unused-value = deny\nshadowing = allow".lines() {
    let (name, level) = line.split_once('=').expect("name = level");
    linter.set_level(name.trim(), level.trim().parse()?)?;
}
assert_eq!(linter.level("unused-value"), Some(Level::Deny));

let typo = linter.set_level("unused-vaule", Level::Warn);
assert!(matches!(typo, Err(LintError::UnknownRule { .. })));
# Ok::<(), LintError>(())
```

With the `serde` feature, `Level` deserializes from the same lowercase strings, so a `BTreeMap<String, Level>` reads straight out of a TOML or JSON configuration file.

<hr>
<br>

## Examples

Three runnable examples ship in [`examples/`](./examples), built on a miniature arithmetic language (parser and four rules in [`examples/common`](./examples/common/mod.rs)):

- **Quick start** — parse an expression, lint it, and render every finding with source context, including a multi-label diagnostic with a note.
  ```bash
  cargo run --example quickstart
  ```
- **Configure** — layer rule defaults, a project configuration file, and `-D`/`-W`/`-A` flags; list the effective configuration; report a misspelled rule and an invalid level.
  ```bash
  cargo run --example configure
  ```
- **Parallel** — lint 2,000 files across every available core with one shared `Linter`, each worker reusing a findings buffer through `run_into`.
  ```bash
  cargo run --example parallel
  ```

<hr>
<br>

## Performance

A run is one iterative depth-first walk no matter how many rules are registered: each node is visited once and handed to every enabled rule in turn. Rules at `Allow` are never called, and a linter with nothing enabled returns without touching the tree. Apart from the walk's own stacks and the findings it produces, a run allocates nothing, and `run_into` lets a caller reuse one findings buffer across a whole batch of files.

Measured with the benchmarks in [`benches/`](./benches) (Linux/WSL2, x86_64, Rust stable, release profile):

| Benchmark | What it measures | Result |
|---|---|---|
| `run/walk_raw` | A bare `ast_lang::walk` that only counts nodes — the traversal floor. | ~1.8–1.9 ns/node |
| `run/noop_rule` | One enabled rule that returns immediately — the framework's own cost. | ~4.7 ns/node |
| `run/example_rules` | The four example rules on a 135k-node tree, building real findings. | ~1.5 ms (~11 ns/node) |
| `rules/noop_x` | 1 → 4 → 16 enabled no-op rules over 17k nodes. | ~1.0–1.2 ns/node per extra rule |
| `run/all_allowed` | Every rule allowed, 135k-node tree. | ~1.3–1.4 ns total (walk skipped) |

Run them yourself:

```bash
cargo bench --bench bench
```

Criterion writes per-benchmark reports to `target/criterion/`. Numbers vary by CPU; compare the trend across runs, not a single absolute.

<hr>
<br>

## Design notes

- **One walk, every rule.** Rules are not passes. The linter walks the tree once and calls each enabled rule at each node, so the cost of a lint run grows with tree size, not with tree size times rule count.
- **Rules are stateless; position comes from the context.** `check` takes `&self`. Everything a rule needs about where it is — parent, ancestors, depth — comes from the `Context`, maintained by the walk at no extra cost. That is what makes a configured linter reusable and shareable.
- **`Send + Sync` by construction.** The `Rule` trait requires `Send + Sync` and `run` takes `&self`, so one linter lints many files in parallel from a shared reference.
- **Deterministic output.** Findings come back in depth-first pre-order, and within a node in rule registration order. The same tree and configuration always produce the same list.
- **Deep trees are safe.** The walk is iterative, so a 200,000-deep tree lints without growing the call stack; a stale root handle produces no findings instead of a panic.
- **No glue types.** Nodes are `ast-lang` nodes and findings carry `diag-lang` diagnostics. Both share one `span-lang`, so a node's `Span` labels a diagnostic with no conversion, and the types are re-exported so a single dependency is enough.

<hr>
<br>

## Testing

The suite runs on Windows, Linux (WSL2 Ubuntu), and macOS through the CI matrix, on stable and the 1.85 MSRV:

```bash
cargo test                       # unit + integration + property + doctests
cargo test --all-features        # adds serde coverage
cargo clippy --all-targets --all-features -- -D warnings
cargo bench --bench bench
```

Property tests in [`tests/proptests.rs`](./tests/proptests.rs) check the runner against an independent model over randomly shaped trees and random configurations: the findings are exactly the enabled rules' matches, in pre-order, with the configured severity and the true node depth. Every `rust` example in this README and in [`docs/API.md`](./docs/API.md) is compiled and run as a doctest, so the published examples cannot drift from the API.

<hr>
<br>

## Cross-platform support

- Linux (x86_64, aarch64)
- macOS (x86_64, Apple Silicon)
- Windows (x86_64)

The crate uses no operating-system facilities and no platform-specific code; behaviour is identical on every target.

<hr>
<br>

## Contributing

See <a href="./dev/DIRECTIVES.md"><code>dev/DIRECTIVES.md</code></a> for engineering standards and the definition of done. Before a PR: `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-features` must be clean.

<br>

<div id="license">
    <h2>License</h2>
    <p>Licensed under either of</p>
    <ul>
        <li><b>Apache License, Version 2.0</b> &mdash; <a href="./LICENSE-APACHE">LICENSE-APACHE</a></li>
        <li><b>MIT License</b> &mdash; <a href="./LICENSE-MIT">LICENSE-MIT</a></li>
    </ul>
    <p>at your option.</p>
</div>

<div align="center">
  <h2></h2>
  <sup>COPYRIGHT <small>&copy;</small> 2026 <strong>James Gober <me@jamesgober.com>.</strong></sup>
</div>
