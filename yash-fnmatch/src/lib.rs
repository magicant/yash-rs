// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! This crate provides the `fnmatch` function that performs pattern matching
//! based on a globbing pattern.
//!
//! This implementation supports the following syntax in patterns:
//!
//! - Any single character (`?`)
//! - Any character sequence (`*`)
//! - Bracket expression (`[...]`)
//!     - Character literals
//!     - Character ranges (e.g. `a-z`)
//!     - Complement (`[!...]`)
//!     - Collating symbols (e.g. `[.ch.]`)
//!     - Equivalence classes (e.g. `[=a=]`)
//!     - Character classes (`[:alpha:]`)
//!
//! The current implementation does not support any locale-specific
//! characteristics. Especially, collating symbols and equivalent classes only
//! match the specified character sequence itself, and character classes only
//! match ASCII characters.
//!
//! This crate is very similar to the [`fnmatch-regex`] crate in that the both
//! perform matching by converting the pattern to a regular expression. The
//! `yash-fnmatch` crate tries to support the POSIX specification as much as
//! possible rather than introducing unique (non-portable) functionalities.
//!
//! [`fnmatch-regex`]: https://crates.io/crates/fnmatch-regex

mod char_iter;
pub use char_iter::*;

/// Configuration for a pattern
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Config {
    /// Whether the pattern matches only at the beginning of text
    ///
    /// For example, the pattern `in` matches the text `begin` iff
    /// `anchor_begin` is `false`.
    anchor_begin: bool,

    /// Whether the pattern matches only at the end of text
    ///
    /// For example, the pattern `mat` matches the text `match` iff `anchor_end`
    /// is `false`.
    anchor_end: bool,

    /// Whether a leading period has to be matched explicitly
    ///
    /// When `match_period` is `true`, a leading period in the text, if any,
    /// must be matched by a literal period in the pattern. In other words, a
    /// wildcard pattern (`*` or `?`) or bracket expression (`[...]`) does not
    /// match a leading period. For example, the pattern `*.txt` does not match
    /// the filename `.foo.txt`.
    ///
    /// When `match_period` is `false`, the above restriction does not apply.
    match_period: bool,

    /// Whether the pattern matches shortest part of text
    ///
    /// When matching the pattern `a*a` against the text `banana`, for example,
    /// the shortest match will be `ana` while the longest `anana`.
    shortest_match: bool,

    /// Whether the pattern should match case-insensitively
    case_insensitive: bool,
}

/// Error that may happen in building a pattern.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Error in underlying regular expression processing
    RegexError(regex::Error),
}

/// Main part of compiled pattern
#[derive(Clone, Debug)]
enum Body {
    /// Literal string pattern
    Literal(String),
}

/// Compiled globbing pattern
#[derive(Clone, Debug)]
#[must_use = "creating a pattern without doing pattern matching is nonsense"]
pub struct Pattern {
    body: Body,
    config: Config,
}

// TODO impl Display for Pattern
// TODO impl FromStr for Pattern

impl Pattern {
    /// Creates a pattern with defaulted configuration.
    #[inline]
    pub fn new<I>(pattern: I) -> Self
    where
        I: IntoIterator<Item = (char, CharKind)>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        Self::with_config(pattern, Config::default())
    }

    /// Creates a pattern with a specified configuration.
    pub fn with_config<I>(pattern: I, config: Config) -> Self
    where
        I: IntoIterator<Item = (char, CharKind)>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        let s = pattern.into_iter().map(|(c, _)| c).collect();
        let body = Body::Literal(s);
        Pattern { body, config }
    }

    /// Returns the configuration for this pattern.
    #[inline]
    #[must_use]
    pub fn config(&self) -> Config {
        self.config
    }

    /// Tests whether this pattern matches the given text.
    #[must_use]
    pub fn is_match(&self, text: &str) -> bool {
        match &self.body {
            Body::Literal(s) => text.contains(s),
        }
    }
}

/*
/// Pattern builder
#[derive(Clone, Debug, Default)]
pub struct Builder {}

impl Builder {
    /// Creates a new builder.
    #[must_use]
    pub fn new() -> Self {
        Builder::default()
    }

    /// Build a new pattern.
    pub fn build(&self) -> Result<Pattern, Error> {
        todo!()
    }

    /// Sets whether the pattern matches only at the beginning.
    pub fn anchor_begin(&mut self, yes: bool) -> &mut Builder {
        todo!()
    }

    /// Sets whether the pattern matches only at the end.
    pub fn anchor_end(&mut self, yes: bool) -> &mut Builder {
        todo!()
    }

    /// Sets whether a leading period has to be matched explicitly.
    pub fn match_period(&mut self, yes: bool) -> &mut Builder {
        todo!()
    }

    /// Sets whether the pattern should match case-insensitively.
    pub fn case_insensitive(&mut self, yes: bool) -> &mut Builder {
        todo!()
    }
}
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pattern() {
        let p = Pattern::new(without_escape(""));
        assert!(p.is_match(""));
        assert!(p.is_match("a"));
        assert!(p.is_match("."));
        assert!(p.is_match("*"));
    }

    #[test]
    fn single_character_pattern() {
        let p = Pattern::new(without_escape("a"));
        assert!(!p.is_match(""));
        assert!(p.is_match("a"));
        assert!(p.is_match("aa"));
        assert!(!p.is_match("b"));
        assert!(p.is_match("ab"));
        assert!(p.is_match("ba"));
    }

    #[test]
    fn double_character_pattern() {
        let p = Pattern::new(without_escape("in"));
        assert!(!p.is_match(""));
        assert!(!p.is_match("i"));
        assert!(!p.is_match("n"));
        assert!(p.is_match("bin"));
        assert!(p.is_match("inn"));
        assert!(!p.is_match("nit"));
    }
}
