// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! Abstract syntax tree for globbing patterns

use crate::Config;
use crate::Error;
use crate::PatternChar;
use regex_syntax::ast::ClassAsciiKind;
use std::ops::RangeInclusive;

/// Bracket expression component
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BracketAtom {
    /// Literal character
    Char(char),
    /// Collating symbol (`[.x.]`)
    CollatingSymbol(String),
    /// Equivalence Class (`[=x=]`)
    EquivalenceClass(String),
    /// Character class (`[:digit:]`)
    CharClass(ClassAsciiKind),
}

impl From<char> for BracketAtom {
    fn from(c: char) -> Self {
        BracketAtom::Char(c)
    }
}

impl BracketAtom {
    /// Parses an inner bracket expression (except the initial '[').
    ///
    /// This function parses a collating symbol, equivalence class, or character
    /// class.
    ///
    /// If successful, returns the result as well as an iterator that yields
    /// characters following the closing bracket. Returns `Ok(None)` if the
    /// inner bracket expression is not valid.
    fn parse_inner<I>(mut i: I) -> Result<Option<(Self, I)>, Error>
    where
        I: Iterator<Item = PatternChar>,
    {
        match i.next() {
            Some(PatternChar::Normal('.')) => {
                let mut value = String::new();
                while let Some(pc) = i.next() {
                    value.push(pc.char_value());
                    if value.ends_with(".]") {
                        value.truncate(value.len() - 2);
                        return Ok(Some((BracketAtom::CollatingSymbol(value), i)));
                    }
                }
                Ok(None)
            }
            Some(PatternChar::Normal('=')) => {
                let mut value = String::new();
                while let Some(pc) = i.next() {
                    value.push(pc.char_value());
                    if value.ends_with("=]") {
                        value.truncate(value.len() - 2);
                        return Ok(Some((BracketAtom::EquivalenceClass(value), i)));
                    }
                }
                Ok(None)
            }
            Some(PatternChar::Normal(':')) => {
                let mut name = String::new();
                while let Some(pc) = i.next() {
                    name.push(pc.char_value());
                    if name.ends_with(":]") {
                        name.truncate(name.len() - 2);
                        return if let Some(class) = ClassAsciiKind::from_name(&name) {
                            Ok(Some((BracketAtom::CharClass(class), i)))
                        } else {
                            Err(Error::UndefinedCharacterClass(name))
                        };
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}

/// Bracket expression component
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BracketItem {
    /// Atom
    Atom(BracketAtom),
    /// Character range
    Range(RangeInclusive<BracketAtom>),
}

impl<T: Into<BracketAtom>> From<T> for BracketItem {
    fn from(value: T) -> Self {
        BracketItem::Atom(value.into())
    }
}
impl From<RangeInclusive<BracketAtom>> for BracketItem {
    fn from(range: RangeInclusive<BracketAtom>) -> Self {
        BracketItem::Range(range)
    }
}

/// Converts the last three items into a range if applicable.
fn make_range(items: &mut Vec<BracketItem>) {
    use BracketAtom::*;
    use BracketItem::*;

    if let Some(i1) = items.pop() {
        if let Atom(end) = i1 {
            if let Some(i2) = items.pop() {
                if let Atom(Char('-')) = i2 {
                    if let Some(i3) = items.pop() {
                        if let Atom(start) = i3 {
                            items.push(Range(start..=end));
                            return;
                        }
                        items.push(i3);
                    }
                }
                items.push(i2);
            }
            items.push(Atom(end));
        } else {
            items.push(i1);
        }
    }
}

/// Bracket expression
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bracket {
    /// Whether there is an initial `!`
    ///
    /// When the bracket expression starts with an `!`, the set of matching
    /// character is inverted.
    pub complement: bool,

    /// Content of the bracket expression
    pub items: Vec<BracketItem>,
}

impl Bracket {
    /// Parses a bracket expression (except the initial '[').
    ///
    /// If successful, returns the result as well as an iterator that yields
    /// characters following the bracket expression. Returns `Ok(None)` if a
    /// bracket expression is not found.
    fn parse<I>(mut i: I, _config: Config) -> Result<Option<(Self, I)>, Error>
    where
        I: Iterator<Item = PatternChar> + Clone,
    {
        use BracketAtom::*;
        use BracketItem::*;

        let mut bracket = Bracket {
            complement: false,
            items: Vec::new(),
        };
        while let Some(pc) = i.next() {
            match pc {
                PatternChar::Normal(']') if !bracket.items.is_empty() => {
                    return Ok(Some((bracket, i)))
                }
                PatternChar::Normal('!' | '^')
                    if !bracket.complement && bracket.items.is_empty() =>
                {
                    bracket.complement = true
                }
                PatternChar::Normal('[') => {
                    if let Some((atom, j)) = BracketAtom::parse_inner(i.clone())? {
                        bracket.items.push(atom.into());
                        i = j;
                    } else {
                        bracket.items.push(Atom(Char('[')));
                    }
                }
                c => bracket.items.push(Atom(Char(c.char_value()))),
            }
            make_range(&mut bracket.items);
        }
        Ok(None)
    }
}

/// Pattern component
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Atom {
    /// Literal character
    Char(char),
    /// Pattern that matches a single character (`?`)
    AnyChar,
    /// Pattern that matches any string (`*`)
    AnyString,
    /// Bracket expression
    Bracket(Bracket),
}

impl Atom {
    fn parse<I>(mut i: I, config: Config) -> Result<Option<(Self, I)>, Error>
    where
        I: Iterator<Item = PatternChar> + Clone,
    {
        if let Some(pc) = i.next() {
            let atom = match pc {
                PatternChar::Normal('?') => Atom::AnyChar,
                PatternChar::Normal('*') => Atom::AnyString,
                PatternChar::Normal('[') => {
                    if let Some((bracket, j)) = Bracket::parse(i.clone(), config)? {
                        i = j;
                        Atom::Bracket(bracket)
                    } else {
                        Atom::Char('[')
                    }
                }
                c => Atom::Char(c.char_value()),
            };
            Ok(Some((atom, i)))
        } else {
            Ok(None)
        }
    }
}

/// Abstract syntax tree for a whole pattern
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Ast {
    /// Content of the pattern
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
    pub fn with_config<I>(pattern: I, config: Config) -> Result<Self, Error>
    where
        I: IntoIterator<Item = PatternChar>,
        <I as IntoIterator>::IntoIter: Clone,
    {
        let mut atoms = Vec::new();
        let mut i = pattern.into_iter();
        while let Some((atom, j)) = Atom::parse(i, config)? {
            atoms.push(atom);
            i = j;
        }
        Ok(Ast { atoms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::with_escape;
    use crate::without_escape;
    use assert_matches::assert_matches;

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

    #[test]
    fn double_character_pattern() {
        let ast = Ast::new(without_escape("in")).unwrap();
        assert_eq!(ast.atoms, [Atom::Char('i'), Atom::Char('n')]);
    }

    #[test]
    fn any_character_pattern() {
        let ast = Ast::new(without_escape("?")).unwrap();
        assert_eq!(ast.atoms, [Atom::AnyChar]);
    }

    #[test]
    fn any_string_pattern() {
        let ast = Ast::new(without_escape("*")).unwrap();
        assert_eq!(ast.atoms, [Atom::AnyString]);
    }

    #[test]
    fn escaped_any_patterns() {
        let ast = Ast::new(with_escape(r"\?\*")).unwrap();
        assert_eq!(ast.atoms, [Atom::Char('?'), Atom::Char('*')]);
    }

    // TODO unmatched bracket [a
    // TODO unmatched bracket a]
    // TODO unmatched bracket ][

    #[test]
    fn empty_bracket_expression() {
        let ast = Ast::new(without_escape("[]")).unwrap();
        assert_eq!(ast.atoms, [Atom::Char('['), Atom::Char(']')]);
    }

    #[test]
    fn escaped_bracket_expression() {
        let ast = Ast::new(with_escape(r"\[a]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Char('['), Atom::Char('a'), Atom::Char(']')]
        );

        let ast = Ast::new(with_escape(r"[a\]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Char('['), Atom::Char('a'), Atom::Char(']')]
        );
    }

    // TODO unmatched bracket after another bracket [a][a

    #[test]
    fn single_character_bracket_expression_pattern() {
        let ast = Ast::new(without_escape("[a]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::Char('a'))]
            })]
        );
    }

    #[test]
    fn multi_character_bracket_expression_pattern() {
        let ast = Ast::new(without_escape("[xyz]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('x')),
                    BracketItem::Atom(BracketAtom::Char('y')),
                    BracketItem::Atom(BracketAtom::Char('z')),
                ]
            })]
        );
    }

    #[test]
    fn brackets_in_bracket_expression() {
        let ast = Ast::new(without_escape("[]a[]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char(']')),
                    BracketItem::Atom(BracketAtom::Char('a')),
                    BracketItem::Atom(BracketAtom::Char('[')),
                ]
            })]
        );
    }

    #[test]
    fn bracket_expression_complement_with_exclamation() {
        let ast = Ast::new(without_escape("[!12]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('1')),
                    BracketItem::Atom(BracketAtom::Char('2')),
                ]
            })]
        );
    }

    #[test]
    fn exclamation_in_bracket_expression() {
        let ast = Ast::new(without_escape("[12!]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('1')),
                    BracketItem::Atom(BracketAtom::Char('2')),
                    BracketItem::Atom(BracketAtom::Char('!')),
                ]
            })]
        );
    }

    #[test]
    fn exclamation_in_bracket_expression_complement() {
        let ast = Ast::new(without_escape("[!!]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![BracketItem::Atom(BracketAtom::Char('!'))]
            })]
        );
    }

    #[test]
    fn bracket_expression_complement_with_caret() {
        let ast = Ast::new(without_escape("[^34]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('3')),
                    BracketItem::Atom(BracketAtom::Char('4')),
                ]
            })]
        );

        let ast = Ast::new(without_escape("[^]a]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char(']')),
                    BracketItem::Atom(BracketAtom::Char('a')),
                ]
            })]
        );

        let ast = Ast::new(without_escape("[^^]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![BracketItem::Atom(BracketAtom::Char('^'))]
            })]
        );
    }

    #[test]
    fn character_range() {
        let ast = Ast::new(without_escape("[a-z]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Range(
                    BracketAtom::Char('a')..=BracketAtom::Char('z')
                )]
            })]
        );
    }

    #[test]
    fn dash_at_start_of_bracket_expression() {
        let ast = Ast::new(without_escape("[-a]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('-')),
                    BracketItem::Atom(BracketAtom::Char('a')),
                ]
            })]
        );

        let ast = Ast::new(without_escape("[!-b]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: true,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('-')),
                    BracketItem::Atom(BracketAtom::Char('b')),
                ]
            })]
        );
    }

    #[test]
    fn dash_at_end_of_bracket_expression() {
        let ast = Ast::new(without_escape("[5-]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('5')),
                    BracketItem::Atom(BracketAtom::Char('-')),
                ]
            })]
        );
    }

    #[test]
    fn ambiguous_character_range() {
        let ast = Ast::new(without_escape("[2-4-6-8]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Range(BracketAtom::Char('2')..=BracketAtom::Char('4')),
                    BracketItem::Atom(BracketAtom::Char('-')),
                    BracketItem::Range(BracketAtom::Char('6')..=BracketAtom::Char('8')),
                ]
            })]
        );
    }

    #[test]
    fn double_dash_at_start_of_bracket_expression() {
        let ast = Ast::new(without_escape("[--0]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Range(
                    BracketAtom::Char('-')..=BracketAtom::Char('0')
                )]
            })]
        );
    }

    #[test]
    fn double_dash_at_end_of_bracket_expression() {
        let ast = Ast::new(without_escape("[+--]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Range(
                    BracketAtom::Char('+')..=BracketAtom::Char('-')
                )]
            })]
        );
    }

    #[test]
    fn escapes_in_bracket_expression() {
        let ast = Ast::new(with_escape(r"[\!\[.a.]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [
                Atom::Bracket(Bracket {
                    complement: false,
                    items: vec![
                        BracketItem::Atom(BracketAtom::Char('!')),
                        BracketItem::Atom(BracketAtom::Char('[')),
                        BracketItem::Atom(BracketAtom::Char('.')),
                        BracketItem::Atom(BracketAtom::Char('a')),
                        BracketItem::Atom(BracketAtom::Char('.')),
                    ]
                }),
                Atom::Char(']')
            ]
        );
    }

    #[test]
    fn single_character_collating_symbol() {
        let ast = Ast::new(without_escape("[[.a.]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::CollatingSymbol(
                    "a".to_string()
                ))]
            })]
        );

        let ast = Ast::new(without_escape("[[.].]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::CollatingSymbol(
                    "]".to_string()
                ))]
            })]
        );
    }

    #[test]
    fn multi_character_collating_symbol() {
        let ast = Ast::new(without_escape("[[.ch.]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::CollatingSymbol(
                    "ch".to_string()
                ))]
            })]
        );
    }

    #[test]
    fn escapes_in_collating_symbol() {
        let ast = Ast::new(with_escape(r"[[\.a.]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [
                Atom::Bracket(Bracket {
                    complement: false,
                    items: vec![
                        BracketItem::Atom(BracketAtom::Char('[')),
                        BracketItem::Atom(BracketAtom::Char('.')),
                        BracketItem::Atom(BracketAtom::Char('a')),
                        BracketItem::Atom(BracketAtom::Char('.')),
                    ]
                }),
                Atom::Char(']')
            ]
        );

        // TODO [[.a\.]]
        // TODO [[.a.\]]
    }

    #[test]
    fn single_character_equivalence_class() {
        let ast = Ast::new(without_escape("[[=a=]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::EquivalenceClass(
                    "a".to_string()
                ))]
            })]
        );

        let ast = Ast::new(without_escape("[[=]=]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::EquivalenceClass(
                    "]".to_string()
                ))]
            })]
        );
    }

    #[test]
    fn multi_character_equivalence_class() {
        let ast = Ast::new(without_escape("[[=ch=]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::EquivalenceClass(
                    "ch".to_string()
                ))]
            })]
        );
    }

    #[test]
    fn escapes_in_equivalence_class() {
        let ast = Ast::new(with_escape(r"[[\=a=]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [
                Atom::Bracket(Bracket {
                    complement: false,
                    items: vec![
                        BracketItem::Atom(BracketAtom::Char('[')),
                        BracketItem::Atom(BracketAtom::Char('=')),
                        BracketItem::Atom(BracketAtom::Char('a')),
                        BracketItem::Atom(BracketAtom::Char('=')),
                    ]
                }),
                Atom::Char(']')
            ]
        );

        // TODO [[=a\=]]
        // TODO [[=a=\]]
    }

    #[test]
    fn character_classes() {
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
        for (name, kind) in cases {
            let pattern = format!("[[:{name}:]]");
            let ast = Ast::new(without_escape(&pattern)).unwrap();
            assert_eq!(
                ast.atoms,
                [Atom::Bracket(Bracket {
                    complement: false,
                    items: vec![BracketItem::Atom(BracketAtom::CharClass(kind))]
                })]
            );
        }
    }

    #[test]
    fn undefined_character_class() {
        let e = Ast::new(without_escape("[[:foo_bar:]]")).unwrap_err();
        assert_matches!(e, Error::UndefinedCharacterClass(name) if name == "foo_bar");
    }

    #[test]
    fn escapes_in_character_class() {
        let ast = Ast::new(with_escape(r"[[\:alpha:]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [
                Atom::Bracket(Bracket {
                    complement: false,
                    items: vec![
                        BracketItem::Atom(BracketAtom::Char('[')),
                        BracketItem::Atom(BracketAtom::Char(':')),
                        BracketItem::Atom(BracketAtom::Char('a')),
                        BracketItem::Atom(BracketAtom::Char('l')),
                        BracketItem::Atom(BracketAtom::Char('p')),
                        BracketItem::Atom(BracketAtom::Char('h')),
                        BracketItem::Atom(BracketAtom::Char('a')),
                        BracketItem::Atom(BracketAtom::Char(':')),
                    ]
                }),
                Atom::Char(']')
            ]
        );

        // TODO [[=a\=]]
        // TODO [[=a=\]]
    }

    #[test]
    fn inner_brackets_in_character_range() {
        let ast = Ast::new(without_escape("[[.ch.]-[=x=]]")).unwrap();
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Range(
                    BracketAtom::CollatingSymbol("ch".to_string())
                        ..=BracketAtom::EquivalenceClass("x".to_string())
                )]
            })]
        );
    }

    // TODO Config
    // TODO PatternChar Normal vs Literal
}
