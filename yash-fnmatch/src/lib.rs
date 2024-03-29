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
//!     - Character classes (e.g. `[:alpha:]`)
//!
//! The current implementation does not support any locale-specific
//! characteristics. Especially, collating symbols and equivalent classes only
//! match the specified character sequence itself, and character classes only
//! match ASCII characters.
//!
//! This crate is very similar to the [`fnmatch-regex`] crate in that both
//! perform matching by converting the pattern to a regular expression. The
//! `yash-fnmatch` crate tries to support the POSIX specification as much as
//! possible rather than introducing unique (non-portable) functionalities.
//!
//! # Example
//!
//! ```
//! use yash_fnmatch::{Pattern, without_escape};
//! let p = Pattern::parse(without_escape("r*g")).unwrap();
//! assert_eq!(p.find("string"), Some(2..6));
//! ```
//!
//! [`fnmatch-regex`]: https://crates.io/crates/fnmatch-regex

pub mod ast;
mod char_iter;

use self::ast::Ast;
pub use self::char_iter::*;
use regex::Regex;
use regex::RegexBuilder;
use std::ops::Range;
use thiserror::Error;

/// Configuration for a pattern
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct Config {
    /// Whether the pattern matches only at the beginning of text
    ///
    /// For example, the pattern `in` matches the text `begin` iff
    /// `anchor_begin` is `false`.
    pub anchor_begin: bool,

    /// Whether the pattern matches only at the end of text
    ///
    /// For example, the pattern `mat` matches the text `match` iff `anchor_end`
    /// is `false`.
    pub anchor_end: bool,

    /// Whether a leading period has to be matched explicitly
    ///
    /// When `literal_period` is `true`, a leading period in the text, if any,
    /// must be matched by a literal period in the pattern. In other words, a
    /// wildcard pattern (`*` or `?`) or bracket expression (`[...]`) does not
    /// match a leading period. For example, the pattern `*.txt` does not match
    /// the filename `.foo.txt`.
    ///
    /// When `literal_period` is `false`, the above restriction does not apply.
    pub literal_period: bool,

    /// Whether the pattern matches shortest part of text
    ///
    /// When matching the pattern `a*a` against the text `banana`, for example,
    /// the shortest match will be `ana` while the longest `anana`.
    pub shortest_match: bool,

    /// Whether the pattern should match case-insensitively
    ///
    /// For patterns that are literal (i.e., [`Pattern::as_literal`] returns
    /// `Some(literal)`), this flag is ignored.
    /// For non-literal patterns, the "simple" case folding rules defined by
    /// Unicode are applied to allow case-insensitive matches.
    pub case_insensitive: bool,
}

/// Error that may happen in building a pattern.
#[derive(Clone, Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// Empty bracket expression
    ///
    /// This error should not occur in any pattern that was parsed from a
    /// string. It may happen only when converting a hand-crafted AST to a
    /// regular expression.
    #[error("empty bracket expression")]
    EmptyBracket,

    /// Empty collating symbol or equivalence class
    #[error("empty collating symbol")]
    EmptyCollatingSymbol,

    /// Character class with an undefined name
    ///
    /// The associated value is the name that caused the error.
    /// For example, the pattern `[[:nothing:]]` will produce
    /// `Error::UndefinedCharClass("nothing".to_string())`.
    #[error("undefined character class [:{0}:]")]
    UndefinedCharClass(String),

    /// Character class used as a range bound
    ///
    /// The associated value is the name that caused the error.
    /// For example, the pattern `[[:digit:]-0]` will produce
    /// `Error::CharClassInRange("digit".to_string())`.
    #[error("character class [:{0}:] used as range bound")]
    CharClassInRange(String),

    /// Error in underlying regular expression processing
    #[error(transparent)]
    RegexError(#[from] regex::Error),
}

/// Main part of compiled pattern
#[derive(Clone, Debug)]
enum Body {
    /// Literal string pattern
    Literal(String),
    /// Compiled regular expression
    Regex {
        regex: Regex,
        starts_with_literal_dot: bool,
    },
}

/// Compiled globbing pattern
#[derive(Clone, Debug)]
#[must_use = "creating a pattern without doing pattern matching is nonsense"]
pub struct Pattern {
    body: Body,
    config: Config,
}

impl Pattern {
    /// Compiles a pattern with defaulted configuration.
    #[inline]
    pub fn parse<I>(pattern: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = PatternChar>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        Self::from_ast(&Ast::new(pattern))
    }

