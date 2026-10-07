//! Lint a batch of files across threads with one shared linter.
//!
//! A `Linter` is `Send + Sync` and `run` takes `&self`, so it is configured once
//! and borrowed by every worker. Each worker reuses one findings buffer through
//! `run_into`, so linting a file allocates nothing beyond the walk itself.
//!
//! ```bash
//! cargo run --example parallel
//! ```

mod common;

use std::{num::NonZeroUsize, thread};

use lint_lang::{Finding, Level};

/// Totals one worker reports back.
#[derive(Default)]
struct Tally {
    files: usize,
    errors: usize,
    warnings: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A synthetic project: every seventh file divides by zero, every fifth has a
    // double negation, the rest are clean.
    let sources: Vec<String> = (0..2_000)
        .map(|i| match (i % 7, i % 5) {
            (0, _) => format!("a{i} / 0 + b"),
            (_, 0) => format!("--a{i} * b"),
            _ => format!("a{i} * (b + {i})"),
        })
        .collect();
    let programs = sources
        .iter()
        .map(|src| common::parse(src))
        .collect::<Result<Vec<_>, _>>()?;

    let mut linter = common::linter()?;
    linter.set_level("identity-op", Level::Warn)?;
    let linter = &linter;

    let workers = thread::available_parallelism().map_or(4, NonZeroUsize::get);
    let chunk = programs.len().div_ceil(workers);

    let tallies: Vec<Tally> = thread::scope(|scope| -> Result<Vec<Tally>, &'static str> {
        let handles: Vec<_> = programs
            .chunks(chunk)
            .map(|batch| {
                scope.spawn(move || {
                    let mut tally = Tally::default();
                    let mut buffer: Vec<Finding> = Vec::new();
                    for program in batch {
                        buffer.clear();
                        linter.run_into(&program.arena, program.root, &mut buffer);
                        tally.files += 1;
                        for finding in &buffer {
                            if finding.is_error() {
                                tally.errors += 1;
                            } else {
                                tally.warnings += 1;
                            }
                        }
                    }
                    tally
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().map_err(|_| "a lint worker panicked"))
            .collect()
    })?;

    let files: usize = tallies.iter().map(|t| t.files).sum();
    let errors: usize = tallies.iter().map(|t| t.errors).sum();
    let warnings: usize = tallies.iter().map(|t| t.warnings).sum();
    println!("linted {files} files on {} thread(s)", tallies.len());
    println!("{errors} error(s), {warnings} warning(s)");
    Ok(())
}
