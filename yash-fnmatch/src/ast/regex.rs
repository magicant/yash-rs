// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! Conversion to regular expression

use super::*;
use crate::Config;
use std::fmt::Result;
use std::fmt::Write;

const SPECIAL_CHARS: &str = r"\.+*?()|[]{}^$";

pub trait ToRegex {
    /// Converts this pattern to a regular expression.
    ///
    /// The result is written to `regex`.
    fn fmt_regex(&self, config: &Config, regex: &mut dyn Write) -> Result;

    /// Converts this pattern to a regular expression.
    ///
    /// The result is returned as a string.
    fn to_regex(&self, config: &Config) -> String {
        let mut regex = String::new();
        self.fmt_regex(config, &mut regex)
            .expect("write should not fail");
        regex
    }
}

impl ToRegex for Atom {
    fn fmt_regex(&self, _config: &Config, regex: &mut dyn Write) -> Result {
        match self {
            Atom::Char(c) => {
                if SPECIAL_CHARS.contains(*c) {
                    regex.write_char('\\')?;
                }
                regex.write_char(*c)
            }
            Atom::AnyChar => regex.write_char('.'),
            Atom::AnyString => regex.write_str(".*"),
            _ => todo!(),
        }
    }
}

impl ToRegex for Ast {
    fn fmt_regex(&self, config: &Config, regex: &mut dyn Write) -> Result {
        self.atoms
            .iter()
            .try_for_each(|atom| atom.fmt_regex(config, regex))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pattern() {
        let ast = Ast { atoms: vec![] };
        let regex = ast.to_regex(&Config::default());
        assert_eq!(regex, "");
    }

    #[test]
    fn char_pattern() {
        let atoms = vec![Atom::Char('a')];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default());
        assert_eq!(regex, "a");

        let atoms = vec![Atom::Char('1'), Atom::Char('9')];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default());
        assert_eq!(regex, "19");
    }

    #[test]
    fn characters_that_needs_escaping() {
        let atoms = SPECIAL_CHARS.chars().map(Atom::Char).collect();
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default());
        assert_eq!(regex, r"\\\.\+\*\?\(\)\|\[\]\{\}\^\$");
    }

    #[test]
    fn any_patterns() {
        let atoms = vec![Atom::AnyChar, Atom::AnyString, Atom::AnyChar];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default());
        assert_eq!(regex, "..*.");
    }

    // TODO Config
}
