// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2025 WATANABE Yuki
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

//! Definition of `Runtime`

use yash_env::System;

/// Runtime environment for executing shell commands
///
/// This trait combines various capabilities required for command execution and
/// word expansion into a single trait. Since the implementation of command
/// execution and word expansion is mutually recursive, any trait needed for any
/// part of the implementation is transitively required by most of the
/// implementation. Therefore, this trait serves as a convenient shorthand to
/// express the required capabilities.
pub trait Runtime: System {}
// TODO: Runtime temporarily requires System. It should require only the
// necessary traits and System should be removed later.

/// Any type automatically implements `Runtime` if it implements all the
/// supertraits of `Runtime`.
impl<S: System> Runtime for S {}
