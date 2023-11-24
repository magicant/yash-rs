// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Colon (`:`) built-in
//!
//! The colon (**`:`**) built-in does nothing.
//!
//! # Synopsis
//!
//! ```sh
//! : […]
//! ```
//!
//! # Description
//!
//! The colon built-in is a dummy command that does nothing.
//! Any arguments are ignored.
//!
//! # Errors
//!
//! None.
//!
//! # Exit status
//!
//! Zero.
//!
//! # Portability
//!
//! The colon built-in is specified in the POSIX standard.

use crate::Result;
use yash_env::semantics::Field;
use yash_env::Env;

/// Entry point for executing the `:` built-in
pub fn main(_env: &mut Env, _args: Vec<Field>) -> Result {
    Result::default()
}
