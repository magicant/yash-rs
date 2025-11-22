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

//! Items for parsing declaration utilities
//!
//! This module re-exports utilities for parsing declaration utilities from the
//! `yash-env` crate. See the documentation of the [`yash_env::decl_util`]
//! module for details.
//!
//! # Parser behavior
//!
//! When the [parser] recognizes a command name as a declaration utility,
//! command words that follow the command name are tested for the form of
//! variable assignments. If a word is a variable assignment, it is parsed as
//! such: the word is split into a variable name and a value, and tilde expansions
//! are parsed with the [`parse_tilde_everywhere_after`] method in the value part.
//! The result word is marked with [`ExpansionMode::Single`] in
//! [`SimpleCommand::words`] to indicate that the word is not subject to field
//! splitting and pathname expansion. If a word is not a variable assignment, it
//! is parsed as a normal command word with [`parse_tilde_front`] and marked with
//! [`ExpansionMode::Multiple`].
//!
//! The shell is expected to change the expansion behavior of the words based on
//! the [`ExpansionMode`] of the words. In yash-rs, the semantics is implemented
//! in the `yash-semantics` crate.
//!
//! [parser]: crate::parser
//! [`parse_tilde_front`]: crate::syntax::Word::parse_tilde_front
//! [`parse_tilde_everywhere_after`]: crate::syntax::Word::parse_tilde_everywhere_after
//! [`ExpansionMode`]: crate::syntax::ExpansionMode
//! [`ExpansionMode::Multiple`]: crate::syntax::ExpansionMode::Multiple
//! [`ExpansionMode::Single`]: crate::syntax::ExpansionMode::Single
//! [`SimpleCommand::words`]: crate::syntax::SimpleCommand::words

#[doc(no_inline)]
pub use yash_env::decl_util::*;
