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

//! Shell startup

use self::args::Run;
use self::args::Source;
use yash_env::io::Fd;
use yash_env::option::Option::Interactive;
use yash_env::System;

pub mod args;
pub mod input;

/// Tests whether the shell should be implicitly interactive.
///
/// As per POSIX, "if there are no operands and the shell's standard input and
/// standard error are attached to a terminal, the shell is considered to be
/// interactive." This function implements this rule.
pub fn auto_interactive<S: System>(system: &S, run: &Run) -> bool {
    if run.source != Source::Stdin {
        return false;
    }
    if run.options.iter().any(|&(o, _)| o == Interactive) {
        return false;
    }
    if !run.positional_params.is_empty() {
        return false;
    }
    system.isatty(Fd::STDIN).unwrap_or(false) && system.isatty(Fd::STDERR).unwrap_or(false)
}
