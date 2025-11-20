// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

// TODO Update this documentation
//! Methods about passing [source](yash_syntax::source) code to the
//! [parser](yash_syntax::parser).
//!
//! This module extends [`yash_syntax::input`] with input functions that are
//! implemented depending on the environment.

#[doc(no_inline)]
pub use yash_syntax::input::*;

mod fd_reader;
pub use fd_reader::FdReader;

mod echo;
pub use echo::Echo;

mod ignore_eof;
pub use ignore_eof::IgnoreEof;

mod reporter;
pub use reporter::Reporter;
