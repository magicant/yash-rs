// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Symbolic notation
//!
//! This module defines data structures for representing symbolic notation of file
//! mode bits and provides functions for parsing and formatting symbolic notation.
//!
//! For the syntax of symbolic notation, see the
//! [documentation of the built-in](super).

use thiserror::Error;

/// Error [parsing clauses](parse_clauses)
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
pub enum ParseClausesError {
    /// There is an invalid character in the input.
    #[error("invalid character: {0:?}")]
    InvalidChar(char),
    /// A clause is invalid.
    #[error(transparent)]
    BadClause(#[from] ParseClauseError),
}

/// Parses a whole symbolic notation of the file mode creation mask, which is a
/// sequence of clauses separated by commas.
///
/// If successful, this function returns a vector of clauses. Otherwise, it
/// returns an error indicating the reason for the failure.
pub fn parse_clauses(mut s: &str) -> Result<Vec<Clause>, ParseClausesError> {
    let mut clauses = vec![Clause::parse(&mut s)?];
    while !s.is_empty() {
        if !s.starts_with(',') {
            return Err(ParseClausesError::InvalidChar(s.chars().next().unwrap()));
        }
        s = &s[1..];
        clauses.push(Clause::parse(&mut s)?);
    }
    Ok(clauses)
}

/// Clause in the symbolic notation of the file mode creation mask
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Clause {
    /// Selection of entities the permission applies to
    pub who: Who,
    /// Actions
    pub actions: Vec<Action>,
}

/// Error parsing a clause
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
#[error(transparent)]
pub enum ParseClauseError {
    /// There is no valid action.
    BadAction(#[from] ParseActionError),
}

impl Clause {
    /// Parses a clause from a string.
    ///
    /// This function parses a clause from a string and returns the parsed clause
    /// if successful. The argument is updated to the remaining unparsed part of
    /// the string.
    ///
    /// In case of an error, the argument is left in an unspecified state.
    pub fn parse(s: &mut &str) -> Result<Self, ParseClauseError> {
        let who = Who::parse(s);
        let mut actions = Vec::new();
        loop {
            match Action::parse(s) {
                Ok(action) => actions.push(action),
                Err(ParseActionError::NoOperator(_)) if !actions.is_empty() => {
                    return Ok(Self { who, actions })
                }
                Err(e) => return Err(ParseClauseError::BadAction(e)),
            }
        }
    }
}

/// Selection of entities the permission applies to
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct Who {
    /// Permission bit mask represented by the who symbols
    pub mask: u16,
}

impl std::fmt::Debug for Who {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO Use DebugStruct::field_with
        write!(f, "Who {{ mask: {:#05o} }}", self.mask)
    }
}

impl Who {
    /// Parses a who sequence from a string.
    ///
    /// This function parses a who sequence from a string and returns the parsed
    /// who sequence. The argument is updated to the remaining unparsed part of
    /// the string.
    pub fn parse(s: &mut &str) -> Self {
        let mut mask = 0;
        loop {
            let mut chars = s.chars();
            match chars.next() {
                Some('u') => mask |= 0o700,
                Some('g') => mask |= 0o070,
                Some('o') => mask |= 0o007,
                Some('a') => mask |= 0o777,
                _ => break,
            }
            *s = chars.as_str();
        }
        if mask == 0 {
            mask = 0o777;
        }
        Self { mask }
    }
}

/// Action in the symbolic notation of the file mode creation mask
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Action {
    /// Operator
    pub operator: Operator,
    /// Operand
    pub permission: Permission,
}

/// Error parsing an action
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
#[error(transparent)]
pub enum ParseActionError {
    /// There is no operator.
    NoOperator(#[from] ParseOperatorError),
    /// The permission is invalid.
    BadPermission(#[from] ParsePermissionError),
}

impl Action {
    /// Parses an action from a string.
    ///
    /// This function parses an action from a string and returns the parsed
    /// action if successful. The argument is updated to the remaining
    /// unparsed part of the string.
    ///
    /// In case of an error, the argument is left in an unspecified state.
    pub fn parse(s: &mut &str) -> Result<Self, ParseActionError> {
        let operator = Operator::parse(s)?;
        let permission = Permission::parse(s)?;
        Ok(Self {
            operator,
            permission,
        })
    }
}

/// Operator of an [`Action`]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Operator {
    /// Add the permission (**`+`**)
    Add,
    /// Remove the permission (**`-`**)
    Remove,
    /// Set the permission (**`=`**)
    Set,
}

/// Error parsing an operator
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
pub struct ParseOperatorError;

impl std::fmt::Display for ParseOperatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no operator")
    }
}

