// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! Conversion to regular expression

use super::*;
use crate::Config;
use crate::Error;
use regex_syntax::ast::ClassAsciiKind;
use std::fmt::Write;

type Result = std::result::Result<(), Error>;

const SPECIAL_CHARS: &str = r"\.+*?()|[]{}^$";
const BRACKET_SPECIAL_CHARS: &str = "-&~";

impl BracketAtom {
    fn fmt_regex_char(c: char, regex: &mut dyn Write) -> Result {
        if BRACKET_SPECIAL_CHARS.contains(c) || SPECIAL_CHARS.contains(c) {
            regex.write_char('\\').unwrap();
        }
        regex.write_char(c).unwrap();
        Ok(())
    }

    fn matches_multi_character(&self) -> bool {
        match self {
            BracketAtom::CollatingSymbol(value) | BracketAtom::EquivalenceClass(value) => {
                value.len() > 1
            }
            _ => false,
        }
    }

    fn fmt_regex(&self, regex: &mut dyn Write) -> Result {
        match self {
            BracketAtom::Char(c) => return BracketAtom::fmt_regex_char(*c, regex),
            BracketAtom::CollatingSymbol(value) | BracketAtom::EquivalenceClass(value) => {
                if !value.is_empty() {
                    regex.write_str(value)
                } else {
                    return Err(Error::EmptyCollatingSymbol);
                }
            }
            BracketAtom::CharClass(class) => {
                if ClassAsciiKind::from_name(class).is_some() {
                    regex.write_fmt(format_args!("[:{class}:]"))
                } else {
                    return Err(Error::UndefinedCharClass(class.clone()));
                }
            }
        }
        .unwrap();
        Ok(())
    }

    fn fmt_regex_single(&self, regex: &mut dyn Write) -> Result {
        match self {
            BracketAtom::Char(c) => BracketAtom::fmt_regex_char(*c, regex),
            BracketAtom::CollatingSymbol(value) | BracketAtom::EquivalenceClass(value) => {
                let c = value.chars().next().ok_or(Error::EmptyCollatingSymbol)?;
                BracketAtom::fmt_regex_char(c, regex)
            }
            BracketAtom::CharClass(class) => Err(Error::CharClassInRange(class.clone())),
        }
    }
}

impl BracketItem {
    fn matches_multi_character(&self) -> bool {
        match self {
            BracketItem::Atom(a) => a.matches_multi_character(),
            BracketItem::Range(_) => false,
        }
    }

    fn fmt_regex(&self, regex: &mut dyn Write) -> Result {
        match self {
            BracketItem::Atom(a) => a.fmt_regex(regex),
            BracketItem::Range(range) => {
                range.start().fmt_regex_single(regex)?;
                regex.write_char('-').unwrap();
                range.end().fmt_regex_single(regex)
            }
        }
    }
}

impl Bracket {
    fn matches_multi_character(&self) -> bool {
        self.items.iter().any(BracketItem::matches_multi_character)
    }

    fn fmt_regex(&self, regex: &mut dyn Write) -> Result {
        if self.items.is_empty() {
            return Err(Error::EmptyBracket);
        }
        if !self.matches_multi_character() {
            regex.write_char('[').unwrap();
            if self.complement {
                regex.write_char('^').unwrap();
            }
            for item in &self.items {
                item.fmt_regex(regex)?;
            }
            regex.write_char(']').unwrap();
        } else if !self.complement {
            regex.write_str("(?:").unwrap();
            let mut first = true;
            for item in &self.items {
                if first {
                    first = false;
                } else {
                    regex.write_char('|').unwrap();
                }

                if !item.matches_multi_character() {
                    regex.write_char('[').unwrap();
                    item.fmt_regex(regex)?;
                    regex.write_char(']').unwrap();
                } else {
                    item.fmt_regex(regex)?;
                }
            }
            regex.write_char(')').unwrap();
        } else {
            regex.write_str("[^").unwrap();
            for item in &self.items {
                if !item.matches_multi_character() {
                    item.fmt_regex(regex)?;
                }
            }
            regex.write_char(']').unwrap();
        }
        Ok(())
    }
}

impl Atom {
    fn fmt_regex(&self, _config: &Config, regex: &mut dyn Write) -> Result {
        match self {
            Atom::Char(c) => {
                if SPECIAL_CHARS.contains(*c) {
                    regex.write_char('\\').unwrap();
                }
                regex.write_char(*c).unwrap();
            }
            Atom::AnyChar => regex.write_char('.').unwrap(),
            Atom::AnyString => regex.write_str(".*").unwrap(),
            Atom::Bracket(bracket) => bracket.fmt_regex(regex)?,
        }
        Ok(())
    }
}

