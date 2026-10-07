//! Criterion benchmarks for the lint runner.
//!
//! - `run/walk_raw` — a bare `ast_lang::walk` that counts nodes: the traversal
//!   floor.
//! - `run/noop_rule` — a linter with one rule that returns immediately: the
//!   framework's own per-node cost on top of the walk.
//! - `run/example_rules` — the four example rules on a tree that triggers some of
//!   them: a realistic run, including building findings.
//! - `run/all_allowed` — every rule allowed: the early exit that skips the walk.
//! - `rules/noop_x` — `n` no-op rules over a fixed tree: the cost of each
//!   additional enabled rule.

#![allow(clippy::expect_used)] // benchmark setup: a failed registration must abort

#[path = "../examples/common/mod.rs"]
mod common;

use std::hint::black_box;

use ast_lang::{Flow, Visitor, walk};
use common::{BinOp, Expr};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use lint_lang::{Arena, Context, Id, Level, Linter, Rule, Span};

/// Builds a balanced expression tree with `leaves` integer leaves (about
/// `2 * leaves` nodes). Every 16th leaf is `0`, so some divisions divide by zero,
/// and every 32nd operator is wrapped in a double negation: the example rules fire
/// on a small fraction of nodes, as they would on real code.
fn tree(leaves: usize) -> (Arena<Expr>, Id<Expr>) {
    let mut arena = Arena::with_capacity(leaves * 3);
    let span = Span::new(0, 1);
    let mut level: Vec<Id<Expr>> = (0..leaves)
        .map(|i| {
            let value = if i % 16 == 15 { 0 } else { i as i64 + 2 };
            arena.alloc(Expr::Int(value, span))
        })
        .collect();
    let mut counter = 0usize;
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let node = match *pair {
                [lhs, rhs] => {
                    counter += 1;
                    let op = [BinOp::Add, BinOp::Mul, BinOp::Sub, BinOp::Div][counter % 4];
                    let bin = arena.alloc(Expr::Binary(op, lhs, rhs, span));
                    if counter % 32 == 0 {
                        let inner = arena.alloc(Expr::Neg(bin, span));
                        arena.alloc(Expr::Neg(inner, span))
                    } else {
                        bin
                    }
                }
                [single] => single,
                _ => continue,
            };
            next.push(node);
        }
        level = next;
    }
    (arena, level[0])
}

/// The traversal floor: a visitor that only counts.
struct Count(usize);
impl Visitor<Expr> for Count {
    fn enter(&mut self, _: &Arena<Expr>, _: Id<Expr>, _: &Expr) -> Flow {
        self.0 += 1;
        Flow::Continue
    }
}

/// A rule that inspects nothing; `NAMES` gives each instance a unique name.
struct Noop(usize);
const NAMES: [&str; 16] = [
    "n0", "n1", "n2", "n3", "n4", "n5", "n6", "n7", "n8", "n9", "n10", "n11", "n12", "n13", "n14",
    "n15",
];
impl Rule<Expr> for Noop {
    fn name(&self) -> &'static str {
        NAMES[self.0]
    }
    fn check(&self, node: &Expr, _: &mut Context<'_, Expr>) {
        let _ = black_box(node);
    }
}

fn noop_linter(rules: usize) -> Linter<Expr> {
    let mut linter = Linter::new();
    for i in 0..rules {
        linter.register(Noop(i)).expect("unique names");
    }
    linter
}

fn example_linter() -> Linter<Expr> {
    let mut linter = common::linter().expect("example rules register");
    linter
        .set_level("identity-op", Level::Warn)
        .expect("rule exists");
    linter
}

fn bench_run(c: &mut Criterion) {
    let mut group = c.benchmark_group("run");
    for leaves in [512usize, 8_192, 65_536] {
        let (arena, root) = tree(leaves);
        let nodes = arena.len();
        group.throughput(Throughput::Elements(nodes as u64));

        group.bench_with_input(BenchmarkId::new("walk_raw", nodes), &(), |b, ()| {
            b.iter(|| {
                let mut count = Count(0);
                walk(black_box(&arena), root, &mut count);
                black_box(count.0)
            });
        });

        let noop = noop_linter(1);
        group.bench_with_input(BenchmarkId::new("noop_rule", nodes), &(), |b, ()| {
            let mut buffer = Vec::new();
            b.iter(|| {
                buffer.clear();
                noop.run_into(black_box(&arena), root, &mut buffer);
                black_box(buffer.len())
            });
        });

        let examples = example_linter();
        group.bench_with_input(BenchmarkId::new("example_rules", nodes), &(), |b, ()| {
            let mut buffer = Vec::new();
            b.iter(|| {
                buffer.clear();
                examples.run_into(black_box(&arena), root, &mut buffer);
                black_box(buffer.len())
            });
        });
    }

    let (arena, root) = tree(65_536);
    let mut allowed = example_linter();
    for name in [
        "division-by-zero",
        "double-negation",
        "identity-op",
        "deep-nesting",
    ] {
        allowed.set_level(name, Level::Allow).expect("rule exists");
    }
    group.throughput(Throughput::Elements(arena.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("all_allowed", arena.len()),
        &(),
        |b, ()| {
            b.iter(|| black_box(allowed.run(black_box(&arena), root)));
        },
    );
    group.finish();
}

fn bench_rule_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("rules");
    let (arena, root) = tree(8_192);
    group.throughput(Throughput::Elements(arena.len() as u64));
    for rules in [1usize, 4, 16] {
        let linter = noop_linter(rules);
        group.bench_with_input(BenchmarkId::new("noop_x", rules), &(), |b, ()| {
            let mut buffer = Vec::new();
            b.iter(|| {
                buffer.clear();
                linter.run_into(black_box(&arena), root, &mut buffer);
                black_box(buffer.len())
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_run, bench_rule_scaling);
criterion_main!(benches);
