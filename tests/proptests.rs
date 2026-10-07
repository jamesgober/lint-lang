//! Property tests: the linter's core invariants over randomly shaped trees and
//! random rule configurations.

#![allow(clippy::unwrap_used)] // test setup: a failed registration must fail loudly

use lint_lang::{Arena, Context, Id, Label, Level, Linter, Node, Rule, Span};
use proptest::prelude::*;

/// An n-ary tree node carrying a small tag.
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

/// Reports every node whose tag equals the rule's tag. Rule `i` matches tag `i`.
struct MatchTag(u8);

const NAMES: [&str; 4] = ["tag-0", "tag-1", "tag-2", "tag-3"];

impl Rule<T> for MatchTag {
    fn name(&self) -> &'static str {
        NAMES[usize::from(self.0)]
    }
    fn check(&self, node: &T, cx: &mut Context<'_, T>) {
        if node.tag == self.0 {
            // Encode the node's depth in the message so ordering can be checked.
            cx.report(cx.depth().to_string(), Label::unlabelled(node.span));
        }
    }
}

/// Builds a tree from a parent list: node `i > 0` hangs under `parents[i] % i`,
/// so every list is a valid tree rooted at node 0. Returns the arena, the root,
/// and each node's (tag, depth) in pre-order.
fn build(tags: &[u8], parents: &[usize]) -> (Arena<T>, Id<T>, Vec<(u8, usize)>) {
    let n = tags.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 1..n {
        children[parents[i] % i].push(i);
    }
    // Allocate bottom-up (children before parents) so handles exist when needed.
    let mut ids: Vec<Option<Id<T>>> = vec![None; n];
    let mut arena = Arena::new();
    for i in (0..n).rev() {
        let kids = children[i].iter().map(|&c| ids[c].unwrap()).collect();
        ids[i] = Some(arena.alloc(T {
            tag: tags[i],
            kids,
            span: Span::new(i as u32, i as u32 + 1),
        }));
    }
    // Expected pre-order of (tag, depth), computed independently of the walk.
    let mut order = Vec::with_capacity(n);
    let mut stack = vec![(0usize, 0usize)];
    while let Some((node, depth)) = stack.pop() {
        order.push((tags[node], depth));
        for &c in children[node].iter().rev() {
            stack.push((c, depth + 1));
        }
    }
    (arena, ids[0].unwrap(), order)
}

fn tree() -> impl Strategy<Value = (Vec<u8>, Vec<usize>)> {
    (1usize..120).prop_flat_map(|n| {
        (
            prop::collection::vec(0u8..4, n),
            prop::collection::vec(any::<usize>(), n),
        )
    })
}

fn levels() -> impl Strategy<Value = Vec<Level>> {
    prop::collection::vec(
        prop_oneof![Just(Level::Allow), Just(Level::Warn), Just(Level::Deny)],
        4,
    )
}

fn configured(levels: &[Level]) -> Linter<T> {
    let mut linter = Linter::new();
    for (i, level) in levels.iter().enumerate() {
        linter.register(MatchTag(i as u8)).unwrap();
        linter.set_level(NAMES[i], *level).unwrap();
    }
    linter
}

proptest! {
    /// The findings are exactly: for each node in pre-order, for each enabled rule
    /// in registration order whose tag matches — with the rule's severity and the
    /// node's true depth.
    #[test]
    fn findings_match_an_independent_model((tags, parents) in tree(), levels in levels()) {
        let (arena, root, order) = build(&tags, &parents);
        let linter = configured(&levels);

        let expected: Vec<_> = order
            .iter()
            .flat_map(|&(tag, depth)| {
                levels.iter().enumerate().filter_map(move |(i, level)| {
                    (i as u8 == tag)
                        .then(|| level.severity().map(|s| (NAMES[i], s, depth.to_string())))
                        .flatten()
                })
            })
            .collect();
        let actual: Vec<_> = linter
            .run(&arena, root)
            .into_iter()
            .map(|f| (f.rule(), f.severity(), f.diagnostic().message().to_owned()))
            .collect();
        prop_assert_eq!(actual, expected);
    }

    /// With every rule allowed, nothing is ever reported.
    #[test]
    fn all_allowed_reports_nothing((tags, parents) in tree()) {
        let (arena, root, _) = build(&tags, &parents);
        let linter = configured(&[Level::Allow; 4]);
        prop_assert!(linter.run(&arena, root).is_empty());
    }

    /// Changing only `Warn` ↔ `Deny` changes severities, never which findings exist.
    #[test]
    fn warn_and_deny_report_the_same_findings((tags, parents) in tree()) {
        let (arena, root, _) = build(&tags, &parents);
        let warn = configured(&[Level::Warn; 4]).run(&arena, root);
        let deny = configured(&[Level::Deny; 4]).run(&arena, root);
        prop_assert_eq!(warn.len(), deny.len());
        for (w, d) in warn.iter().zip(&deny) {
            prop_assert_eq!(w.rule(), d.rule());
            prop_assert_eq!(w.diagnostic().primary(), d.diagnostic().primary());
            prop_assert!(!w.is_error() && d.is_error());
        }
    }

    /// `run_into` appends exactly what `run` returns, after existing content.
    #[test]
    fn run_into_appends_run((tags, parents) in tree(), levels in levels()) {
        let (arena, root, _) = build(&tags, &parents);
        let linter = configured(&levels);
        let fresh = linter.run(&arena, root);
        let mut buffer = linter.run(&arena, root);
        linter.run_into(&arena, root, &mut buffer);
        prop_assert_eq!(&buffer[..fresh.len()], &fresh[..]);
        prop_assert_eq!(&buffer[fresh.len()..], &fresh[..]);
    }
}
