// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! AST parser

use super::*;
use crate::PatternChar;

impl BracketAtom {
    /// Parses an inner bracket expression (except the initial '[').
    ///
    /// This function parses a collating symbol, equivalence class, or character
    /// class.
    ///
    /// If successful, returns the result as well as an iterator that yields
    /// characters following the closing bracket. Returns `Ok(None)` if the
    /// inner bracket expression is not valid.
    fn parse_inner<I>(mut i: I) -> Option<(Self, I)>
    where
        I: Iterator<Item = PatternChar>,
    {
        match i.next() {
            Some(PatternChar::Normal('.')) => {
                let mut value = Vec::new();
                while let Some(pc) = i.next() {
                    value.push(pc);
                    if value.ends_with(&[PatternChar::Normal('.'), PatternChar::Normal(']')]) {
                        value.truncate(value.len() - 2);
                        let value = value.into_iter().map(PatternChar::char_value).collect();
                        return Some((BracketAtom::CollatingSymbol(value), i));
                    }
                }
            }
            Some(PatternChar::Normal('=')) => {
                let mut value = Vec::new();
                while let Some(pc) = i.next() {
                    value.push(pc);
                    if value.ends_with(&[PatternChar::Normal('='), PatternChar::Normal(']')]) {
                        value.truncate(value.len() - 2);
                        let value = value.into_iter().map(PatternChar::char_value).collect();
                        return Some((BracketAtom::EquivalenceClass(value), i));
                    }
                }
            }
            Some(PatternChar::Normal(':')) => {
                let mut value = Vec::new();
                while let Some(pc) = i.next() {
                    value.push(pc);
                    if value.ends_with(&[PatternChar::Normal(':'), PatternChar::Normal(']')]) {
                        value.truncate(value.len() - 2);
                        let class = value.into_iter().map(PatternChar::char_value).collect();
                        return Some((BracketAtom::CharClass(class), i));
                    }
                }
            }
            _ => (),
        }
        None
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

impl Bracket {
    /// Parses a bracket expression (except the initial '[').
    ///
    /// If successful, returns the result as well as an iterator that yields
    /// characters following the bracket expression. Returns `Ok(None)` if a
    /// bracket expression is not found.
    fn parse<I>(mut i: I) -> Option<(Self, I)>
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
                PatternChar::Normal(']') if !bracket.items.is_empty() => return Some((bracket, i)),
                PatternChar::Normal('!' | '^')
                    if !bracket.complement && bracket.items.is_empty() =>
                {
                    bracket.complement = true
                }
                PatternChar::Normal('[') => {
                    match BracketAtom::parse_inner(i.clone()) { Some((atom, j)) => {
                        bracket.items.push(atom.into());
                        i = j;
                    } _ => {
                        bracket.items.push(Atom(Char('[')));
                    }}
                }
                c => bracket.items.push(Atom(Char(c.char_value()))),
            }
            make_range(&mut bracket.items);
        }
        None
    }
}

