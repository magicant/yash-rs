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
use regex::bytes::Regex;
use std::ops::Range;

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

impl From<regex::Error> for Error {
    fn from(error: regex::Error) -> Self {
        Error::RegexError(error)
    }
}

/// Main part of compiled pattern
#[derive(Clone, Debug)]
enum Body {
    /// Literal string pattern
    Literal(String),
    /// Compiled regular expression
    Regex(Regex),
}

/// Converts a globbing pattern to a regular expression.
///
/// The result is appended to `result`.
fn to_regex<I>(pattern: I, result: &mut String)
where
    I: Iterator<Item = (char, CharKind)> + Clone,
{
    // TODO multiline option
    for (c, _) in pattern {
        match c {
            '?' => result.push('.'),
            '*' => result.push_str(".*"),
            // TODO bracket expression
            _ => result.push(c),
        }
    }
}

impl Body {
    fn new<I>(pattern: I, _config: Config) -> Result<Self, Error>
    where
        I: Iterator<Item = (char, CharKind)> + Clone,
    {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        enum Bracket {
            None,
            Open,
            Closed,
        }

        let mut chars = String::new();
        chars.reserve(pattern.size_hint().0);
        let mut bracket = Bracket::None;
        for (c, _) in pattern.clone() {
            match c {
                '?' | '*' => {
                    chars.clear();
                    to_regex(pattern, &mut chars);
                    return Ok(Body::Regex(Regex::new(&chars)?));
                }
                '[' => {
                    bracket = Bracket::Open;
                    chars.push(c);
                }
                ']' if bracket == Bracket::Open => {
                    bracket = Bracket::Closed;
                    chars.push(c);
                }
                _ => chars.push(c),
            }
        }
        if bracket == Bracket::Closed {
            chars.clear();
            to_regex(pattern, &mut chars);
            return Ok(Body::Regex(Regex::new(&chars)?));
        }
        Ok(Body::Literal(chars))
    }
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
    pub fn new<I>(pattern: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = (char, CharKind)>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        Self::with_config(pattern, Config::default())
    }

