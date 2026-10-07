//! The error type returned when a linter is configured incorrectly.

use alloc::boxed::Box;
use core::fmt;

/// An error raised while configuring a [`Linter`](crate::Linter) or parsing a
/// [`Level`](crate::Level).
///
/// Every variant is a *configuration* mistake, caught before any tree is linted:
/// a rule registered twice, a level set on a rule that does not exist, or a level
/// name that does not parse. Running a configured linter never fails — rules
/// report findings, not errors — so this type only appears at setup time, where
/// the caller can surface it to whoever wrote the configuration.
///
/// The enum is `#[non_exhaustive]`: new configuration checks may add variants in
/// a minor release, so a `match` needs a wildcard arm.
///
/// # Examples
///
/// ```
/// use lint_lang::{Level, LintError};
///
/// let err = "loud".parse::<Level>().unwrap_err();
/// assert_eq!(err, LintError::InvalidLevel { input: "loud".into() });
/// assert_eq!(err.to_string(), "invalid lint level `loud` (expected `allow`, `warn`, or `deny`)");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LintError {
    /// A rule was registered under a name another registered rule already uses.
    ///
    /// Rule names are how a linter is configured, so they must be unique. The
    /// second registration is rejected and the linter is left unchanged; rename
    /// one of the rules, or skip registering the duplicate.
    DuplicateRule {
        /// The name both rules claim.
        name: &'static str,
    },
    /// A level was set on a rule name the linter does not know.
    ///
    /// Usually a typo in a configuration file or command-line flag. The linter is
    /// left unchanged; report the name back to the user, ideally alongside the
    /// list from [`Linter::rules`](crate::Linter::rules).
    UnknownRule {
        /// The name that matched no registered rule.
        name: Box<str>,
    },
    /// A string did not name a lint level.
    ///
    /// Returned by `Level`'s [`FromStr`](core::str::FromStr) implementation when
    /// the input is not `allow`, `warn`, or `deny` (compared ignoring ASCII case).
    InvalidLevel {
        /// The input that failed to parse.
        input: Box<str>,
    },
}

impl fmt::Display for LintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateRule { name } => {
                write!(f, "a lint rule named `{name}` is already registered")
            }
            Self::UnknownRule { name } => write!(f, "no lint rule named `{name}` is registered"),
            Self::InvalidLevel { input } => write!(
                f,
                "invalid lint level `{input}` (expected `allow`, `warn`, or `deny`)"
            ),
        }
    }
}

impl core::error::Error for LintError {}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn test_display_duplicate_rule_names_the_rule() {
        let err = LintError::DuplicateRule { name: "no-shadow" };
        assert_eq!(
            err.to_string(),
            "a lint rule named `no-shadow` is already registered"
        );
    }

    #[test]
    fn test_display_unknown_rule_names_the_rule() {
        let err = LintError::UnknownRule {
            name: "no-shadwo".into(),
        };
        assert_eq!(
            err.to_string(),
            "no lint rule named `no-shadwo` is registered"
        );
    }

    #[test]
    fn test_display_invalid_level_lists_accepted_values() {
        let err = LintError::InvalidLevel { input: "".into() };
        assert_eq!(
            err.to_string(),
            "invalid lint level `` (expected `allow`, `warn`, or `deny`)"
        );
    }

    #[test]
    fn test_error_is_core_error() {
        fn assert_error<E: core::error::Error>() {}
        assert_error::<LintError>();
    }
}
