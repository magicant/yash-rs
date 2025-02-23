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

//! Computation of the new file mode creation mask.
//!
//! This module contains a function that computes a new file mode creation mask
//! from the current mask and a command. It is part of the implementation of the
//! `umask` built-in. (See [`Command::execute`].)

use super::Command;
use super::symbol::{Operator, Permission};

/// Computes a mask to be set.
///
/// This function applies the given command to the current mask and returns the
/// result. The current mask and the result are both given as negative bits of
/// the file mode creation mask.
#[must_use]
pub fn new_mask(current: u16, command: &Command) -> u16 {
    match command {
        Command::Show { .. } => current,

        Command::Set(clauses) => {
            let mut result = current;
            for clause in clauses {
                for action in &clause.actions {
                    let resolution = match action.permission {
                        Permission::CopyUser => copy(current >> 6),
                        Permission::CopyGroup => copy(current >> 3),
                        Permission::CopyOther => copy(current),
                        Permission::Literal {
                            mask,
                            conditional_executable,
                        } => {
                            let add_x = conditional_executable && current & 0o111 != 0;
                            mask | (if add_x { 0o111 } else { 0 })
                        }
                    };
                    let who = clause.who.mask;
                    result = match action.operator {
                        Operator::Add => (resolution & who) | result,
                        Operator::Remove => !(resolution & who) & result,
                        Operator::Set => (resolution & who) | (result & !who),
                    };
                }
            }
            result
        }
    }
}

