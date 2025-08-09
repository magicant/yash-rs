// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki

//! This crate provides a function that quotes a string according to the POSIX
//! shell quoting rules.
//!
//! When used in a POSIX shell script, the resultant string will expand to a
//! single field having the same value as the original string.
//!
//! POSIX specifies several types of quoting mechanisms we can use. This crate
//! picks one according to the following decision rules:
//!
//! - If the string is not empty and contains no characters that need quoting,
//!   the string is returned intact.
//! - Otherwise, if the string contains no single quote, the whole string is
//!   single-quoted.
//! - Otherwise, the whole string is double-quoted, and all occurrences of `"`,
//!   `` ` ``, `$`, and `\` are backslash-escaped.
//!
//! The following characters need quoting:
//!
//! - `;`, `&`, `|`, `(`, `)`, `<`, and `>`
//! - A space, tab, newline, or any other whitespace character
//! - `$`, `` ` ``, `\`, `"`, and `'`
//! - `=`, `*`, and `?`
//! - `#` or `~` occurring at the beginning of the string
//! - `:` immediately followed by `~`
//! - `{` preceding `}`
//! - `[` preceding `]`
//!
//!
//! The [`quoted`] function wraps a string in [`Quoted`], which implements
//! `Display` to produce the quoted version of the string with a formatter. The
//! [`quote`] function returns a `Cow<str>`, avoiding unnecessary clone of the
//! string if it requires no quoting.
//!
//! # Examples
//!
//! ```
//! # use yash_quote::quoted;
//! assert_eq!(format!("value={}", quoted("foo")), "value=foo");
//! assert_eq!(format!("value={}", quoted("")), "value=''");
//! assert_eq!(format!("value={}", quoted("$foo")), "value='$foo'");
//! assert_eq!(format!("value={}", quoted("'$foo'")), r#"value="'\$foo'""#);
//! ```
//!
//! ```
//! # use yash_quote::quote;
//! assert_eq!(quote("foo"), "foo");
//! assert_eq!(quote(""), "''");
//! assert_eq!(quote("$foo"), "'$foo'");
//! assert_eq!(quote("'$foo'"), r#""'\$foo'""#);
//! ```

use std::borrow::Cow::{self, Borrowed, Owned};

#[must_use]
fn char_needs_quoting(c: char) -> bool {
    match c {
        ';' | '&' | '|' | '(' | ')' | '<' | '>' | ' ' | '\t' | '\n' => true,
        '$' | '`' | '\\' | '"' | '\'' | '=' | '*' | '?' => true,
        _ => c.is_whitespace(),
    }
}

#[must_use]
fn str_needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }

    // `#` or `~` occurring at the beginning of the string
    if let Some(c) = s.chars().next() {
        if c == '#' || c == '~' {
            return true;
        }
    }

    // characters that require quoting regardless of the position
    if s.chars().any(char_needs_quoting) {
        return true;
    }

    // `:` immediately followed by `~`
    if s.contains(":~") {
        return true;
    }

    // `{` preceding `}`
    if let Some(i) = s.find('{') {
        if s[i + 1..].contains('}') {
            return true;
        }
    }

    // `[` preceding `]`
    if let Some(i) = s.find('[') {
        if s[i + 1..].contains(']') {
            return true;
        }
    }

    false
}

/// Wrapper for quoting a string.
///
/// `Quoted` wraps a `&str` and implements `Display` to produce a quoted version
/// of the string. The implementation prints the same result as [`quote`] but
/// may be more efficient if the result is to be part of a larger string built
/// with a formatter.
#[derive(Clone, Copy, Debug)]
#[must_use = "`Quoted` does nothing unless printed"]
pub struct Quoted<'a> {
    raw: &'a str,
    needs_quoting: bool,
}

impl<'a> Quoted<'a> {
    /// Returns the original string.
    #[inline]
    #[must_use]
    pub fn as_raw(&self) -> &'a str {
        self.raw
    }

    /// Tests whether the contained string requires quoting.
    #[inline]
    #[must_use]
    pub fn needs_quoting(&self) -> bool {
        self.needs_quoting
    }
}

/// Quotes the contained string.
impl std::fmt::Display for Quoted<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Write;
        if !self.needs_quoting {
            f.write_str(self.raw)
        } else if !self.raw.contains('\'') {
            write!(f, "'{}'", self.raw)
        } else {
            f.write_char('"')?;
            for c in self.raw.chars() {
                if matches!(c, '"' | '`' | '$' | '\\') {
                    f.write_char('\\')?;
                }
                f.write_char(c)?;
            }
            f.write_char('"')
        }
    }
}

/// Wraps a string in [`Quoted`].
///
/// This function scans the string to cache the value for
/// [`Quoted::needs_quoting`], so this is an _O_(_n_) operation.
impl<'a> From<&'a str> for Quoted<'a> {
    #[inline]
    fn from(raw: &'a str) -> Self {
        let needs_quoting = str_needs_quoting(raw);
        Quoted { raw, needs_quoting }
    }
}

/// Constructs a quoted string.
impl<'a> From<Quoted<'a>> for Cow<'a, str> {
    fn from(q: Quoted<'a>) -> Self {
        if q.needs_quoting() {
            Owned(q.to_string())
        } else {
            Borrowed(q.as_raw())
        }
    }
}

/// Wraps a string in [`Quoted`].
///
/// This function scans the string to cache the value for
/// [`Quoted::needs_quoting`], so this is an _O_(_n_) operation.
#[inline]
pub fn quoted(raw: &str) -> Quoted<'_> {
    Quoted::from(raw)
}

/// Quotes the argument.
///
/// If the argument needs no quoting, the return value is `Borrowed(raw)`.
/// Otherwise, it is `Owned(new_quoted_string)`.
///
/// See the [module doc](self) for more details.
#[inline]
#[must_use]
pub fn quote(raw: &str) -> Cow<'_, str> {
    quoted(raw).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_quoting() {
        fn test(s: &str) {
            assert_eq!(quote(s), Borrowed(s));
        }
        test("a");
        test("z");
        test("_");
        test("!#%+,-./:@^~");
        test("{");
        test("{x");
        test("}");
        test("x}");
        test("}{");
        test("[");
        test("[x");
        test("]");
        test("x]");
        test("][");
    }

    #[test]
    fn single_quoted() {
        fn test(s: &str) {
            assert_eq!(quote(s), Owned::<str>(format!("'{s}'")));
        }
        test("");
        for c in ";&|()<> \t\n\u{3000}$`\\\"=*?#~".chars() {
            test(&c.to_string());
        }
        test("{}");
        test("{a}");
        test("[]");
        test("[a]");
        test("foo:~bar");
    }

    #[test]
    fn double_quoted() {
        fn test(input: &str, output: &str) {
            assert_eq!(quote(input), Owned::<str>(output.to_string()));
        }
        test("'", r#""'""#);
        test(r#"'"'"#, r#""'\"'""#);
        test("'$", r#""'\$""#);
        test("'foo'", r#""'foo'""#);
        test(r"'\'\\''", r#""'\\'\\\\''""#);
        test("'{\n}'", "\"'{\n}'\"");
    }
}
