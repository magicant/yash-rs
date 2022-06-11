// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! Conversion to regular expression

use super::*;
use crate::Config;
use std::fmt::Result;
use std::fmt::Write;

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

impl ToRegex for Ast {
    fn fmt_regex(&self, _config: &Config, regex: &mut dyn Write) -> Result {
        Ok(())
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
    #[ignore]
    fn char_pattern() {
        let ast = Ast {
            atoms: vec![Atom::Char('a')],
        };
        let regex = ast.to_regex(&Config::default());
        assert_eq!(regex, "a");
    }

    // TODO any_patterns

    // TODO Config
}