fn copy(mask: u16) -> u16 {
    let mask = mask & 0o7;
    (mask << 6) | (mask << 3) | mask
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::umask::symbol::{Action, Clause, Who};

    #[test]
    fn new_mask_for_show() {
        let result = new_mask(0o766, &Command::Show { symbolic: false });
        assert_eq!(result, 0o766);
    }

    #[test]
    fn new_mask_all_set_literal() {
        let command = Command::Set(vec![Clause {
            who: Who { mask: 0o777 },
            actions: vec![Action {
                operator: Operator::Set,
                permission: Permission::Literal {
                    mask: 0o635,
                    conditional_executable: false,
                },
            }],
        }]);
        let result = new_mask(0o766, &command);
        assert_eq!(result, 0o635);
    }

    #[test]
    fn new_mask_user_set_literal() {
        let command = Command::Set(vec![Clause {
            who: Who { mask: 0o700 },
            actions: vec![Action {
                operator: Operator::Set,
                permission: Permission::Literal {
                    mask: 0o635,
                    conditional_executable: false,
                },
            }],
        }]);
        let result = new_mask(0o766, &command);
        assert_eq!(result, 0o666);
    }

    #[test]
    fn new_mask_group_set_literal() {
        let command = Command::Set(vec![Clause {
            who: Who { mask: 0o070 },
            actions: vec![Action {
                operator: Operator::Set,
                permission: Permission::Literal {
                    mask: 0o635,
                    conditional_executable: false,
                },
            }],
        }]);
        let result = new_mask(0o766, &command);
        assert_eq!(result, 0o736);
    }

    #[test]
    fn new_mask_other_set_literal() {
        let command = Command::Set(vec![Clause {
            who: Who { mask: 0o007 },
            actions: vec![Action {
                operator: Operator::Set,
                permission: Permission::Literal {
                    mask: 0o635,
                    conditional_executable: false,
                },
            }],
        }]);
        let result = new_mask(0o766, &command);
        assert_eq!(result, 0o765);
    }

    #[test]
    fn new_mask_set_conditional_without_initial_x() {
        // This test case starts with a mask that does not have any executable
        // bits set. The first clause sets the executable bit for the user, but
        // that does not affect the second clause because the conditional
        // executable bit only takes the initial state into account.
        let command = Command::Set(vec![
            Clause {
                who: Who { mask: 0o700 },
                actions: vec![Action {
                    operator: Operator::Add,
                    permission: Permission::Literal {
                        mask: 0o111,
                        conditional_executable: false,
                    },
                }],
            },
            Clause {
                who: Who { mask: 0o007 },
                actions: vec![Action {
                    operator: Operator::Add,
                    permission: Permission::Literal {
                        mask: 0o000,
                        conditional_executable: true,
                    },
                }],
            },
        ]);
        let result = new_mask(0o660, &command);
        assert_eq!(result, 0o760);
    }

    #[test]
    fn new_mask_set_conditional_with_initial_x() {
        // In this test case, we have the executable bit set for the user in the
        // initial mask. The conditional executable bit affects the final
        // result.
        let command = Command::Set(vec![Clause {
            who: Who { mask: 0o007 },
            actions: vec![Action {
                operator: Operator::Add,
                permission: Permission::Literal {
                    mask: 0o000,
                    conditional_executable: true,
                },
            }],
        }]);
        let result = new_mask(0o760, &command);
        assert_eq!(result, 0o761);
    }

    #[test]
    fn new_mask_set_copy_user() {
        let command = Command::Set(vec![Clause {
            who: Who { mask: 0o777 },
            actions: vec![Action {
                operator: Operator::Set,
                permission: Permission::CopyUser,
            }],
        }]);
        let result = new_mask(0o650, &command);
        assert_eq!(result, 0o666);
    }

    #[test]
    fn new_mask_set_copy_group() {
        let command = Command::Set(vec![Clause {
            who: Who { mask: 0o777 },
            actions: vec![Action {
                operator: Operator::Set,
                permission: Permission::CopyGroup,
            }],
        }]);
        let result = new_mask(0o650, &command);
        assert_eq!(result, 0o555);
    }

    #[test]
    fn new_mask_set_copy_other() {
        let command = Command::Set(vec![Clause {
            who: Who { mask: 0o777 },
            actions: vec![Action {
                operator: Operator::Set,
                permission: Permission::CopyOther,
            }],
        }]);
        let result = new_mask(0o650, &command);
        assert_eq!(result, 0o000);
    }

    #[test]
    fn new_mask_add_literal() {
        let command = Command::Set(vec![Clause {
            who: Who { mask: 0o770 },
            actions: vec![Action {
                operator: Operator::Add,
                permission: Permission::Literal {
                    mask: 0o635,
                    conditional_executable: false,
                },
            }],
        }]);
        let result = new_mask(0o653, &command);
        assert_eq!(result, 0o673);
    }

    #[test]
    fn new_mask_remove_literal() {
        let command = Command::Set(vec![Clause {
            who: Who { mask: 0o770 },
            actions: vec![Action {
                operator: Operator::Remove,
                permission: Permission::Literal {
                    mask: 0o635,
                    conditional_executable: false,
                },
            }],
        }]);
        let result = new_mask(0o753, &command);
        assert_eq!(result, 0o143);
    }

    #[test]
    fn new_mask_with_multiple_actions() {
        let command = Command::Set(vec![Clause {
            who: Who { mask: 0o770 },
            actions: vec![
                Action {
                    operator: Operator::Set,
                    permission: Permission::Literal {
                        mask: 0o635,
                        conditional_executable: false,
                    },
                },
                Action {
                    operator: Operator::Add,
                    permission: Permission::Literal {
                        mask: 0o000,
                        conditional_executable: true,
                    },
                },
            ],
        }]);
        let result = new_mask(0o766, &command);
        assert_eq!(result, 0o736);
    }

    #[test]
    fn new_mask_with_multiple_clauses() {
        let command = Command::Set(vec![
            Clause {
                who: Who { mask: 0o700 },
                actions: vec![Action {
                    operator: Operator::Set,
                    permission: Permission::Literal {
                        mask: 0o635,
                        conditional_executable: false,
                    },
                }],
            },
            Clause {
                who: Who { mask: 0o007 },
                actions: vec![Action {
                    operator: Operator::Add,
                    permission: Permission::Literal {
                        mask: 0o000,
                        conditional_executable: true,
                    },
                }],
            },
        ]);
        let result = new_mask(0o766, &command);
        assert_eq!(result, 0o667);
    }
}