impl Operator {
    /// Parses an operator from a string.
    ///
    /// This function parses an operator from a string and returns the parsed
    /// operator if successful. The argument is updated to the remaining
    /// unparsed part of the string.
    ///
    /// In case of an error, the argument is left in an unspecified state.
    pub fn parse(s: &mut &str) -> Result<Self, ParseOperatorError> {
        let mut chars = s.chars();
        let operator = match chars.next() {
            Some('+') => Self::Add,
            Some('-') => Self::Remove,
            Some('=') => Self::Set,
            _ => return Err(ParseOperatorError),
        };
        *s = chars.as_str();
        Ok(operator)
    }
}

/// Operand of an [`Action`]
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub enum Permission {
    /// Dynamically evaluated to the current user permission (**`u`**)
    CopyUser,
    /// Dynamically evaluated to the current group permission (**`g`**)
    CopyGroup,
    /// Dynamically evaluated to the current other permission (**`o`**)
    CopyOther,
    /// Specifies permission by value
    Literal {
        /// Constant permission bit mask represented by a combination of
        /// **`r`**, **`w`**, and **`x`**
        mask: u16,
        /// True if the permission contains conditional executable bits (**`X`**)
        conditional_executable: bool,
    },
}

impl std::fmt::Debug for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO Use DebugStruct::field_with
        match self {
            Self::CopyUser => write!(f, "CopyUser"),
            Self::CopyGroup => write!(f, "CopyGroup"),
            Self::CopyOther => write!(f, "CopyOther"),
            Self::Literal {
                mask,
                conditional_executable,
            } => write!(
                f,
                "Literal {{ mask: {mask:#05o}, conditional_executable: {conditional_executable} }}",
            ),
        }
    }
}

/// Error parsing a permission
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
pub enum ParsePermissionError {
    /// Invalid combination of permission symbols
    ///
    /// This error occurs when one of `u`, `g`, and `o` is combined with another
    /// permission symbol.
    InvalidCombination(char, char),
}

impl std::fmt::Display for ParsePermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCombination(c1, c2) => {
                write!(
                    f,
                    "invalid combination of permission symbols: {c1:?} and {c2:?}",
                )
            }
        }
    }
}

