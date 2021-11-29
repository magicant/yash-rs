// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki

//! This crate provides a function that quotes a string according to the POSIX
//! shell quoting rules.
//!
//! When used in a POSIX shell script, the resultant string will expand to a
//! single field having the same value as the original string.
//!
//! POSIX specifies several types of quoting mechanisms we can use. The
//! [`quote`] function chooses one according to the following decision rules:
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
//! - `{` preceding `}`
//! - `[` preceding `]`
//!
//! # Examples
//!
//! ```
//! # use std::borrow::Cow::{Borrowed, Owned};
//! # use yash_quote::quote;
//! assert_eq!(quote("foo"), Borrowed("foo"));
//! assert_eq!(quote(""), Owned::<str>("''".to_owned()));
//! assert_eq!(quote("$foo"), Owned::<str>("'$foo'".to_owned()));
//! assert_eq!(quote("'$foo'"), Owned::<str>(r#""'\$foo'""#.to_owned()));
//! ```

use std::borrow::Cow::{self, Borrowed, Owned};

/// Quotes the argument.
///
/// If the argument needs no quoting, the return value is `Borrowed(s)`.
/// Otherwise, it is `Owned(new_quoted_string)`.
///
/// See the [module doc](self) for more details.
pub fn quote(s: &str) -> Cow<str> {
    if !s.is_empty() && !str_needs_quoting(s) {
        return Borrowed(s);
    }

    if s.find('\'').is_none() {
        return Owned(format!("'{}'", s));
    }

    let mut result = String::with_capacity(s.len().saturating_add(8));
    result.push('"');
    for c in s.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            result.push('\\');
        }
        result.push(c);
    }
    result.push('"');
    Owned(result)
}

/// Returns true iff any character needs quoting.
fn str_needs_quoting(s: &str) -> bool {
    if s.chars().any(char_needs_quoting) {
        return true;
    }

    // `#` or `~` occurring at the beginning of the string
    if let Some(c) = s.chars().next() {
        if c == '#' || c == '~' {
            return true;
        }
    }

    // `{` preceding `}`
    if let Some(i) = s.find('{') {
        let sub = &s[i + 1..];
        if sub.find('}').is_some() {
            return true;
        }
    }

    // `[` preceding `]`
    if let Some(i) = s.find('[') {
        let sub = &s[i + 1..];
        if sub.find(']').is_some() {
            return true;
        }
    }

    false
}

fn char_needs_quoting(c: char) -> bool {
    match c {
        ';' | '&' | '|' | '(' | ')' | '<' | '>' | ' ' | '\t' | '\n' => true,
        '$' | '`' | '\\' | '"' | '\'' | '=' | '*' | '?' => true,
        _ => c.is_whitespace(),
    }
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
        test("[");
        test("[x");
        test("]");
        test("x]");
    }

    #[test]
    fn single_quoted() {
        fn test(s: &str) {
            assert_eq!(quote(s), Owned::<str>(format!("'{}'", s)));
        }
        test("");
        for c in ";&|()<> \t\n\u{3000}$`\\\"=*?#~".chars() {
            test(&c.to_string());
        }
        test("{}");
        test("{a}");
        test("[]");
        test("[a]");
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
        test(r#"'\'\\''"#, r#""'\\'\\\\''""#);
        test("'{\n}'", "\"'{\n}'\"");
    }
}
