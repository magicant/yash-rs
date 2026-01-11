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
use yash_env::system::{Close, Dup, Fcntl, Fstat, Isatty, Open, Wait, Write};

/// Runtime environment for executing shell commands
///
/// TBD
pub trait Runtime: Close + Dup + Fcntl + Fstat + Isatty + Open + System + Wait + Write {}

/// Blanket implementation of `Runtime` for any type implementing `System`
impl<S: Close + Dup + Fcntl + Fstat + Isatty + Open + System + Wait + Write> Runtime for S {}