impl Permission {
    /// Parses a permission from a string.
    ///
    /// This function parses a permission from a string and returns the parsed
    /// permission if successful. The argument is updated to the remaining
    /// unparsed part of the string.
    ///
    /// In case of an error, the argument is left in an unspecified state.
    pub fn parse(s: &mut &str) -> Result<Self, ParsePermissionError> {
        let alphabets_len = s
            .find(|c: char| !matches!(c, 'u' | 'g' | 'o' | 'r' | 'w' | 'x' | 'X' | 's'))
            .unwrap_or(s.len());
        let alphabets = &s[..alphabets_len];

        if let Some(index) = alphabets.find(['u', 'g', 'o']) {
            let copy = match alphabets {
                "u" => Permission::CopyUser,
                "g" => Permission::CopyGroup,
                "o" => Permission::CopyOther,
                _ => {
                    // We have checked all the single-letter cases above,
                    // so `alphabets` must contain at least two letters.
                    let mut chars = alphabets.chars();
                    let c1 = chars.next().unwrap();
                    let c2 = if index == 0 {
                        chars.next().unwrap()
                    } else {
                        alphabets[index..].chars().next().unwrap()
                    };
                    return Err(ParsePermissionError::InvalidCombination(c1, c2));
                }
            };
            *s = &s[1..];
            return Ok(copy);
        }

        let mut mask = 0;
        let mut conditional_executable = false;
        for c in alphabets.chars() {
            match c {
                'r' => mask |= 0o444,
                'w' => mask |= 0o222,
                'x' => mask |= 0o111,
                'X' => conditional_executable = true,
                's' => {} // TODO Support the `s` permission?
                _ => unreachable!(),
            }
        }

        *s = &s[alphabets_len..];
        Ok(Permission::Literal {
            mask,
            conditional_executable,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_single_clause() {
        let result = parse_clauses("u=");
        assert_eq!(
            result,
            Ok(vec![Clause {
                who: Who { mask: 0o700 },
                actions: vec![Action {
                    operator: Operator::Set,
                    permission: Permission::Literal {
                        mask: 0,
                        conditional_executable: false,
                    },
                }],
            }])
        );
    }

    #[test]
    fn parsing_multiple_clauses() {
        let result = parse_clauses("u+r,g-w,o=");
        assert_eq!(
            result,
            Ok(vec![
                Clause {
                    who: Who { mask: 0o700 },
                    actions: vec![Action {
                        operator: Operator::Add,
                        permission: Permission::Literal {
                            mask: 0o444,
                            conditional_executable: false,
                        },
                    }],
                },
                Clause {
                    who: Who { mask: 0o070 },
                    actions: vec![Action {
                        operator: Operator::Remove,
                        permission: Permission::Literal {
                            mask: 0o222,
                            conditional_executable: false,
                        },
                    }],
                },
                Clause {
                    who: Who { mask: 0o007 },
                    actions: vec![Action {
                        operator: Operator::Set,
                        permission: Permission::Literal {
                            mask: 0,
                            conditional_executable: false,
                        },
                    }],
                },
            ])
        );
    }

    #[test]
    fn parsing_invalid_clauses() {
        let result = parse_clauses("u-go");
        assert_eq!(
            result,
            Err(ParseClausesError::BadClause(ParseClauseError::BadAction(
                ParseActionError::BadPermission(ParsePermissionError::InvalidCombination('g', 'o'))
            )))
        );

        let result = parse_clauses("u+r,g-w,o");
        assert_eq!(
            result,
            Err(ParseClausesError::BadClause(ParseClauseError::BadAction(
                ParseActionError::NoOperator(ParseOperatorError)
            )))
        );
    }

    #[test]
    fn parsing_ill_separated_clauses() {
        let result = parse_clauses("u+r,g-w;o=");
        assert_eq!(result, Err(ParseClausesError::InvalidChar(';')));
    }
}

#[cfg(test)]
mod clause_tests {
    use super::*;

    #[test]
    fn parsing_minimum_clause() {
        let mut s = "=";
        let result = Clause::parse(&mut s);
        assert_eq!(
            result,
            Ok(Clause {
                who: Who { mask: 0o777 },
                actions: vec![Action {
                    operator: Operator::Set,
                    permission: Permission::Literal {
                        mask: 0,
                        conditional_executable: false,
                    },
                }],
            })
        );
        assert_eq!(s, "");
    }

    #[test]
    fn clause_with_nonempty_who() {
        let mut s = "u=";
        let result = Clause::parse(&mut s);
        assert_eq!(
            result,
            Ok(Clause {
                who: Who { mask: 0o700 },
                actions: vec![Action {
                    operator: Operator::Set,
                    permission: Permission::Literal {
                        mask: 0,
                        conditional_executable: false,
                    },
                }],
            })
        );
        assert_eq!(s, "");
    }

    #[test]
    fn clause_with_one_action() {
        let mut s = "u+w";
        let result = Clause::parse(&mut s);
        assert_eq!(
            result,
            Ok(Clause {
                who: Who { mask: 0o700 },
                actions: vec![Action {
                    operator: Operator::Add,
                    permission: Permission::Literal {
                        mask: 0o222,
                        conditional_executable: false,
                    },
                }],
            })
        );
    }

    #[test]
    fn clause_with_multiple_actions() {
        let mut s = "u-w+r,";
        let result = Clause::parse(&mut s);
        assert_eq!(
            result,
            Ok(Clause {
                who: Who { mask: 0o700 },
                actions: vec![
                    Action {
                        operator: Operator::Remove,
                        permission: Permission::Literal {
                            mask: 0o222,
                            conditional_executable: false,
                        },
                    },
                    Action {
                        operator: Operator::Add,
                        permission: Permission::Literal {
                            mask: 0o444,
                            conditional_executable: false,
                        },
                    },
                ],
            })
        );
        assert_eq!(s, ",");
    }

    #[test]
    fn clause_with_no_actions() {
        let mut s = "u";
        let result = Clause::parse(&mut s);
        assert_eq!(
            result,
            Err(ParseClauseError::BadAction(ParseActionError::NoOperator(
                ParseOperatorError
            )))
        );
    }

    #[test]
    fn clause_with_invalid_permission_combination() {
        let mut s = "+ug";
        let result = Clause::parse(&mut s);
        assert_eq!(
            result,
            Err(ParseClauseError::BadAction(
                ParseActionError::BadPermission(ParsePermissionError::InvalidCombination('u', 'g'))
            ))
        );
    }
}

#[cfg(test)]
mod who_tests {
    use super::*;

    #[test]
    fn parsing_single() {
        let mut s = "u";
        let result = Who::parse(&mut s);
        assert_eq!(result, Who { mask: 0o700 });
        assert_eq!(s, "");

        let mut s = "g+w";
        let result = Who::parse(&mut s);
        assert_eq!(result, Who { mask: 0o070 });
        assert_eq!(s, "+w");

        let mut s = "o";
        let result = Who::parse(&mut s);
        assert_eq!(result, Who { mask: 0o007 });
        assert_eq!(s, "");
    }

    #[test]
    fn parsing_all() {
        let mut s = "a";
        let result = Who::parse(&mut s);
        assert_eq!(result, Who { mask: 0o777 });
        assert_eq!(s, "");
    }

    #[test]
    fn parsing_multiple() {
        let mut s = "ug";
        let result = Who::parse(&mut s);
        assert_eq!(result, Who { mask: 0o770 });
        assert_eq!(s, "");

        let mut s = "go=";
        let result = Who::parse(&mut s);
        assert_eq!(result, Who { mask: 0o077 });
        assert_eq!(s, "=");
    }

    #[test]
    fn parsing_empty() {
        let mut s = "";
        let result = Who::parse(&mut s);
        assert_eq!(result, Who { mask: 0o777 });
        assert_eq!(s, "");
    }
}

#[cfg(test)]
mod action_tests {
    use super::*;

    #[test]
    fn parsing_empty() {
        let mut s = "";
        let result = Action::parse(&mut s);
        assert_eq!(
            result,
            Err(ParseActionError::NoOperator(ParseOperatorError))
        );
    }

    #[test]
    fn parsing_invalid_operator() {
        let mut s = "x";
        let result = Action::parse(&mut s);
        assert_eq!(
            result,
            Err(ParseActionError::NoOperator(ParseOperatorError))
        );
    }

    #[test]
    fn parsing_operator_with_empty_permission() {
        let mut s = "+";
        let result = Action::parse(&mut s);
        assert_eq!(
            result,
            Ok(Action {
                operator: Operator::Add,
                permission: Permission::Literal {
                    mask: 0,
                    conditional_executable: false,
                },
            })
        );
        assert_eq!(s, "");

        let mut s = "-+";
        let result = Action::parse(&mut s);
        assert_eq!(
            result,
            Ok(Action {
                operator: Operator::Remove,
                permission: Permission::Literal {
                    mask: 0,
                    conditional_executable: false,
                },
            })
        );
        assert_eq!(s, "+");
    }

    #[test]
    fn parsing_operator_with_nonempty_permission() {
        let mut s = "+r";
        let result = Action::parse(&mut s);
        assert_eq!(
            result,
            Ok(Action {
                operator: Operator::Add,
                permission: Permission::Literal {
                    mask: 0o444,
                    conditional_executable: false,
                },
            })
        );
        assert_eq!(s, "");

        let mut s = "-rXw=x";
        let result = Action::parse(&mut s);
        assert_eq!(
            result,
            Ok(Action {
                operator: Operator::Remove,
                permission: Permission::Literal {
                    mask: 0o666,
                    conditional_executable: true,
                },
            })
        );
        assert_eq!(s, "=x");
    }
}

#[cfg(test)]
mod operator_tests {
    use super::*;

    #[test]
    fn parsing_plus() {
        let mut s = "+";
        let result = Operator::parse(&mut s);
        assert_eq!(result, Ok(Operator::Add));
        assert_eq!(s, "");

        let mut s = "+r";
        let result = Operator::parse(&mut s);
        assert_eq!(result, Ok(Operator::Add));
        assert_eq!(s, "r");
    }

    #[test]
    fn parsing_minus() {
        let mut s = "-";
        let result = Operator::parse(&mut s);
        assert_eq!(result, Ok(Operator::Remove));
        assert_eq!(s, "");

        let mut s = "-w";
        let result = Operator::parse(&mut s);
        assert_eq!(result, Ok(Operator::Remove));
        assert_eq!(s, "w");
    }

    #[test]
    fn parsing_equal() {
        let mut s = "=";
        let result = Operator::parse(&mut s);
        assert_eq!(result, Ok(Operator::Set));
        assert_eq!(s, "");

        let mut s = "=x";
        let result = Operator::parse(&mut s);
        assert_eq!(result, Ok(Operator::Set));
        assert_eq!(s, "x");
    }

    #[test]
    fn parsing_non_operator() {
        let mut s = "";
        let result = Operator::parse(&mut s);
        assert_eq!(result, Err(ParseOperatorError));

        let mut s = "x";
        let result = Operator::parse(&mut s);
        assert_eq!(result, Err(ParseOperatorError));
    }
}

#[cfg(test)]
mod permission_tests {
    use super::*;

    #[test]
    fn parsing_empty() {
        let mut s = "";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Ok(Permission::Literal {
                mask: 0,
                conditional_executable: false,
            })
        );
        assert_eq!(s, "");

        let mut s = ",";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Ok(Permission::Literal {
                mask: 0,
                conditional_executable: false,
            })
        );
        assert_eq!(s, ",");
    }

    #[test]
    fn parsing_copy_user() {
        let mut s = "u";
        let result = Permission::parse(&mut s);
        assert_eq!(result, Ok(Permission::CopyUser));
        assert_eq!(s, "");

        let mut s = "u+g";
        let result = Permission::parse(&mut s);
        assert_eq!(result, Ok(Permission::CopyUser));
        assert_eq!(s, "+g");
    }

    #[test]
    fn parsing_copy_group() {
        let mut s = "g";
        let result = Permission::parse(&mut s);
        assert_eq!(result, Ok(Permission::CopyGroup));
        assert_eq!(s, "");

        let mut s = "g+o";
        let result = Permission::parse(&mut s);
        assert_eq!(result, Ok(Permission::CopyGroup));
        assert_eq!(s, "+o");
    }

    #[test]
    fn parsing_copy_other() {
        let mut s = "o";
        let result = Permission::parse(&mut s);
        assert_eq!(result, Ok(Permission::CopyOther));
        assert_eq!(s, "");

        let mut s = "o+u";
        let result = Permission::parse(&mut s);
        assert_eq!(result, Ok(Permission::CopyOther));
        assert_eq!(s, "+u");
    }

    #[test]
    fn parsing_literal_r() {
        let mut s = "r";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Ok(Permission::Literal {
                mask: 0o444,
                conditional_executable: false,
            })
        );
        assert_eq!(s, "");
    }