impl Ast {
    /// Writes the AST as a regular expression.
    ///
    /// Only the `anchor_begin` and `anchor_end` options in `config` affect the
    /// results. The other options are ignored.
    pub fn fmt_regex(&self, config: &Config, regex: &mut dyn Write) -> Result {
        if config.anchor_begin {
            regex.write_str(r"\A").unwrap();
        }

        self.atoms
            .iter()
            .try_for_each(|atom| atom.fmt_regex(config, regex))?;

        if config.anchor_end {
            regex.write_str(r"\z").unwrap();
        }

        Ok(())
    }

    /// Converts the AST to a regular expression.
    ///
    /// Only the `anchor_begin` and `anchor_end` options in `config` affect the
    /// results. The other options are ignored.
    pub fn to_regex(&self, config: &Config) -> std::result::Result<String, Error> {
        let mut regex = String::new();
        self.fmt_regex(config, &mut regex)?;
        Ok(regex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

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
        assert_eq!(result, Err(Error::EmptyBracket));
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
                .chain(BRACKET_SPECIAL_CHARS.chars())
                .map(|c| BracketItem::Atom(BracketAtom::Char(c)))
                .collect(),
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, r"[\\\.\+\*\?\(\)\|\[\]\{\}\^\$\-\&\~]");
    }

    #[test]
    fn character_range() {
        let bracket = Bracket {
            complement: false,
            items: vec![
                BracketItem::Range(BracketAtom::Char('a')..=BracketAtom::Char('z')),
                BracketItem::Range(BracketAtom::Char('2')..=BracketAtom::Char('4')),
                BracketItem::Range(BracketAtom::Char('[')..=BracketAtom::Char(']')),
            ],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, r"[a-z2-4\[-\]]");

        let bracket = Bracket {
            complement: false,
            items: vec![
                BracketItem::Range(
                    BracketAtom::CollatingSymbol("A".to_string())
                        ..=BracketAtom::CollatingSymbol("Z".to_string()),
                ),
                BracketItem::Range(
                    BracketAtom::EquivalenceClass("3".to_string())
                        ..=BracketAtom::EquivalenceClass("5".to_string()),
                ),
            ],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "[A-Z3-5]");

        let bracket = Bracket {
            complement: false,
            items: vec![
                BracketItem::Range(
                    BracketAtom::CollatingSymbol("ch".to_string())
                        ..=BracketAtom::CollatingSymbol("ij".to_string()),
                ),
                BracketItem::Range(
                    BracketAtom::EquivalenceClass("a".to_string())
                        ..=BracketAtom::EquivalenceClass("s".to_string()),
                ),
            ],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "[c-ia-s]");
    }

