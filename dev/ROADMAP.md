# lint-lang - Roadmap

> Path from scaffold to a stable 1.0. Hard parts are front-loaded; each phase has hard exit criteria.
> Master plan: ../../_strategy/LANG_COLLECTION.md
>
> **Anti-deferral rule:** no listed hard task moves to a later phase unless this file records the move and the reason.

## v0.1.0 - Scaffold (DONE)
Compiles, CI green, structure correct, no domain logic.
- [x] Manifest, README, CHANGELOG, REPS, dual license, CI, deny, clippy, rustfmt.

Scaffold defects found and fixed at the start of v0.2.0: `Cargo.toml` `keywords`
and `categories` were unquoted (the manifest did not parse), `clippy.toml` pinned
MSRV 1.87 instead of 1.85, and the `deny.toml` header named another project. With
those fixed, every CI workflow step passed on the bare scaffold (stable and 1.85)
before any domain code was added.

## v0.2.0 - Core (THE HARD PART, NOT DEFERRED) (DONE)
A lint framework: configurable rules over AST/IR that emit diagnostics.
Dependencies (wires ast, diag, pass) are wired here, when first used.
Exit criteria:
- [x] Every public item has rustdoc + a runnable example.
- [x] Core invariants property-tested (full DIRECTIVES + API authored at this stage).

Delivered 2026-07-08: `Rule<N>`, `Linter<N>` (single fused walk, `Allow` rules
never called, empty-enabled early exit), `Context`, `Level`, `Finding`,
`LintError`. Property tests check the runner against an independent model over
random trees and random configurations.

Dependency wiring, recorded under the anti-deferral rule:

- **ast-lang — wired.** The tree model (`Node`, `Arena`, `Id`) and the iterative
  `walk` the runner drives.
- **diag-lang — wired.** Every finding is a `diag_lang::Diagnostic`. ast-lang and
  diag-lang share one `span-lang` 0.4, so a node's `Span` labels a diagnostic
  directly.
- **pass-lang — dropped, not deferred.** pass-lang sequences whole-program passes
  over a `T`; a lint run is a single fused walk that calls every rule per node, and
  routing rules through a pass manager would turn one traversal into one per rule.
  A driver that wants linting as a pipeline step wraps `Linter::run` in its own
  `Pass` in a few lines; lint-lang gains nothing by depending on it.
- **"IR" — covered by the same API, no separate surface.** Any arena-stored tree
  whose node type implements `ast_lang::Node` can be linted, which includes an IR
  modelled that way. ir-lang's own SSA form has a dedicated validator and is not
  tree-shaped, so no ir-lang adapter is planned.

Later-minor candidates (additive, not 1.0 requirements): a global "treat warnings
as errors" switch, per-rule descriptions for `--explain`, and rule-group
configuration.

## v1.0.0 - API freeze (DONE)
Public surface stable and frozen until 2.0.
- [x] docs/API.md marked stable; SemVer promise recorded.
- [x] Full test + benchmark suite green on all three platforms.

Shipped 2026-07-08. No code changes from 0.2.0; the surface (`Rule`, `Linter`,
`Context`, `Level`, `Finding`, `LintError`, and the `ast-lang`/`diag-lang`
re-exports) is frozen, and the run semantics (finding order, `Allow` rules never
called, level-to-severity mapping), the text and `serde` forms of `Level`, and MSRV
1.85 are recorded as part of the contract in `docs/API.md#stability`. Verified
locally on Linux (WSL2, stable + 1.85) and Windows (stable); macOS is covered by the
CI matrix. `Level` is frozen as an exhaustive three-variant enum and `Rule` keeps
its `Send + Sync` supertraits — both deliberate. Only additive 1.x work remains
(see the later-minor candidates above).