    #[test]
    fn parsing_literal_w() {
        let mut s = "w";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Ok(Permission::Literal {
                mask: 0o222,
                conditional_executable: false,
            })
        );
        assert_eq!(s, "");
    }

    #[test]
    fn parsing_literal_x() {
        let mut s = "x";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Ok(Permission::Literal {
                mask: 0o111,
                conditional_executable: false,
            })
        );
        assert_eq!(s, "");
    }

    #[test]
    fn parsing_literal_conditional_x() {
        let mut s = "X";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Ok(Permission::Literal {
                mask: 0,
                conditional_executable: true,
            })
        );
        assert_eq!(s, "");
    }

    #[test]
    fn parsing_literal_of_rwx_combination() {
        let mut s = "rw";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Ok(Permission::Literal {
                mask: 0o666,
                conditional_executable: false,
            })
        );
        assert_eq!(s, "");

        let mut s = "xr";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Ok(Permission::Literal {
                mask: 0o555,
                conditional_executable: false,
            })
        );
        assert_eq!(s, "");

        let mut s = "xwr-u";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Ok(Permission::Literal {
                mask: 0o777,
                conditional_executable: false,
            })
        );
        assert_eq!(s, "-u");
    }

    #[test]
    fn parsing_literal_s() {
        // The current implementation ignores the `s` permission.
        let mut s = "s";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Ok(Permission::Literal {
                mask: 0,
                conditional_executable: false,
            })
        );
        assert_eq!(s, "");
    }

    #[test]
    fn copy_cannot_be_combined_with_literal() {
        let mut s = "ur";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Err(ParsePermissionError::InvalidCombination('u', 'r'))
        );

        let mut s = "ru";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Err(ParsePermissionError::InvalidCombination('r', 'u'))
        );
    }

    #[test]
    fn copy_cannot_be_combined_with_copy() {
        let mut s = "ug";
        let result = Permission::parse(&mut s);
        assert_eq!(
            result,
            Err(ParsePermissionError::InvalidCombination('u', 'g'))
        );
    }
}
