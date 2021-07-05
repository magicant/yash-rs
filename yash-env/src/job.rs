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

//! Type definitions for job management.

use nix::unistd::Pid;
use std::collections::HashMap;

/// Child process of the shell.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChildProcess {
    // TODO state
}

// TODO Job as a set of child processes

/// Collection of jobs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct JobSet {
    pub child_processes: HashMap<Pid, ChildProcess>,
}
