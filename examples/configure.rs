//! Configure rule levels the way a real tool does: defaults from the rules, then a
//! project configuration file, then command-line flags on top — and report
//! configuration mistakes instead of silently ignoring them.
//!
//! ```bash
//! cargo run --example configure
//! ```

mod common;

use lint_lang::{Level, LintError, Linter};

/// A project's lint configuration: one `rule = level` per line, `#` comments.
const PROJECT_CONFIG: &str = "
# Arithmetic that does nothing is noise in this codebase.
identity-op = warn

# Nesting is reviewed by hand here.
deep-nesting = allow
";

/// Flags as a user would pass them: `-D` deny, `-W` warn, `-A` allow.
const FLAGS: &[&str] = &["-D", "double-negation", "-W", "identity-op"];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut linter = common::linter()?;
    print_levels("defaults", &linter);

    apply_config(&mut linter, PROJECT_CONFIG)?;
    print_levels("after project config", &linter);

    apply_flags(&mut linter, FLAGS)?;
    print_levels("after command-line flags", &linter);

    // A typo in a configuration file is reported, not ignored.
    match apply_config(&mut linter, "identty-op = deny") {
        Err(err) => println!("configuration error: {err}\n"),
        Ok(()) => println!("unexpectedly accepted a misspelled rule\n"),
    }
    match apply_config(&mut linter, "identity-op = loud") {
        Err(err) => println!("configuration error: {err}\n"),
        Ok(()) => println!("unexpectedly accepted an invalid level\n"),
    }

    let program = common::parse("--x * 1 + y / 1")?;
    let findings = linter.run(&program.arena, program.root);
    for finding in &findings {
        println!("{finding}");
    }
    let failed = findings.iter().any(|f| f.is_error());
    println!(
        "\n{} finding(s); build {}",
        findings.len(),
        if failed { "fails" } else { "passes" }
    );
    Ok(())
}

/// Applies `rule = level` lines, skipping blanks and `#` comments.
fn apply_config<N>(linter: &mut Linter<N>, text: &str) -> Result<(), LintError> {
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, level) = line.split_once('=').unwrap_or((line, ""));
        linter.set_level(name.trim(), level.trim().parse()?)?;
    }
    Ok(())
}

/// Applies `-D name` / `-W name` / `-A name` flag pairs.
fn apply_flags<N>(linter: &mut Linter<N>, flags: &[&str]) -> Result<(), LintError> {
    for pair in flags.chunks(2) {
        let [flag, name] = pair else { break };
        let level = match *flag {
            "-D" => Level::Deny,
            "-W" => Level::Warn,
            _ => Level::Allow,
        };
        linter.set_level(name, level)?;
    }
    Ok(())
}

fn print_levels<N>(heading: &str, linter: &Linter<N>) {
    println!("{heading}:");
    for (name, level) in linter.rules() {
        println!("  {name:<18} {level}");
    }
    println!();
}
