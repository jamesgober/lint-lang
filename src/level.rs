//! How seriously a rule's findings are taken.

use core::{fmt, str::FromStr};

use diag_lang::Severity;

use crate::LintError;

/// The level a rule runs at: off, reported as a warning, or reported as an error.
///
/// This is the knob a linter is configured with. Every [`Rule`](crate::Rule)
/// declares a [default level](crate::Rule::default_level), and a user can override
/// it per rule with [`Linter::set_level`](crate::Linter::set_level) — the same
/// `allow` / `warn` / `deny` model as `rustc` and Clippy.
///
/// - [`Allow`](Level::Allow) turns the rule off. It is not just silenced: an
///   allowed rule is never called, so it costs nothing at run time.
/// - [`Warn`](Level::Warn) reports findings as [`Severity::Warning`].
/// - [`Deny`](Level::Deny) reports findings as [`Severity::Error`], which a driver
///   typically treats as a failed build.
///
/// Levels are ordered by strictness, `Allow < Warn < Deny`, so `max` picks the
/// stricter of two levels.
///
/// # Examples
///
/// ```
/// use lint_lang::{Level, Severity};
///
/// assert_eq!(Level::Allow.severity(), None);
/// assert_eq!(Level::Warn.severity(), Some(Severity::Warning));
/// assert_eq!(Level::Deny.severity(), Some(Severity::Error));
///
/// // Parse a level from a flag or a configuration value.
/// let level: Level = "deny".parse()?;
/// assert_eq!(level, Level::Deny);
/// assert_eq!(Level::Warn.max(Level::Deny), Level::Deny);
/// # Ok::<(), lint_lang::LintError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum Level {
    /// The rule is disabled and never runs.
    Allow,
    /// The rule runs and reports its findings as warnings.
    Warn,
    /// The rule runs and reports its findings as errors.
    Deny,
}

impl Level {
    /// Returns the diagnostic severity findings at this level carry, or `None` for
    /// [`Allow`](Level::Allow), which produces no findings.
    ///
    /// # Examples
    ///
    /// ```
    /// use lint_lang::{Level, Severity};
    ///
    /// assert_eq!(Level::Deny.severity(), Some(Severity::Error));
    /// assert!(Level::Allow.severity().is_none());
    /// ```
    #[inline]
    #[must_use]
    pub const fn severity(self) -> Option<Severity> {
        match self {
            Self::Allow => None,
            Self::Warn => Some(Severity::Warning),
            Self::Deny => Some(Severity::Error),
        }
    }

    /// Returns the lowercase name of the level: `"allow"`, `"warn"`, or `"deny"`.
    ///
    /// This is the spelling [`FromStr`] accepts and the `serde` feature writes, so
    /// a level round-trips through text unchanged.
    ///
    /// # Examples
    ///
    /// ```
    /// use lint_lang::Level;
    ///
    /// assert_eq!(Level::Warn.as_str(), "warn");
    /// assert_eq!(Level::Warn.as_str().parse(), Ok(Level::Warn));
    /// ```
    #[inline]
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Warn => "warn",
            Self::Deny => "deny",
        }
    }
}

impl fmt::Display for Level {
    /// Formats as the lowercase name from [`as_str`](Level::as_str).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Level {
    type Err = LintError;

    /// Parses `allow`, `warn`, or `deny`, ignoring ASCII case.
    ///
    /// The input is matched exactly apart from case — surrounding whitespace is
    /// not trimmed — so a configuration reader should trim its values first.
    /// Anything else is a [`LintError::InvalidLevel`] carrying the input.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        [Self::Allow, Self::Warn, Self::Deny]
            .into_iter()
            .find(|level| level.as_str().eq_ignore_ascii_case(s))
            .ok_or_else(|| LintError::InvalidLevel { input: s.into() })
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use super::*;

    #[test]
    fn test_severity_maps_each_level() {
        assert_eq!(Level::Allow.severity(), None);
        assert_eq!(Level::Warn.severity(), Some(Severity::Warning));
        assert_eq!(Level::Deny.severity(), Some(Severity::Error));
    }

    #[test]
    fn test_order_is_by_strictness() {
        assert!(Level::Allow < Level::Warn);
        assert!(Level::Warn < Level::Deny);
        assert_eq!(Level::Allow.max(Level::Deny), Level::Deny);
    }

    #[test]
    fn test_display_matches_as_str() {
        for level in [Level::Allow, Level::Warn, Level::Deny] {
            assert_eq!(level.to_string(), level.as_str());
        }
    }

    #[test]
    fn test_from_str_round_trips_every_level() {
        for level in [Level::Allow, Level::Warn, Level::Deny] {
            assert_eq!(level.as_str().parse::<Level>(), Ok(level));
        }
    }

    #[test]
    fn test_from_str_ignores_ascii_case() {
        assert_eq!("DENY".parse::<Level>(), Ok(Level::Deny));
        assert_eq!("Warn".parse::<Level>(), Ok(Level::Warn));
    }

    #[test]
    fn test_from_str_unknown_returns_invalid_level() {
        assert_eq!(
            "warning".parse::<Level>(),
            Err(LintError::InvalidLevel {
                input: "warning".into()
            })
        );
    }

    #[test]
    fn test_from_str_does_not_trim_whitespace() {
        assert!(" warn".parse::<Level>().is_err());
        assert!("".parse::<Level>().is_err());
    }
}
