// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! Abstract syntax tree for globbing patterns

use crate::Config;
use crate::Error;
use crate::PatternChar;

/// Pattern component
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Atom {
    /// Literal character
    Char(char),
}

/// Abstract syntax tree for a whole pattern
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Ast {
    pub atoms: Vec<Atom>,
}

impl Ast {
    /// Creates a pattern with defaulted configuration.
    #[inline]
    pub fn new<I>(pattern: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = PatternChar>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        Self::with_config(pattern, Config::default())
    }

    /// Creates a pattern with a specified configuration.
    pub fn with_config<I>(pattern: I, _config: Config) -> Result<Self, Error>
    where
        I: IntoIterator<Item = PatternChar>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        if let Some(pc) = pattern.into_iter().next() {
            Ok(Ast {
                atoms: vec![Atom::Char(pc.char_value())],
            })
        } else {
            Ok(Self::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::without_escape;

    #[test]
    fn empty_pattern() {
        let ast = Ast::new(without_escape("")).unwrap();
        assert_eq!(ast.atoms, []);
    }

    #[test]
    fn single_character_pattern() {
        let ast = Ast::new(without_escape("a")).unwrap();
        assert_eq!(ast.atoms, [Atom::Char('a')]);

        let ast = Ast::new(without_escape("0")).unwrap();
        assert_eq!(ast.atoms, [Atom::Char('0')]);
    }
}
