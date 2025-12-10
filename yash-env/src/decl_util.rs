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

//! Defining declaration utilities
//!
//! This module contains the [`Glossary`] trait, which is used by the parser to
//! determine whether a command name is a declaration utility. It also provides
//! implementations of the `Glossary` trait: [`EmptyGlossary`],
//! [`PosixGlossary`], and [`Env`].
//!
//! This crate (`yash-env`) does not provide a parser itself. The `yash-syntax`
//! crate provides a parser that uses this module.
//!
//! # What are declaration utilities?
//!
//! A [declaration utility] is a type of command that causes its argument words
//! to be expanded in a manner slightly different from other commands. Usually,
//! command word expansion includes field splitting and pathname expansion. For
//! declaration utilities, however, those expansions are not performed on the
//! arguments that have a form of variable assignments.
//!
//! [declaration utility]: https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap03.html#tag_03_100
//!
//! Generally, a simple command consists of assignments, redirections, and command
//! words. The shell syntax allows the redirections to be placed anywhere in the
//! command, but the assignments must come before the command words. An assignment
//! token has the form `name=value`, the first token that does not match this
//! form is considered the command name, and the rest are arguments regardless of
//! whether they match the form. For example, in the command `a=1 b=2 echo c=3`,
//! `a=1` and `b=2` are assignments, `echo` is the command name, and `c=3` is an
//! argument.
//!
//! All assignments and command words are expanded when the command is executed,
//! but the expansions are different. The expansions of assignments are performed
//! in a way that does not include field splitting and pathname expansion. This
//! ensures that the values of the assignments are not split or expanded into
//! multiple fields. The expansions of command words, on the other hand, are
//! performed in a way that includes field splitting and pathname expansion,
//! which may expand a single word into multiple fields.
//!
//! The assignments specified in a simple command are performed by the shell
//! before the utility specified by the command name is invoked. However, some
//! utilities perform their own assignments based on their arguments. For such
//! a utility, the tokens that specify the assigned variable names and values
//! are given as arguments to the utility as in the command `export a=1 b=2`.
//!
//! By default, such arguments are expanded in the same way as usual command
//! words, which means that the assignments are subject to field splitting and
//! pathname expansion even though they are effectively assignments. To prevent
//! this, the shell recognizes certain command names as declaration utilities
//! and expands their arguments differently. The shell does not perform field
//! splitting and pathname expansion on the arguments of declaration utilities
//! that have the form of variable assignments.
//!
//! # Example
//!
//! POSIX requires the `export` utility to be recognized as a declaration
//! utility. In the command `v='1 b=2'; export a=$v`, the word `a=$v` is not
//! subject to field splitting because `export` is a declaration utility, so the
//! expanded word `a=1 b=2` is passed to `export` as an argument, so `export`
//! assigns the value `1 b=2` to the variable `a`. If `export` were not a
//! declaration utility, the word `a=$v` would be subject to field splitting,
//! and the expanded word `a=1 b=2` would be split into two fields `a=1` and
//! `b=2`, so `export` would assign the value `1` to the variable `a` and the
//! value `2` to the variable `b`.
//!
//! # Which command names are declaration utilities?
//!
//! The POSIX standard specifies that the following command names are declaration
//! utilities:
//!
//! - `export` and `readonly` are declaration utilities.
//! - `command` is neutral; it delegates to the next command word to determine
//!   whether it is a declaration utility.
//!
//! It is unspecified whether other command names are declaration utilities.
//!
//! The syntax parser can use the [`Glossary`] trait to determine whether a
//! command name is a declaration utility. The parser calls its
//! [`is_declaration_utility`] method when it encounters a command name, and
//! changes how the following arguments are parsed based on the result.
//!
//! [`is_declaration_utility`]: Glossary::is_declaration_utility
//!
//! This module provides three implementations of the `Glossary` trait:
//!
//! - [`PosixGlossary`] recognizes the declaration utilities defined by POSIX
//!   (and no others). This is the default glossary used by the parser.
//! - [`EmptyGlossary`] recognizes no command name as a declaration utility.
//!   The parse result does not conform to POSIX when this glossary is used.
//! - [`Env`] recognizes declaration utilities based on the built-ins defined
//!   in the environment.
//!
//! You can implement the `Glossary` trait for your own glossary if you want to
//! recognize additional command names as declaration utilities. Such a custom
//! glossary can only be used when you directly configure the parser.

use crate::Env;
use std::cell::RefCell;
use std::fmt::Debug;

/// Interface used by the parser to tell if a command name is a declaration utility
///
/// The parser uses this trait to determine whether a command name is a declaration
/// utility. See the [module-level documentation](self) for details.
pub trait Glossary: Debug {
    /// Returns whether the given command name is a declaration utility.
    ///
    /// If the command name is a declaration utility, this method should return
    /// `Some(true)`. If the command name is not a declaration utility, this
    /// method should return `Some(false)`. If the return value is `None`, this
    /// method is called again with the next command word in the simple command
    /// being parsed, effectively delegating the decision to the next command word.
    ///
    /// To meet the POSIX standard, the method should return `Some(true)` for the
    /// command names `export` and `readonly`, and `None` for the command name
    /// `command`.
    fn is_declaration_utility(&self, name: &str) -> Option<bool>;
}

/// Empty glossary that does not recognize any command name as a declaration utility
///
/// When this glossary is used, the parser recognizes no command name as a
/// declaration utility. Note that this does not conform to POSIX.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct EmptyGlossary;

impl Glossary for EmptyGlossary {
    #[inline(always)]
    fn is_declaration_utility(&self, _name: &str) -> Option<bool> {
        Some(false)
    }
}

/// Glossary that recognizes declaration utilities defined by POSIX
///
/// This glossary recognizes the declaration utilities defined by POSIX and no
/// others. The `is_declaration_utility` method returns `Some(true)` for the
/// command names `export` and `readonly`, and `None` for the command name
/// `command`.
///
/// This is the minimal glossary that conforms to POSIX, and is the default
/// glossary used by the parser.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct PosixGlossary;

impl Glossary for PosixGlossary {
    fn is_declaration_utility(&self, name: &str) -> Option<bool> {
        match name {
            "export" | "readonly" => Some(true),
            "command" => None,
            _ => Some(false),
        }
    }
}

impl<T: Glossary> Glossary for &T {
    fn is_declaration_utility(&self, name: &str) -> Option<bool> {
        (**self).is_declaration_utility(name)
    }
}

impl<T: Glossary> Glossary for &mut T {
    fn is_declaration_utility(&self, name: &str) -> Option<bool> {
        (**self).is_declaration_utility(name)
    }
}

/// Allows a glossary to be wrapped in a `RefCell`.
///
/// This implementation's methods immutably borrow the inner glossary.
/// If the inner glossary is mutably borrowed at the same time, it panics.
impl<T: Glossary> Glossary for RefCell<T> {
    fn is_declaration_utility(&self, name: &str) -> Option<bool> {
        self.borrow().is_declaration_utility(name)
    }
}

/// Determines whether a command name is a declaration utility.
///
/// This implementation looks up the command name in `self.builtins` and returns
/// the value of `is_declaration_utility` if the built-in is found. Otherwise,
/// the command is not a declaration utility.
impl<S: Debug> Glossary for Env<S> {
    fn is_declaration_utility(&self, name: &str) -> Option<bool> {
        match self.builtins.get(name) {
            Some(builtin) => builtin.is_declaration_utility,
            None => Some(false),
        }
    }
}
