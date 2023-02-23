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

//! Functions about signals

use super::super::Signal::{self, *};

/// Default effect of a signal delivered to a process.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SignalEffect {
    /// Does nothing.
    None,
    /// Terminates the process.
    Terminate { core_dump: bool },
    /// Suspends the process.
    Suspend,
    /// Resumes the process.
    Resume,
}

impl SignalEffect {
    /// Returns the default effect for the specified signal.
    #[must_use]
    pub fn of(signal: Signal) -> Self {
        match signal {
            SIGHUP => Self::Terminate { core_dump: false },
            SIGINT => Self::Terminate { core_dump: false },
            SIGQUIT => Self::Terminate { core_dump: true },
            SIGILL => Self::Terminate { core_dump: true },
            SIGTRAP => Self::Terminate { core_dump: true },
            SIGABRT => Self::Terminate { core_dump: true },
            SIGBUS => Self::Terminate { core_dump: true },
            // SIGEMT => Self::Terminate { core_dump: false },
            SIGFPE => Self::Terminate { core_dump: true },
            SIGKILL => Self::Terminate { core_dump: false },
            SIGUSR1 => Self::Terminate { core_dump: false },
            SIGSEGV => Self::Terminate { core_dump: true },
            SIGUSR2 => Self::Terminate { core_dump: false },
            SIGPIPE => Self::Terminate { core_dump: false },
            SIGALRM => Self::Terminate { core_dump: false },
            SIGTERM => Self::Terminate { core_dump: false },
            // SIGSTKFLT => Self::Terminate { core_dump: false },
            SIGCHLD => Self::None,
            SIGCONT => Self::Resume,
            SIGSTOP => Self::Suspend,
            SIGTSTP => Self::Suspend,
            SIGTTIN => Self::Suspend,
            SIGTTOU => Self::Suspend,
            SIGURG => Self::None,
            SIGXCPU => Self::Terminate { core_dump: true },
            SIGXFSZ => Self::Terminate { core_dump: true },
            SIGVTALRM => Self::Terminate { core_dump: false },
            SIGPROF => Self::Terminate { core_dump: false },
            SIGWINCH => Self::None,
            SIGIO => Self::Terminate { core_dump: false },
            // SIGPWR => Self::Terminate { core_dump: false },
            // SIGINFO => Self::Terminate { core_dump: false },
            SIGSYS => Self::Terminate { core_dump: true },
            _ => Self::Terminate { core_dump: false },
        }
    }
}