impl Atom {
    pub(crate) fn parse<I>(mut i: I) -> Option<(Self, I)>
    where
        I: Iterator<Item = PatternChar> + Clone,
    {
        i.next().map(|pc| {
            let atom = match pc {
                PatternChar::Normal('?') => Atom::AnyChar,
                PatternChar::Normal('*') => Atom::AnyString,
                PatternChar::Normal('[') => {
                    match Bracket::parse(i.clone()) { Some((bracket, j)) => {
                        i = j;
                        Atom::Bracket(bracket)
                    } _ => {
                        Atom::Char('[')
                    }}
                }
                c => Atom::Char(c.char_value()),
            };
            (atom, i)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::with_escape;
    use crate::without_escape;

    #[test]
    fn empty_pattern() {
        let ast = Ast::new(without_escape(""));
        assert_eq!(ast.atoms, []);
    }

    #[test]
    fn single_character_pattern() {
        let ast = Ast::new(without_escape("a"));
        assert_eq!(ast.atoms, [Atom::Char('a')]);

        let ast = Ast::new(without_escape("0"));
        assert_eq!(ast.atoms, [Atom::Char('0')]);
    }

    #[test]
    fn double_character_pattern() {
        let ast = Ast::new(without_escape("in"));
        assert_eq!(ast.atoms, [Atom::Char('i'), Atom::Char('n')]);
    }

    #[test]
    fn any_character_pattern() {
        let ast = Ast::new(without_escape("?"));
        assert_eq!(ast.atoms, [Atom::AnyChar]);
    }

    #[test]
    fn any_string_pattern() {
        let ast = Ast::new(without_escape("*"));
        assert_eq!(ast.atoms, [Atom::AnyString]);
    }

    #[test]
    fn escaped_any_patterns() {
        let ast = Ast::new(with_escape(r"\?\*"));
        assert_eq!(ast.atoms, [Atom::Char('?'), Atom::Char('*')]);
    }

    #[test]
    fn empty_bracket_expression() {
        let ast = Ast::new(without_escape("[]"));
        assert_eq!(ast.atoms, [Atom::Char('['), Atom::Char(']')]);
    }

    #[test]
    fn escaped_bracket_expression() {
        let ast = Ast::new(with_escape(r"\[a]"));
        assert_eq!(
            ast.atoms,
            [Atom::Char('['), Atom::Char('a'), Atom::Char(']')]
        );

        let ast = Ast::new(with_escape(r"[a\]"));
        assert_eq!(
            ast.atoms,
            [Atom::Char('['), Atom::Char('a'), Atom::Char(']')]
        );
    }

    #[test]
    fn single_character_bracket_expression_pattern() {
        let ast = Ast::new(without_escape("[a]"));
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
        let ast = Ast::new(without_escape("[xyz]"));
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
        let ast = Ast::new(without_escape("[]a[]"));
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
        let ast = Ast::new(without_escape("[!12]"));
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
        let ast = Ast::new(without_escape("[12!]"));
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
        let ast = Ast::new(without_escape("[!!]"));
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
        let ast = Ast::new(without_escape("[^34]"));
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

        let ast = Ast::new(without_escape("[^]a]"));
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

        let ast = Ast::new(without_escape("[^^]"));
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
        let ast = Ast::new(without_escape("[a-z]"));
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
        let ast = Ast::new(without_escape("[-a]"));
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

        let ast = Ast::new(without_escape("[!-b]"));
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
        let ast = Ast::new(without_escape("[5-]"));
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
        let ast = Ast::new(without_escape("[2-4-6-8]"));
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
        let ast = Ast::new(without_escape("[--0]"));
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
        let ast = Ast::new(without_escape("[+--]"));
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
        let ast = Ast::new(with_escape(r"[\!\[.a.]]"));
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
        let ast = Ast::new(without_escape("[[.a.]]"));
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::CollatingSymbol(
                    "a".to_string()
                ))]
            })]
        );

        let ast = Ast::new(without_escape("[[.].]]"));
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
        let ast = Ast::new(without_escape("[[.ch.]]"));
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
        let ast = Ast::new(with_escape(r"[[\.a.]]"));
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
                Atom::Char(']'),
            ]
        );

        let ast = Ast::new(with_escape(r"[[.a\.]]"));
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
                Atom::Char(']'),
            ]
        );

        let ast = Ast::new(with_escape(r"[[.a.\]]"));
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('[')),
                    BracketItem::Atom(BracketAtom::Char('.')),
                    BracketItem::Atom(BracketAtom::Char('a')),
                    BracketItem::Atom(BracketAtom::Char('.')),
                    BracketItem::Atom(BracketAtom::Char(']')),
                ]
            })]
        );
    }

    #[test]
    fn single_character_equivalence_class() {
        let ast = Ast::new(without_escape("[[=a=]]"));
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![BracketItem::Atom(BracketAtom::EquivalenceClass(
                    "a".to_string()
                ))]
            })]
        );

        let ast = Ast::new(without_escape("[[=]=]]"));
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
        let ast = Ast::new(without_escape("[[=ch=]]"));
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
        let ast = Ast::new(with_escape(r"[[\=a=]]"));
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
                Atom::Char(']'),
            ]
        );

        let ast = Ast::new(with_escape(r"[[=a\=]]"));
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
                Atom::Char(']'),
            ]
        );

        let ast = Ast::new(with_escape(r"[[=a=\]]"));
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
                complement: false,
                items: vec![
                    BracketItem::Atom(BracketAtom::Char('[')),
                    BracketItem::Atom(BracketAtom::Char('=')),
                    BracketItem::Atom(BracketAtom::Char('a')),
                    BracketItem::Atom(BracketAtom::Char('=')),
                    BracketItem::Atom(BracketAtom::Char(']')),
                ]
            }),]
        );
    }

    #[test]
    fn character_classes() {
        let cases = [
            "alnum", "alpha", "ascii", "blank", "cntrl", "digit", "graph", "lower", "print",
            "punct", "space", "upper", "word", "xdigit",
        ];
        for class in cases {
            let pattern = format!("[[:{class}:]]");
            let ast = Ast::new(without_escape(&pattern));
            assert_eq!(
                ast.atoms,
                [Atom::Bracket(Bracket {
                    complement: false,
                    items: vec![BracketItem::Atom(BracketAtom::CharClass(class.to_string()))]
                })]
            );
        }
    }

    #[test]
    fn escapes_in_character_class() {
        let ast = Ast::new(with_escape(r"[[\:alpha:]]"));
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
                Atom::Char(']'),
            ]
        );

        let ast = Ast::new(with_escape(r"[[:alpha\:]]"));
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
                Atom::Char(']'),
            ]
        );

        let ast = Ast::new(with_escape(r"[[:alpha:\]]"));
        assert_eq!(
            ast.atoms,
            [Atom::Bracket(Bracket {
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
                    BracketItem::Atom(BracketAtom::Char(']')),
                ]
            })]
        );
    }

    #[test]
    fn inner_brackets_in_character_range() {
        let ast = Ast::new(without_escape("[[.ch.]-[=x=]]"));
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
}
