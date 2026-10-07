//! Parse a small expression, lint it with the built-in example rules, and render
//! every finding with source context through `diag-lang`.
//!
//! ```bash
//! cargo run --example quickstart
//! ```

mod common;

use diag_lang::{Renderer, SourceMap};
use lint_lang::Finding;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "total / 0 + --count * (rate - (1 + (2 * (3 - (4 / 5)))))";

    let program = common::parse(source)?;
    let linter = common::linter()?;
    let findings = linter.run(&program.arena, program.root);

    // Findings are ordinary `diag-lang` diagnostics: register the source and render.
    let mut map = SourceMap::new();
    let _file = map.add("calc.expr", source)?;
    let renderer = Renderer::new();

    for finding in &findings {
        println!("[{}]", finding.rule());
        println!("{}", renderer.render(finding.diagnostic(), &map));
    }

    let errors = findings.iter().filter(|f| f.is_error()).count();
    println!(
        "{} finding(s): {} error(s), {} warning(s)",
        findings.len(),
        errors,
        findings.len() - errors
    );
    if findings.iter().any(Finding::is_error) {
        println!("lint failed: a `deny` rule fired");
    }
    Ok(())
}
