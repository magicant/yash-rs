// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! Conversion to regular expression

use super::*;
use crate::Config;
use std::fmt::Error;
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
    fn to_regex(&self, config: &Config) -> std::result::Result<String, Error> {
        let mut regex = String::new();
        self.fmt_regex(config, &mut regex)?;
        Ok(regex)
    }
}

impl ToRegex for BracketAtom {
    fn fmt_regex(&self, _config: &Config, regex: &mut dyn Write) -> Result {
        match self {
            BracketAtom::Char(c) => {
                if *c == '-' || SPECIAL_CHARS.contains(*c) {
                    regex.write_char('\\')?;
                }
                regex.write_char(*c)
            }
            BracketAtom::CollatingSymbol(value) | BracketAtom::EquivalenceClass(value) => {
                regex.write_str(value)
            }
            BracketAtom::CharClass(ClassAsciiKind::Alnum) => regex.write_str("[:alnum:]"),
            BracketAtom::CharClass(ClassAsciiKind::Alpha) => regex.write_str("[:alpha:]"),
            BracketAtom::CharClass(ClassAsciiKind::Ascii) => regex.write_str("[:ascii:]"),
            BracketAtom::CharClass(ClassAsciiKind::Blank) => regex.write_str("[:blank:]"),
            BracketAtom::CharClass(ClassAsciiKind::Cntrl) => regex.write_str("[:cntrl:]"),
            BracketAtom::CharClass(ClassAsciiKind::Digit) => regex.write_str("[:digit:]"),
            BracketAtom::CharClass(ClassAsciiKind::Graph) => regex.write_str("[:graph:]"),
            BracketAtom::CharClass(ClassAsciiKind::Lower) => regex.write_str("[:lower:]"),
            BracketAtom::CharClass(ClassAsciiKind::Print) => regex.write_str("[:print:]"),
            BracketAtom::CharClass(ClassAsciiKind::Punct) => regex.write_str("[:punct:]"),
            BracketAtom::CharClass(ClassAsciiKind::Space) => regex.write_str("[:space:]"),
            BracketAtom::CharClass(ClassAsciiKind::Upper) => regex.write_str("[:upper:]"),
            BracketAtom::CharClass(ClassAsciiKind::Word) => regex.write_str("[:word:]"),
            BracketAtom::CharClass(ClassAsciiKind::Xdigit) => regex.write_str("[:xdigit:]"),
        }
    }
}

impl ToRegex for BracketItem {
    fn fmt_regex(&self, config: &Config, regex: &mut dyn Write) -> Result {
        match self {
            BracketItem::Atom(a) => a.fmt_regex(config, regex),
            _ => todo!(),
        }
    }
}

impl ToRegex for Bracket {
    fn fmt_regex(&self, config: &Config, regex: &mut dyn Write) -> Result {
        // TODO self.complement
        if self.items.is_empty() {
            return Err(Error);
        }
        regex.write_char('[')?;
        self.items
            .iter()
            .try_for_each(|item| item.fmt_regex(config, regex))?;
        regex.write_char(']')
    }
}

impl ToRegex for Atom {
    fn fmt_regex(&self, config: &Config, regex: &mut dyn Write) -> Result {
        match self {
            Atom::Char(c) => {
                if SPECIAL_CHARS.contains(*c) {
                    regex.write_char('\\')?;
                }
                regex.write_char(*c)
            }
            Atom::AnyChar => regex.write_char('.'),
            Atom::AnyString => regex.write_str(".*"),
            Atom::Bracket(bracket) => bracket.fmt_regex(config, regex),
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
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "");
    }

    #[test]
    fn char_pattern() {
        let atoms = vec![Atom::Char('a')];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "a");

        let atoms = vec![Atom::Char('1'), Atom::Char('9')];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "19");
    }

    #[test]
    fn characters_that_needs_escaping() {
        let atoms = SPECIAL_CHARS.chars().map(Atom::Char).collect();
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, r"\\\.\+\*\?\(\)\|\[\]\{\}\^\$");
    }

    #[test]
    fn any_patterns() {
        let atoms = vec![Atom::AnyChar, Atom::AnyString, Atom::AnyChar];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "..*.");
    }

    #[test]
    fn empty_bracket() {
        let bracket = Bracket {
            complement: false,
            items: vec![],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let result = ast.to_regex(&Config::default());
        assert_eq!(result, Err(Error));
    }

    #[test]
    fn bracket_with_chars() {
        let bracket = Bracket {
            complement: false,
            items: vec![
                BracketItem::Atom(BracketAtom::Char('a')),
                BracketItem::Atom(BracketAtom::Char('n')),
                BracketItem::Atom(BracketAtom::Char('d')),
            ],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "[and]");
    }

    #[test]
    fn bracket_with_chars_that_needs_escaping() {
        let bracket = Bracket {
            complement: false,
            items: SPECIAL_CHARS
                .chars()
                .chain(std::iter::once('-'))
                .map(|c| BracketItem::Atom(BracketAtom::Char(c)))
                .collect(),
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, r"[\\\.\+\*\?\(\)\|\[\]\{\}\^\$\-]");
    }

    #[test]
    fn single_character_collating_symbol() {
        let bracket = Bracket {
            complement: false,
            items: vec![BracketItem::Atom(BracketAtom::CollatingSymbol(
                "x".to_string(),
            ))],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "[x]");
    }

    #[test]
    #[ignore]
    fn multi_character_collating_symbol() {
        let bracket = Bracket {
            complement: false,
            items: vec![BracketItem::Atom(BracketAtom::CollatingSymbol(
                "ch".to_string(),
            ))],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "ch");
    }

    #[test]
    fn single_character_equivalence_class() {
        let bracket = Bracket {
            complement: false,
            items: vec![BracketItem::Atom(BracketAtom::EquivalenceClass(
                "a".to_string(),
            ))],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "[a]");
    }

    #[test]
    #[ignore]
    fn multi_character_equivalence_class() {
        let bracket = Bracket {
            complement: false,
            items: vec![BracketItem::Atom(BracketAtom::EquivalenceClass(
                "ij".to_string(),
            ))],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "ij");
    }

    #[test]
    fn character_class() {
        let cases = [
            ("alnum", ClassAsciiKind::Alnum),
            ("alpha", ClassAsciiKind::Alpha),
            ("ascii", ClassAsciiKind::Ascii),
            ("blank", ClassAsciiKind::Blank),
            ("cntrl", ClassAsciiKind::Cntrl),
            ("digit", ClassAsciiKind::Digit),
            ("graph", ClassAsciiKind::Graph),
            ("lower", ClassAsciiKind::Lower),
            ("print", ClassAsciiKind::Print),
            ("punct", ClassAsciiKind::Punct),
            ("space", ClassAsciiKind::Space),
            ("upper", ClassAsciiKind::Upper),
            ("word", ClassAsciiKind::Word),
            ("xdigit", ClassAsciiKind::Xdigit),
        ];
        for (name, class) in cases {
            let bracket = Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::CharClass(class))],
            };
            let atoms = vec![Atom::Bracket(bracket)];
            let ast = Ast { atoms };
            let regex = ast.to_regex(&Config::default()).unwrap();
            assert_eq!(regex, format!("[[:{name}:]]"));
        }
    }

    // TODO Config
}