    /// Creates a pattern with a specified configuration.
    pub fn with_config<I>(pattern: I, config: Config) -> Result<Self, Error>
    where
        I: IntoIterator<Item = (char, CharKind)>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        let body = Body::new(pattern.into_iter(), config)?;
        Ok(Pattern { body, config })
    }

    /// Returns the configuration for this pattern.
    #[inline]
    #[must_use]
    pub fn config(&self) -> Config {
        self.config
    }

    /// Returns the only string that matches the pattern, if any.
    ///
    /// If the pattern is made up only of literal characters, this function
    /// returns the characters as a string. If the pattern contains any `?`,
    /// `*`, or bracket expression, the result is `None`.
    #[must_use]
    pub fn as_literal(&self) -> Option<&str> {
        match &self.body {
            Body::Literal(s) => Some(s),
            Body::Regex(_) => None,
        }
    }

    /// Tests whether this pattern matches the given text.
    #[must_use]
    pub fn is_match(&self, text: &str) -> bool {
        match &self.body {
            Body::Literal(s) => text.contains(s),
            Body::Regex(regex) => regex.is_match(text.as_bytes()),
        }
    }

    #[must_use]
    pub fn find(&self, text: &str) -> Option<Range<usize>> {
        match &self.body {
            Body::Literal(s) => text.find(s).map(|pos| pos..pos + s.len()),
            Body::Regex(regex) => regex.find(text.as_bytes()).map(|m| m.range()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pattern() {
        let p = Pattern::new(without_escape("")).unwrap();
        assert_eq!(p.as_literal(), Some(""));

        assert!(p.is_match(""));
        assert!(p.is_match("a"));
        assert!(p.is_match("."));
        assert!(p.is_match("*"));

        assert_eq!(p.find(""), Some(0..0));
        assert_eq!(p.find("a"), Some(0..0));
        assert_eq!(p.find("."), Some(0..0));
        assert_eq!(p.find("*"), Some(0..0));
    }

    #[test]
    fn single_character_pattern() {
        let p = Pattern::new(without_escape("a")).unwrap();
        assert_eq!(p.as_literal(), Some("a"));

        assert!(!p.is_match(""));
        assert!(p.is_match("a"));
        assert!(p.is_match("aa"));
        assert!(!p.is_match("b"));
        assert!(p.is_match("ab"));
        assert!(p.is_match("ba"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("aa"), Some(0..1));
        assert_eq!(p.find("b"), None);
        assert_eq!(p.find("ab"), Some(0..1));
        assert_eq!(p.find("ba"), Some(1..2));
    }

    #[test]
    fn double_character_pattern() {
        let p = Pattern::new(without_escape("in")).unwrap();
        assert_eq!(p.as_literal(), Some("in"));

        assert!(!p.is_match(""));
        assert!(!p.is_match("i"));
        assert!(!p.is_match("n"));
        assert!(p.is_match("bin"));
        assert!(p.is_match("inn"));
        assert!(!p.is_match("nit"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("i"), None);
        assert_eq!(p.find("n"), None);
        assert_eq!(p.find("bin"), Some(1..3));
        assert_eq!(p.find("inn"), Some(0..2));
        assert_eq!(p.find("nit"), None);
    }

    #[test]
    fn any_single_character_pattern() {
        let p = Pattern::new(without_escape("?")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert!(!p.is_match(""));
        assert!(p.is_match("i"));
        assert!(p.is_match("yes"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("i"), Some(0..1));
        assert_eq!(p.find("yes"), Some(0..1));
    }

    #[test]
    fn any_single_character_pattern_combined() {
        let p = Pattern::new(without_escape("a?c")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert!(!p.is_match(""));
        assert!(!p.is_match("ab"));
        assert!(!p.is_match("ac"));
        assert!(!p.is_match("bc"));
        assert!(p.is_match("abc"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("ab"), None);
        assert_eq!(p.find("ac"), None);
        assert_eq!(p.find("bc"), None);
        assert_eq!(p.find("abc"), Some(0..3));
    }

    #[test]
    fn any_multi_character_pattern() {
        let p = Pattern::new(without_escape("*")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert!(p.is_match(""));
        assert!(p.is_match("i"));
        assert!(p.is_match("yes"));

        assert_eq!(p.find(""), Some(0..0));
        assert_eq!(p.find("i"), Some(0..1));
        assert_eq!(p.find("yes"), Some(0..3));
    }

    #[test]
    fn any_multi_character_pattern_combined() {
        let p = Pattern::new(without_escape("a*b")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert!(!p.is_match(""));
        assert!(!p.is_match("a"));
        assert!(p.is_match("ab"));
        assert!(p.is_match("aabb"));
        assert!(p.is_match("lambda"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("ab"), Some(0..2));
        assert_eq!(p.find("aabb"), Some(0..4));
        assert_eq!(p.find("lambda"), Some(1..4));
    }

    #[test]
    fn unmatched_bracket_1() {
        let p = Pattern::new(without_escape("[a")).unwrap();
        assert_eq!(p.as_literal(), Some("[a"));

        assert!(!p.is_match(""));
        assert!(!p.is_match("a"));
        assert!(p.is_match("[a]"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("[a]"), Some(0..2));
    }

    #[test]
    fn unmatched_bracket_2() {
        let p = Pattern::new(without_escape("a]")).unwrap();
        assert_eq!(p.as_literal(), Some("a]"));

        assert!(!p.is_match(""));
        assert!(!p.is_match("a"));
        assert!(p.is_match("[a]"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("[a]"), Some(1..3));
    }

    #[test]
    fn unmatched_bracket_3() {
        let p = Pattern::new(without_escape("][")).unwrap();
        assert_eq!(p.as_literal(), Some("]["));

        assert!(!p.is_match(""));
        assert!(p.is_match("]["));
        assert!(p.is_match("[][]"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("]["), Some(0..2));
        assert_eq!(p.find("[][]"), Some(1..3));
    }

    #[test]
    fn single_character_bracket_expression_pattern() {
        let p = Pattern::new(without_escape("[a]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert!(!p.is_match(""));
        assert!(p.is_match("a"));
        assert!(p.is_match("[a]"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("[a]"), Some(1..2));
    }

    // TODO multi_character_bracket_expression_pattern
    // TODO special_characters_in_bracket_expression
    // TODO character_range_in_bracket_expression
    // TODO bracket_expression_complement
    // TODO collating_symbol_in_bracket_expression
    // TODO equivalence_class_in_bracket_expression
    // TODO character_class_in_bracket_expression

    // TODO Config
    // TODO CharKind
}