    #[test]
    fn character_class_in_range() {
        let bracket = Bracket {
            complement: false,
            items: vec![BracketItem::Range(
                BracketAtom::CharClass("graph".to_string())..=BracketAtom::Char(' '),
            )],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let result = ast.to_regex(&Config::default());
        assert_eq!(result, Err(Error::CharClassInRange("graph".to_string())));

        let bracket = Bracket {
            complement: false,
            items: vec![BracketItem::Range(
                BracketAtom::Char('a')..=BracketAtom::CharClass("print".to_string()),
            )],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let result = ast.to_regex(&Config::default());
        assert_eq!(result, Err(Error::CharClassInRange("print".to_string())));
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
        assert_eq!(regex, "(?:ch)");
    }

    #[test]
    fn empty_collating_symbol_in_atom() {
        let bracket = Bracket {
            complement: false,
            items: vec![BracketItem::Atom(BracketAtom::CollatingSymbol(
                "".to_string(),
            ))],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let e = ast.to_regex(&Config::default()).unwrap_err();
        assert_eq!(e, Error::EmptyCollatingSymbol);
    }

    #[test]
    fn empty_collating_symbol_in_range() {
        let bracket = Bracket {
            complement: false,
            items: vec![BracketItem::Range(
                BracketAtom::CollatingSymbol("".to_string())..=BracketAtom::Char('x'),
            )],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let e = ast.to_regex(&Config::default()).unwrap_err();
        assert_eq!(e, Error::EmptyCollatingSymbol);
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
        assert_eq!(regex, "(?:ij)");
    }

    #[test]
    fn empty_equivalence_class_in_atom() {
        let bracket = Bracket {
            complement: false,
            items: vec![BracketItem::Atom(BracketAtom::EquivalenceClass(
                "".to_string(),
            ))],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let e = ast.to_regex(&Config::default()).unwrap_err();
        assert_eq!(e, Error::EmptyCollatingSymbol);
    }

    #[test]
    fn empty_equivalence_class_in_range() {
        let bracket = Bracket {
            complement: false,
            items: vec![BracketItem::Range(
                BracketAtom::EquivalenceClass("".to_string())..=BracketAtom::Char('x'),
            )],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let e = ast.to_regex(&Config::default()).unwrap_err();
        assert_eq!(e, Error::EmptyCollatingSymbol);
    }

    #[test]
    fn character_class() {
        let cases = [
            "alnum", "alpha", "ascii", "blank", "cntrl", "digit", "graph", "lower", "print",
            "punct", "space", "upper", "word", "xdigit",
        ];
        for class in cases {
            let bracket = Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::CharClass(class.to_string()))],
            };
            let atoms = vec![Atom::Bracket(bracket)];
            let ast = Ast { atoms };
            let regex = ast.to_regex(&Config::default()).unwrap();
            assert_eq!(regex, format!("[[:{class}:]]"));
        }
    }

    #[test]
    fn undefined_character_class() {
        let bracket = Bracket {
            complement: false,
            items: vec![BracketItem::Atom(BracketAtom::CharClass("xxx".to_string()))],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let e = ast.to_regex(&Config::default()).unwrap_err();
        assert_matches!(e, Error::UndefinedCharClass(class) if class == "xxx");
    }

    #[test]
    fn bracket_expression_complement() {
        let bracket = Bracket {
            complement: true,
            items: vec![
                BracketItem::Atom(BracketAtom::CollatingSymbol("s".to_string())),
                BracketItem::Atom(BracketAtom::Char('a')),
                BracketItem::Atom(BracketAtom::CharClass("digit".to_string())),
                BracketItem::Atom(BracketAtom::EquivalenceClass("x".to_string())),
            ],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "[^sa[:digit:]x]");
    }

    #[test]
    fn complex_bracket_expression() {
        let bracket = Bracket {
            complement: false,
            items: vec![
                BracketItem::Atom(BracketAtom::CollatingSymbol("ch".to_string())),
                BracketItem::Atom(BracketAtom::Char('a')),
                BracketItem::Atom(BracketAtom::CharClass("space".to_string())),
                BracketItem::Atom(BracketAtom::EquivalenceClass("ij".to_string())),
            ],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "(?:ch|[a]|[[:space:]]|ij)");
    }

    #[test]
    fn complex_bracket_expression_complement() {
        let bracket = Bracket {
            complement: true,
            items: vec![
                BracketItem::Atom(BracketAtom::CollatingSymbol("ch".to_string())),
                BracketItem::Atom(BracketAtom::Char('a')),
                BracketItem::Atom(BracketAtom::CharClass("space".to_string())),
                BracketItem::Atom(BracketAtom::EquivalenceClass("ij".to_string())),
            ],
        };
        let atoms = vec![Atom::Bracket(bracket)];
        let ast = Ast { atoms };
        let regex = ast.to_regex(&Config::default()).unwrap();
        assert_eq!(regex, "[^a[:space:]]");
    }

    #[test]
    fn anchor_begin() {
        let atoms = vec![Atom::Char('a'), Atom::AnyChar];
        let ast = Ast { atoms };
        let config = Config {
            anchor_begin: true,
            ..Config::default()
        };
        let regex = ast.to_regex(&config).unwrap();
        assert_eq!(regex, r"\Aa.");
    }

    #[test]
    fn anchor_end() {
        let atoms = vec![Atom::Char('a'), Atom::AnyChar];
        let ast = Ast { atoms };
        let config = Config {
            anchor_end: true,
            ..Config::default()
        };
        let regex = ast.to_regex(&config).unwrap();
        assert_eq!(regex, r"a.\z");
    }

    #[test]
    fn anchor_both() {
        let atoms = vec![Atom::Char('1'), Atom::AnyChar, Atom::Char('9')];
        let ast = Ast { atoms };
        let config = Config {
            anchor_begin: true,
            anchor_end: true,
            ..Config::default()
        };
        let regex = ast.to_regex(&config).unwrap();
        assert_eq!(regex, r"\A1.9\z");
    }
}
