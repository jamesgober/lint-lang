<h1 align="center">
    <img width="90px" height="auto" src="https://raw.githubusercontent.com/jamesgober/jamesgober/main/media/icons/hexagon-3.svg" alt="Triple Hexagon">
    <br><b>CHANGELOG</b>
</h1>
<p>
  All notable changes to <code>lint-lang</code> will be documented in this file. The format is based on <a href="https://keepachangelog.com/en/1.1.0/">Keep a Changelog</a>,
  and this project adheres to <a href="https://semver.org/spec/v2.0.0.html/">Semantic Versioning</a>.
</p>

---

## [Unreleased]

### Added

### Changed

### Fixed

### Security

---

## [1.0.0] - 2026-07-08

API freeze. The public surface introduced in 0.2.0 is now stable and frozen under
Semantic Versioning: no breaking changes ship before `2.0`. There are no code
changes from 0.2.0 — this release fixes the contract and completes the roadmap.

### Changed

- Bumped the crate version to `1.0.0` and declared the public API stable.
- `docs/API.md` marked stable, with a new **Stability** section recording the
  frozen surface, the behaviour that is part of the contract (finding order,
  `Allow` rules never called, level-to-severity mapping, unchanged linter on a
  configuration error, text and `serde` forms of `Level`), what may change in a
  minor release, and the dependency and MSRV (1.85) policy.
- `README.md` and the crate-level docs point at the stability promise; install
  snippets now use `lint-lang = "1"`.

---

## [0.2.0] - 2026-07-08

The core, and the hard part of the roadmap: the scaffold becomes a working lint
framework. Rules are written once against a small trait, configured per rule with
`allow` / `warn` / `deny`, and run together over an `ast-lang` tree in a single
walk; every finding is a `diag-lang` diagnostic that renders with source context.
The surface is deliberately small: one trait, four types, and one error.

### Added

- `Rule<N>` — the trait a lint check implements: a stable `name`, a
  `default_level` (defaults to `Warn`), and `check(&self, node, cx)`. Requires
  `Send + Sync` so a configured linter can be shared across threads.
- `Linter<N>` — the rule registry and runner. `new`, `register` (rejects duplicate
  names), `set_level` (rejects unknown names), `level`, `rules`, `len`,
  `is_empty`, `run`, and `run_into` (appends to a reusable buffer). A run is one
  iterative depth-first walk whatever the number of rules; `Allow` rules are never
  called and a linter with nothing enabled skips the walk entirely.
- `Context<'a, N>` — a rule's view of the current node: `arena`, `id`, `parent`,
  `ancestors` (root first), `depth`, `rule`, `severity`, plus `report` (message +
  label at the configured severity) and `emit` (a fully built diagnostic).
- `Level` — `Allow` / `Warn` / `Deny`, ordered by strictness, with `severity`,
  `as_str`, `Display`, and case-insensitive `FromStr`.
- `Finding` — one reported problem: `rule`, `diagnostic`, `severity`, `is_error`,
  `into_diagnostic`, and a one-line `Display` (`warning[rule]: message`).
- `LintError` — `#[non_exhaustive]` configuration error: `DuplicateRule`,
  `UnknownRule`, `InvalidLevel`; implements `core::error::Error`.
- Re-exports of `Arena`, `Id`, `Node`, `Span` (from `ast-lang`) and `Diagnostic`,
  `Label`, `Severity` (from `diag-lang`), so one dependency is enough to write and
  run rules.
- `serde` feature: `Serialize` / `Deserialize` for `Level` as `"allow"`, `"warn"`,
  `"deny"`.
- Three runnable examples (`quickstart`, `configure`, `parallel`) built on a
  shared miniature arithmetic language with four rules; Criterion benchmarks for
  the walk floor, framework overhead, per-rule scaling, and the all-allowed early
  exit; integration tests; property tests checking the runner against an
  independent model; and a full `docs/API.md`. Every `rust` block in `README.md`
  and `docs/API.md` runs as a doctest.

### Changed

- Bumped the crate version to `0.2.0`.
- Wired `ast-lang` 1.x (tree model and walk) and `diag-lang` 1.x (finding
  format). Both resolve to one `span-lang` 0.4, so a node's `Span` labels a
  diagnostic with no conversion. `pass-lang` is deliberately not wired; see
  `dev/ROADMAP.md`.
- The `std` feature now forwards to `ast-lang/std` and `diag-lang/std`.
- Crate root enables the full REPS lint set (`unused_results`, `clippy::unwrap_used`,
  `clippy::expect_used`, `clippy::panic`, and the rest).
- Narrowed the crate description to what the crate does: rules over an AST.

### Fixed

- Corrected invalid TOML in `Cargo.toml` (`keywords` and `categories` were
  unquoted) that prevented the manifest from parsing, and added a fifth keyword.
- Aligned the `clippy.toml` MSRV (`1.87` → `1.85`) with `Cargo.toml`.
- Corrected the stale project name in the `deny.toml` header comment.

---

## [0.1.0] - 2026-06-18

Initial scaffold and repository bootstrap. No domain logic yet &mdash; this release establishes the structure, tooling, and quality gates the implementation will be built on.

### Added

- `Cargo.toml` with crate metadata, Rust 2024 edition, MSRV 1.85.
- Dual `Apache-2.0 OR MIT` license files.
- `README.md`, `CHANGELOG.md`, and a documentation skeleton.
- `REPS.md` compliance baseline.
- `.github/workflows/ci.yml` CI matrix; `deny.toml`, `clippy.toml`, `rustfmt.toml`.
- `dev/DIRECTIVES.md` and `dev/ROADMAP.md` (committed engineering standards + plan).

[Unreleased]: https://github.com/jamesgober/lint-lang/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/jamesgober/lint-lang/compare/v0.2.0...v1.0.0
[0.2.0]: https://github.com/jamesgober/lint-lang/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jamesgober/lint-lang/releases/tag/v0.1.0