    /// Compiles a pattern with a specified configuration.
    pub fn parse_with_config<I>(pattern: I, config: Config) -> Result<Self, Error>
    where
        I: IntoIterator<Item = PatternChar>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        Self::from_ast_and_config(&Ast::new(pattern), config)
    }

    /// Compiles a pattern from the given AST with defaulted configuration.
    pub fn from_ast(ast: &Ast) -> Result<Self, Error> {
        Self::from_ast_and_config(ast, Config::default())
    }

    /// Compiles a pattern from the given AST.
    pub fn from_ast_and_config(ast: &Ast, config: Config) -> Result<Self, Error> {
        let body = if let Some(literal) = ast.to_literal() {
            Body::Literal(literal)
        } else {
            Body::Regex {
                regex: RegexBuilder::new(&ast.to_regex(&config)?)
                    .case_insensitive(config.case_insensitive)
                    .dot_matches_new_line(true)
                    .swap_greed(config.shortest_match)
                    .build()?,
                starts_with_literal_dot: ast.starts_with_literal_dot(),
            }
        };
        Ok(Pattern { body, config })
    }

    /// Returns the configuration for this pattern.
    #[inline]
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
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
            Body::Regex { .. } => None,
        }
    }

    /// Returns the only string that matches the pattern, if any.
    ///
    /// If the pattern is made up only of literal characters, this function
    /// returns the characters as a string. If the pattern contains any `?`,
    /// `*`, or bracket expression, the result is `Err(self)`.
    pub fn into_literal(self) -> Result<String, Self> {
        match self.body {
            Body::Literal(s) => Ok(s),
            Body::Regex { .. } => Err(self),
        }
    }

    /// Tests whether this pattern matches the given text.
    #[must_use]
    pub fn is_match(&self, text: &str) -> bool {
        match &self.body {
            Body::Literal(s) => match (self.config.anchor_begin, self.config.anchor_end) {
                (false, false) => text.contains(s),
                (true, false) => text.starts_with(s),
                (false, true) => text.ends_with(s),
                (true, true) => text == s,
            },
            Body::Regex {
                regex,
                starts_with_literal_dot,
            } => {
                let reject_initial_dot =
                    self.config.literal_period && !starts_with_literal_dot && text.starts_with('.');
                #[allow(clippy::bool_to_int_with_if)]
                let at_index = if reject_initial_dot { 1 } else { 0 };
                regex.is_match_at(text, at_index)
            }
        }
    }

    /// Returns the index range where this pattern matches in the given text.
    ///
    /// If `self` matches (part of) `text`, this function returns the index
    /// range of the first match. Otherwise, the result is `None`.
    #[must_use]
    pub fn find(&self, text: &str) -> Option<Range<usize>> {
        match &self.body {
            Body::Literal(s) => match (self.config.anchor_begin, self.config.anchor_end) {
                (false, false) => text.find(s).map(|pos| pos..pos + s.len()),
                (true, false) => text.starts_with(s).then(|| 0..s.len()),
                (false, true) => text.ends_with(s).then(|| text.len() - s.len()..text.len()),
                (true, true) => (text == s).then(|| 0..s.len()),
            },
            Body::Regex {
                regex,
                starts_with_literal_dot,
            } => {
                let reject_initial_dot =
                    self.config.literal_period && !starts_with_literal_dot && text.starts_with('.');
                #[allow(clippy::bool_to_int_with_if)]
                let at_index = if reject_initial_dot { 1 } else { 0 };
                regex.find_at(text, at_index).map(|m| m.range())
            }
        }
    }

    /// Returns the index range where this pattern matches in the given text.
    ///
    /// If `self` matches (part of) `text`, this function returns the index
    /// range of the last match. Otherwise, the result is `None`.
    #[must_use]
    pub fn rfind(&self, text: &str) -> Option<Range<usize>> {
        match &self.body {
            Body::Literal(s) => match (self.config.anchor_begin, self.config.anchor_end) {
                (false, false) => text.rfind(s).map(|pos| pos..pos + s.len()),
                (true, false) => text.starts_with(s).then(|| 0..s.len()),
                (false, true) => text.ends_with(s).then(|| text.len() - s.len()..text.len()),
                (true, true) => (text == s).then(|| 0..s.len()),
            },

            Body::Regex {
                regex,
                starts_with_literal_dot: _,
            } => {
                let mut range = self.find(text)?;

                while let Some(next_range) = (range.start + 1..=text.len())
                    .find(|&index| text.is_char_boundary(index))
                    .and_then(|index| regex.find_at(text, index).map(|m| m.range()))
                {
                    range = next_range;
                }

                Some(range)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn empty_pattern() {
        let p = Pattern::parse(without_escape("")).unwrap();
        assert_eq!(p.as_literal(), Some(""));

        assert!(p.is_match(""));
        assert!(p.is_match("a"));
        assert!(p.is_match("."));
        assert!(p.is_match("*"));

        assert_eq!(p.find(""), Some(0..0));
        assert_eq!(p.find("a"), Some(0..0));
        assert_eq!(p.find("."), Some(0..0));
        assert_eq!(p.find("*"), Some(0..0));

        assert_eq!(p.rfind(""), Some(0..0));
        assert_eq!(p.rfind("a"), Some(1..1));
        assert_eq!(p.rfind("."), Some(1..1));
        assert_eq!(p.rfind("*"), Some(1..1));
    }

    #[test]
    fn single_character_pattern() {
        let p = Pattern::parse(without_escape("a")).unwrap();
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

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("a"), Some(0..1));
        assert_eq!(p.rfind("aa"), Some(1..2));
        assert_eq!(p.rfind("b"), None);
        assert_eq!(p.rfind("ab"), Some(0..1));
        assert_eq!(p.rfind("ba"), Some(1..2));
    }

    #[test]
    fn double_character_pattern() {
        let p = Pattern::parse(without_escape("in")).unwrap();
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

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("i"), None);
        assert_eq!(p.rfind("n"), None);
        assert_eq!(p.rfind("bin"), Some(1..3));
        assert_eq!(p.rfind("inn"), Some(0..2));
        assert_eq!(p.rfind("nit"), None);
    }

    #[test]
    fn characters_that_needs_escaping() {
        let p = Pattern::parse(without_escape(r".\+()][{}^$-?")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(r".\+()][{}^$-X"), Some(0..13));
        assert_eq!(p.find(r".\+()][{}^$-Y"), Some(0..13));
        assert_eq!(p.find(r".\+()][{}^$-"), None);

        assert_eq!(p.rfind(r".\+()][{}^$-X"), Some(0..13));
        assert_eq!(p.rfind(r".\+()][{}^$-Y"), Some(0..13));
        assert_eq!(p.rfind(r".\+()][{}^$-"), None);
    }

    #[test]
    fn matching_newline() {
        let p = Pattern::parse(without_escape("a\nb")).unwrap();
        assert_eq!(p.as_literal(), Some("a\nb"));
        assert_eq!(p.find("a\nb"), Some(0..3));
        assert_eq!(p.rfind("a\nb"), Some(0..3));

        let p = Pattern::parse(without_escape("a?b")).unwrap();
        assert_eq!(p.as_literal(), None);
        assert_eq!(p.find("a\nb"), Some(0..3));
        assert_eq!(p.rfind("a\nb"), Some(0..3));
    }

    #[test]
    fn any_single_character_pattern() {
        let p = Pattern::parse(without_escape("?")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("i"), Some(0..1));
        assert_eq!(p.find("yes"), Some(0..1));

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("i"), Some(0..1));
        assert_eq!(p.rfind("yes"), Some(2..3));
    }

    #[test]
    fn any_single_character_pattern_combined() {
        let p = Pattern::parse(without_escape("a?c")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("ab"), None);
        assert_eq!(p.find("ac"), None);
        assert_eq!(p.find("bc"), None);
        assert_eq!(p.find("abc"), Some(0..3));

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("ab"), None);
        assert_eq!(p.rfind("ac"), None);
        assert_eq!(p.rfind("bc"), None);
        assert_eq!(p.rfind("abc"), Some(0..3));
    }

    #[test]
    fn any_multi_character_pattern() {
        let p = Pattern::parse(without_escape("*")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), Some(0..0));
        assert_eq!(p.find("i"), Some(0..1));
        assert_eq!(p.find("yes"), Some(0..3));

        assert_eq!(p.rfind(""), Some(0..0));
        assert_eq!(p.rfind("i"), Some(1..1));
        assert_eq!(p.rfind("yes"), Some(3..3));
    }

    #[test]
    fn any_multi_character_pattern_combined() {
        let p = Pattern::parse(without_escape("a*b")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("ab"), Some(0..2));
        assert_eq!(p.find("aabb"), Some(0..4));
        assert_eq!(p.find("lambda"), Some(1..4));

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("a"), None);
        assert_eq!(p.rfind("ab"), Some(0..2));
        assert_eq!(p.rfind("aabb"), Some(1..4));
        assert_eq!(p.rfind("lambda"), Some(1..4));
    }

    #[test]
    fn unmatched_bracket_1() {
        let p = Pattern::parse(without_escape("[a")).unwrap();
        assert_eq!(p.as_literal(), Some("[a"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("[a]"), Some(0..2));
    }

    #[test]
    fn unmatched_bracket_2() {
        let p = Pattern::parse(without_escape("a]")).unwrap();
        assert_eq!(p.as_literal(), Some("a]"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("[a]"), Some(1..3));
    }

    #[test]
    fn unmatched_bracket_3() {
        let p = Pattern::parse(without_escape("][")).unwrap();
        assert_eq!(p.as_literal(), Some("]["));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("]["), Some(0..2));
        assert_eq!(p.find("[][]"), Some(1..3));
    }

    #[test]
    fn unmatched_bracket_4() {
        let p = Pattern::parse(without_escape("[]")).unwrap();
        assert_eq!(p.as_literal(), Some("[]"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("[]"), Some(0..2));
        assert_eq!(p.find("][]["), Some(1..3));
    }

    #[test]
    fn unmatched_bracket_after_another_bracket() {
        let p = Pattern::parse(without_escape("[a][")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("["), None);
        assert_eq!(p.find("a["), Some(0..2));
        assert_eq!(p.find("]]a[[a][]"), Some(2..4));
    }

    #[test]
    fn single_character_bracket_expression_pattern() {
        let p = Pattern::parse(without_escape("[a]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("[a]"), Some(1..2));
    }

    #[test]
    fn multi_character_bracket_expression_pattern() {
        let p = Pattern::parse(without_escape("[abc]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("b"), Some(0..1));
        assert_eq!(p.find("c"), Some(0..1));
        assert_eq!(p.find("d"), None);
    }

    #[test]
    fn ampersand_in_bracket_expression() {
        let p = Pattern::parse(without_escape("[a&&b]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("b"), Some(0..1));
        assert_eq!(p.find("&"), Some(0..1));
    }

    #[test]
    fn tilde_in_bracket_expression() {
        let p = Pattern::parse(without_escape("[a~~b]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("b"), Some(0..1));
        assert_eq!(p.find("~"), Some(0..1));
    }

    #[test]
    fn characters_that_needs_escaping_in_bracket_expression() {
        let p = Pattern::parse(without_escape("[.?*-][&|~^][][]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(".&["), Some(0..3));
        assert_eq!(p.find("?|["), Some(0..3));
        assert_eq!(p.find("*~]"), Some(0..3));
        assert_eq!(p.find("-^]"), Some(0..3));
        assert_eq!(p.find("?&]"), Some(0..3));
    }

    #[test]
    fn brackets_in_bracket_expression() {
        let p = Pattern::parse(without_escape("[]a[]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("["), Some(0..1));
        assert_eq!(p.find("]"), Some(0..1));
    }

    #[test]
    fn character_range() {
        let p = Pattern::parse(without_escape("[3-5]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("2"), None);
        assert_eq!(p.find("3"), Some(0..1));
        assert_eq!(p.find("4"), Some(0..1));
        assert_eq!(p.find("5"), Some(0..1));
        assert_eq!(p.find("6"), None);
        assert_eq!(p.find("02468"), Some(2..3));
    }

    #[test]
    fn dash_at_start_of_bracket_expression() {
        // This bracket expression should match only '-' and '0'.
        let p = Pattern::parse(without_escape("[-0]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("-"), Some(0..1));
        assert_eq!(p.find("."), None);
        assert_eq!(p.find("0"), Some(0..1));
        assert_eq!(p.find("1"), None);
    }

    #[test]
    fn dash_at_end_of_bracket_expression() {
        // This bracket expression should match only '+' and '-'.
        let p = Pattern::parse(without_escape("[+-]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("+"), Some(0..1));
        assert_eq!(p.find(","), None);
        assert_eq!(p.find("-"), Some(0..1));
        assert_eq!(p.find("."), None);
    }

    #[test]
    fn ambiguous_character_range() {
        let p = Pattern::parse(without_escape("[2-4-6]")).unwrap();
        assert_eq!(p.as_literal(), None);

        // POSIX leaves the expected results unspecified.
        // The results below depend on the current behavior of the regex crate.
        assert_eq!(p.find("1"), None);
        assert_eq!(p.find("2"), Some(0..1));
        assert_eq!(p.find("3"), Some(0..1));
        assert_eq!(p.find("4"), Some(0..1));
        assert_eq!(p.find("5"), None);
        assert_eq!(p.find("6"), Some(0..1));
        assert_eq!(p.find("7"), None);
    }

    #[test]
    fn double_dash_in_bracket_expression() {
        // This bracket expression should be parsed as a union of the character
        // range between '+' and '-', and a single dot.
        let p = Pattern::parse(without_escape("[+--.]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("+"), Some(0..1));
        assert_eq!(p.find(","), Some(0..1));
        assert_eq!(p.find("-"), Some(0..1));
        assert_eq!(p.find("."), Some(0..1));
    }

    #[test]
    fn double_dash_at_start_of_bracket_expression() {
        // This bracket expression should be parsed as the character range
        // between '-' and '0'.
        let p = Pattern::parse(without_escape("[--0]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("-"), Some(0..1));
        assert_eq!(p.find("."), Some(0..1));
        assert_eq!(p.find("0"), Some(0..1));
        assert_eq!(p.find("1"), None);
    }

    #[test]
    fn double_dash_at_end_of_bracket_expression() {
        // This bracket expression should be parsed as the character range
        // between '+' and '-'.
        let p = Pattern::parse(without_escape("[+--]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("+"), Some(0..1));
        assert_eq!(p.find(","), Some(0..1));
        assert_eq!(p.find("-"), Some(0..1));
        assert_eq!(p.find("."), None);
    }

    #[test]
    fn bracket_expression_complement() {
        let p = Pattern::parse(without_escape("[!ab]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("b"), None);
        assert_eq!(p.find("c"), Some(0..1));
        assert_eq!(p.find("!"), Some(0..1));
        assert_eq!(p.find("abcd"), Some(2..3));

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("a"), None);
        assert_eq!(p.rfind("b"), None);
        assert_eq!(p.rfind("c"), Some(0..1));
        assert_eq!(p.rfind("!"), Some(0..1));
        assert_eq!(p.rfind("abcd"), Some(3..4));
    }

    #[test]
    fn exclamation_in_bracket_expression() {
        let p = Pattern::parse(without_escape("[ab!]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("b"), Some(0..1));
        assert_eq!(p.find("c"), None);
        assert_eq!(p.find("!"), Some(0..1));
    }

    #[test]
    fn exclamation_in_bracket_expression_complement() {
        let p = Pattern::parse(without_escape("[!!]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("!"), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("!x!"), Some(1..2));
    }

    #[test]
    fn bracket_in_bracket_expression_complement() {
        let p = Pattern::parse(without_escape("[!]a]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("!"), Some(0..1));
        assert_eq!(p.find("]"), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("b"), Some(0..1));
        assert_eq!(p.find("abc"), Some(1..2));
    }

    #[test]
    fn caret_in_bracket_expression() {
        let p = Pattern::parse(without_escape("[^]a]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("^"), Some(0..1));
        assert_eq!(p.find("]"), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("b"), Some(0..1));
        assert_eq!(p.find("abc"), Some(1..2));

        let p = Pattern::parse(without_escape("[^^]")).unwrap();
        assert_eq!(p.as_literal(), None);
        assert!(!p.is_match("^"));
    }

    #[test]
    fn single_character_collating_symbol() {
        let p = Pattern::parse(without_escape("[[.a.]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("x"), None);
        assert_eq!(p.find("."), None);
        assert_eq!(p.find("[a]"), Some(1..2));
    }

    #[test]
    fn multi_character_collating_symbol() {
        let p = Pattern::parse(without_escape("[[.ch.]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("c"), None);
        assert_eq!(p.find("h"), None);
        assert_eq!(p.find("."), None);
        assert_eq!(p.find("ch"), Some(0..2));
        assert_eq!(p.find("[ch]"), Some(1..3));
    }

    #[test]
    fn single_character_equivalence_class() {
        let p = Pattern::parse(without_escape("[[=a=]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("x"), None);
        assert_eq!(p.find("="), None);
        assert_eq!(p.find("[a]"), Some(1..2));
    }

    #[test]
    fn multi_character_equivalence_class() {
        let p = Pattern::parse(without_escape("[[=ij=]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("i"), None);
        assert_eq!(p.find("j"), None);
        assert_eq!(p.find("."), None);
        assert_eq!(p.find("ij"), Some(0..2));
        assert_eq!(p.find("[ij]"), Some(1..3));
    }

    #[test]
    fn character_class_alnum() {
        let p = Pattern::parse(without_escape("[[:alnum:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("x"), Some(0..1));
        assert_eq!(p.find("7"), Some(0..1));
        assert_eq!(p.find("="), None);
        assert_eq!(p.find(":"), None);
        assert_eq!(p.find("[A]"), Some(1..2));
    }

    #[test]
    fn character_class_alpha() {
        let p = Pattern::parse(without_escape("[[:alpha:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("x"), Some(0..1));
        assert_eq!(p.find("7"), None);
        assert_eq!(p.find("="), None);
        assert_eq!(p.find(":"), None);
        assert_eq!(p.find("[A]"), Some(1..2));
    }

    #[test]
    fn character_class_blank() {
        let p = Pattern::parse(without_escape("[[:blank:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("="), None);
        assert_eq!(p.find(" "), Some(0..1));
        assert_eq!(p.find("\t"), Some(0..1));
        assert_eq!(p.find("\n"), None);
        assert_eq!(p.find("\r"), None);
        assert_eq!(p.find("[A]"), None);
    }

    #[test]
    fn character_class_cntrl() {
        let p = Pattern::parse(without_escape("[[:cntrl:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("="), None);
        assert_eq!(p.find(" "), None);
        assert_eq!(p.find("\t"), Some(0..1));
        assert_eq!(p.find("\n"), Some(0..1));
        assert_eq!(p.find("\r"), Some(0..1));
        assert_eq!(p.find("[A]"), None);
    }

    #[test]
    fn character_class_digit() {
        let p = Pattern::parse(without_escape("[[:digit:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("x"), None);
        assert_eq!(p.find("7"), Some(0..1));
        assert_eq!(p.find("="), None);
        assert_eq!(p.find(":"), None);
        assert_eq!(p.find("[A]"), None);
    }

    #[test]
    fn character_class_graph() {
        let p = Pattern::parse(without_escape("[[:graph:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("x"), Some(0..1));
        assert_eq!(p.find("7"), Some(0..1));
        assert_eq!(p.find("="), Some(0..1));
        assert_eq!(p.find(":"), Some(0..1));
        assert_eq!(p.find(" "), None);
        assert_eq!(p.find("\t"), None);
        assert_eq!(p.find("[A]"), Some(0..1));
    }

    #[test]
    fn character_class_lower() {
        let p = Pattern::parse(without_escape("[[:lower:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("A"), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("x"), Some(0..1));
        assert_eq!(p.find("7"), None);
        assert_eq!(p.find("="), None);
        assert_eq!(p.find("\t"), None);
        assert_eq!(p.find("[a]"), Some(1..2));
    }

    #[test]
    fn character_class_print() {
        let p = Pattern::parse(without_escape("[[:print:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("x"), Some(0..1));
        assert_eq!(p.find("7"), Some(0..1));
        assert_eq!(p.find("="), Some(0..1));
        assert_eq!(p.find(":"), Some(0..1));
        assert_eq!(p.find(" "), Some(0..1));
        assert_eq!(p.find("\t"), None);
        assert_eq!(p.find("[A]"), Some(0..1));
    }

    #[test]
    fn character_class_punct() {
        let p = Pattern::parse(without_escape("[[:punct:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("x"), None);
        assert_eq!(p.find("7"), None);
        assert_eq!(p.find("="), Some(0..1));
        assert_eq!(p.find(":"), Some(0..1));
        assert_eq!(p.find("[A]"), Some(0..1));
    }

    #[test]
    fn character_class_space() {
        let p = Pattern::parse(without_escape("[[:space:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("="), None);
        assert_eq!(p.find(" "), Some(0..1));
        assert_eq!(p.find("\t"), Some(0..1));
        assert_eq!(p.find("\n"), Some(0..1));
        assert_eq!(p.find("\r"), Some(0..1));
        assert_eq!(p.find("[A]"), None);
    }

    #[test]
    fn character_class_upper() {
        let p = Pattern::parse(without_escape("[[:upper:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("A"), Some(0..1));
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("X"), Some(0..1));
        assert_eq!(p.find("7"), None);
        assert_eq!(p.find("="), None);
        assert_eq!(p.find("\t"), None);
        assert_eq!(p.find("[A]"), Some(1..2));
    }

    #[test]
    fn character_class_xdigit() {
        let p = Pattern::parse(without_escape("[[:xdigit:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("x"), None);
        assert_eq!(p.find("7"), Some(0..1));
        assert_eq!(p.find("="), None);
        assert_eq!(p.find(":"), None);
        assert_eq!(p.find("[A]"), Some(1..2));
    }

    #[test]
    fn undefined_character_class() {
        let e = Pattern::parse(without_escape("[[:foo_bar:]]")).unwrap_err();
        assert_matches!(e, Error::UndefinedCharClass(name) if name == "foo_bar");
    }

    #[test]
    fn combinations_of_inner_bracket_expressions() {
        let p = Pattern::parse(without_escape("[][.-.]-[=0=][:blank:]]")).unwrap();
        assert_eq!(p.as_literal(), None);

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("]"), Some(0..1));
        assert_eq!(p.find("-"), Some(0..1));
        assert_eq!(p.find("."), Some(0..1));
        assert_eq!(p.find("/"), Some(0..1));
        assert_eq!(p.find("0"), Some(0..1));
        assert_eq!(p.find("1"), None);
        assert_eq!(p.find(" "), Some(0..1));
        assert_eq!(p.find("\t"), Some(0..1));
        assert_eq!(p.find("["), None);
    }

    #[test]
    fn escaped_pattern() {
        let p = Pattern::parse(with_escape(r"\*\?\[a]")).unwrap();
        assert_eq!(p.as_literal(), Some("*?[a]"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("*?[a]"), Some(0..5));
        assert_eq!(p.find("aaa"), None);
    }

    #[test]
    fn literal_with_anchor_begin() {
        let config = Config {
            anchor_begin: true,
            ..Config::default()
        };
        let p = Pattern::parse_with_config(without_escape("a"), config).unwrap();
        assert_eq!(p.as_literal(), Some("a"));

        assert!(!p.is_match(""));
        assert!(p.is_match("a"));
        assert!(!p.is_match(".a"));
        assert!(p.is_match("a."));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find(".a"), None);
        assert_eq!(p.find("a."), Some(0..1));

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("a"), Some(0..1));
        assert_eq!(p.rfind(".a"), None);
        assert_eq!(p.rfind("a."), Some(0..1));
    }

    #[test]
    fn non_literal_with_anchor_begin() {
        let config = Config {
            anchor_begin: true,
            ..Config::default()
        };
        let p = Pattern::parse_with_config(without_escape("a?"), config).unwrap();
        assert_eq!(p.as_literal(), None);

        assert!(!p.is_match(""));
        assert!(!p.is_match("a"));
        assert!(p.is_match("as"));
        assert!(p.is_match("apple"));
        assert!(!p.is_match("bass"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), None);
        assert_eq!(p.find("as"), Some(0..2));
        assert_eq!(p.find("apple"), Some(0..2));
        assert_eq!(p.find("bass"), None);

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("a"), None);
        assert_eq!(p.rfind("as"), Some(0..2));
        assert_eq!(p.rfind("apple"), Some(0..2));
        assert_eq!(p.rfind("bass"), None);
    }

    #[test]
    fn literal_with_anchor_end() {
        let config = Config {
            anchor_end: true,
            ..Config::default()
        };
        let p = Pattern::parse_with_config(without_escape("a"), config).unwrap();
        assert_eq!(p.as_literal(), Some("a"));

        assert!(!p.is_match(""));
        assert!(p.is_match("a"));
        assert!(p.is_match("..a"));
        assert!(!p.is_match("a.."));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("..a"), Some(2..3));
        assert_eq!(p.find("a.."), None);

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("a"), Some(0..1));
        assert_eq!(p.rfind("..a"), Some(2..3));
        assert_eq!(p.rfind("a.."), None);
    }

    #[test]
    fn non_literal_with_anchor_end() {
        let config = Config {
            anchor_end: true,
            ..Config::default()
        };
        let p = Pattern::parse_with_config(without_escape("?n"), config).unwrap();
        assert_eq!(p.as_literal(), None);

        assert!(!p.is_match(""));
        assert!(!p.is_match("n"));
        assert!(p.is_match("in"));
        assert!(p.is_match("begin"));
        assert!(!p.is_match("beginning"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("n"), None);
        assert_eq!(p.find("in"), Some(0..2));
        assert_eq!(p.find("begin"), Some(3..5));
        assert_eq!(p.find("beginning"), None);

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("n"), None);
        assert_eq!(p.rfind("in"), Some(0..2));
        assert_eq!(p.rfind("begin"), Some(3..5));
        assert_eq!(p.rfind("beginning"), None);
    }

    #[test]
    fn literal_with_anchor_both() {
        let config = Config {
            anchor_begin: true,
            anchor_end: true,
            ..Config::default()
        };
        let p = Pattern::parse_with_config(without_escape("a"), config).unwrap();
        assert_eq!(p.as_literal(), Some("a"));

        assert!(!p.is_match(""));
        assert!(p.is_match("a"));
        assert!(!p.is_match("..a"));
        assert!(!p.is_match("a.."));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.find("..a"), None);
        assert_eq!(p.find("a.."), None);

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("a"), Some(0..1));
        assert_eq!(p.rfind("..a"), None);
        assert_eq!(p.rfind("a.."), None);
    }

    #[test]
    fn non_literal_with_anchor_both() {
        let config = Config {
            anchor_begin: true,
            anchor_end: true,
            ..Config::default()
        };
        let p = Pattern::parse_with_config(without_escape("???"), config).unwrap();
        assert_eq!(p.as_literal(), None);

        assert!(!p.is_match("in"));
        assert!(p.is_match("out"));
        assert!(!p.is_match("from"));

        assert_eq!(p.find("in"), None);
        assert_eq!(p.find("out"), Some(0..3));
        assert_eq!(p.find("from"), None);

        assert_eq!(p.rfind("in"), None);
        assert_eq!(p.rfind("out"), Some(0..3));
        assert_eq!(p.rfind("from"), None);
    }

    #[test]
    fn initial_period_with_literal_period() {
        let config = Config {
            literal_period: true,
            ..Config::default()
        };

        let p = Pattern::parse_with_config(without_escape("?"), config).unwrap();
        assert!(!p.is_match("."));
        assert!(p.is_match(".."));
        assert!(p.is_match("a"));
        assert_eq!(p.find("."), None);
        assert_eq!(p.find(".."), Some(1..2));
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.rfind("."), None);
        assert_eq!(p.rfind(".."), Some(1..2));
        assert_eq!(p.rfind("a"), Some(0..1));

        let p = Pattern::parse_with_config(without_escape("*"), config).unwrap();
        assert!(p.is_match("."));
        assert!(p.is_match(".."));
        assert!(p.is_match("a"));
        assert_eq!(p.find("."), Some(1..1));
        assert_eq!(p.find(".."), Some(1..2));
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.rfind("."), Some(1..1));
        assert_eq!(p.rfind(".."), Some(2..2));
        assert_eq!(p.rfind("a"), Some(1..1));

        let p = Pattern::parse_with_config(without_escape("[.a]"), config).unwrap();
        assert!(!p.is_match("."));
        assert!(p.is_match(".."));
        assert!(p.is_match("a"));
        assert_eq!(p.find("."), None);
        assert_eq!(p.find(".."), Some(1..2));
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.rfind("."), None);
        assert_eq!(p.rfind(".."), Some(1..2));
        assert_eq!(p.rfind("a"), Some(0..1));

        let p = Pattern::parse_with_config(without_escape(".*"), config).unwrap();
        assert!(p.is_match("."));
        assert!(p.is_match(".."));
        assert_eq!(p.find("."), Some(0..1));
        assert_eq!(p.find(".."), Some(0..2));
        assert_eq!(p.rfind("."), Some(0..1));
        assert_eq!(p.rfind(".."), Some(1..2));
    }

    #[test]
    fn initial_period_without_literal_period() {
        let config = Config::default();

        let p = Pattern::parse_with_config(without_escape("?"), config).unwrap();
        assert!(p.is_match("."));
        assert!(p.is_match(".."));
        assert!(p.is_match("a"));
        assert_eq!(p.find("."), Some(0..1));
        assert_eq!(p.find(".."), Some(0..1));
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.rfind("."), Some(0..1));
        assert_eq!(p.rfind(".."), Some(1..2));
        assert_eq!(p.rfind("a"), Some(0..1));

        let p = Pattern::parse_with_config(without_escape("*"), config).unwrap();
        assert!(p.is_match("."));
        assert!(p.is_match(".."));
        assert!(p.is_match("a"));
        assert_eq!(p.find("."), Some(0..1));
        assert_eq!(p.find(".."), Some(0..2));
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.rfind("."), Some(1..1));
        assert_eq!(p.rfind(".."), Some(2..2));
        assert_eq!(p.rfind("a"), Some(1..1));

        let p = Pattern::parse_with_config(without_escape("[.a]"), config).unwrap();
        assert!(p.is_match("."));
        assert!(p.is_match(".."));
        assert!(p.is_match("a"));
        assert_eq!(p.find("."), Some(0..1));
        assert_eq!(p.find(".."), Some(0..1));
        assert_eq!(p.find("a"), Some(0..1));
        assert_eq!(p.rfind("."), Some(0..1));
        assert_eq!(p.rfind(".."), Some(1..2));
        assert_eq!(p.rfind("a"), Some(0..1));

        let p = Pattern::parse_with_config(without_escape(".*"), config).unwrap();
        assert!(p.is_match("."));
        assert!(p.is_match(".."));
        assert_eq!(p.find("."), Some(0..1));
        assert_eq!(p.find(".."), Some(0..2));
        assert_eq!(p.rfind("."), Some(0..1));
        assert_eq!(p.rfind(".."), Some(1..2));
    }

    #[test]
    fn non_literal_with_shortest_match() {
        let p = Pattern::parse_with_config(without_escape("1*9"), Config::default()).unwrap();
        assert!(p.is_match("119"));
        assert!(p.is_match("11999"));
        assert_eq!(p.find("119"), Some(0..3));
        assert_eq!(p.find("11999"), Some(0..5));
        assert_eq!(p.rfind("119"), Some(1..3));
        assert_eq!(p.rfind("11999"), Some(1..5));

        let config = Config {
            shortest_match: true,
            ..Config::default()
        };
        let p = Pattern::parse_with_config(without_escape("1*9"), config).unwrap();
        assert!(p.is_match("119"));
        assert!(p.is_match("11999"));
        assert_eq!(p.find("119"), Some(0..3));
        assert_eq!(p.find("11999"), Some(0..3));
        assert_eq!(p.rfind("119"), Some(1..3));
        assert_eq!(p.rfind("11999"), Some(1..3));
    }

    #[test]
    fn non_literal_with_case_insensitive() {
        let config = Config {
            case_insensitive: true,
            ..Config::default()
        };
        let p = Pattern::parse_with_config(without_escape("a?z"), config).unwrap();
        assert_eq!(p.as_literal(), None);

        assert!(!p.is_match(""));
        assert!(p.is_match("a-z"));
        assert!(p.is_match("A-Z"));
        assert!(!p.is_match("b&b"));

        assert_eq!(p.find(""), None);
        assert_eq!(p.find("a-z"), Some(0..3));
        assert_eq!(p.find("A-Z"), Some(0..3));
        assert_eq!(p.find("b&b"), None);

        assert_eq!(p.rfind(""), None);
        assert_eq!(p.rfind("a-z"), Some(0..3));
        assert_eq!(p.rfind("A-Z"), Some(0..3));
        assert_eq!(p.rfind("b&b"), None);
    }
}
