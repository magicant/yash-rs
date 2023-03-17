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

//! System error handling

use std::ffi::c_int;
use std::io::Error;

/// Returns the error from `errno` if the result is `-1`.
///
/// If the result is not `-1`, returns the result as `Ok`.
pub fn error_m1(result: c_int) -> Result<c_int, Error> {
    if result != -1 {
        Ok(result)
    } else {
        Err(Error::last_os_error())
    }
}
