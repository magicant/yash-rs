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

//! Type definitions for signal handling settings.

#[cfg(doc)]
use crate::system::{SharedSystem, System};

#[doc(no_inline)]
pub use nix::sys::signal::Signal;
#[doc(no_inline)]
pub use nix::Result;

/// System interface for signal handling configuration.
pub trait SignalSystem {
    /// Sets how a signal is handled.
    ///
    /// This function updates the signal blocking mask and the signal action for
    /// the specified signal and remembers the previous configuration for
    /// restoration.
    ///
    /// Returns the previous handler.
    fn set_signal_handling(
        &mut self,
        signal: Signal,
        handling: crate::system::SignalHandling,
    ) -> Result<crate::system::SignalHandling>;
}

/// Collection of signal handling settings.
///
/// A `TrapSet` remembers the trap configured for each signal, and manages the
/// signal handlers installed to the underlying system. `TrapSet` acts as a
/// decorator for a system implementing [`SignalSystem`]. Methods of `TrapSet`
/// expect to be passed the same system instance in every call.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrapSet {}

impl TrapSet {
    /// Installs a signal handler for `SIGCHLD`.
    ///
    /// You should install the `SIGCHLD` handler to the system by using this
    /// function before waiting for `SIGCHLD` with [`System::wait`] and
    /// [`SharedSystem::wait_for_signal`].
    ///
    /// This function remembers that the handler has been installed, so a second
    /// call to the function will be a no-op.
    pub fn enable_sigchld_handler<S: SignalSystem>(&mut self, system: &mut S) -> Result<()> {
        system
            .set_signal_handling(Signal::SIGCHLD, crate::system::SignalHandling::Catch)
            .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct DummySystem(HashMap<Signal, crate::system::SignalHandling>);

    impl SignalSystem for DummySystem {
        fn set_signal_handling(
            &mut self,
            signal: Signal,
            handling: crate::system::SignalHandling,
        ) -> Result<crate::system::SignalHandling> {
            Ok(self
                .0
                .insert(signal, handling)
                .unwrap_or(crate::system::SignalHandling::Default))
        }
    }

    #[test]
    fn enabling_sigchld_handler() {
        let mut system = DummySystem::default();
        let mut trap_set = TrapSet::default();
        trap_set.enable_sigchld_handler(&mut system).unwrap();
        assert_eq!(
            system.0[&Signal::SIGCHLD],
            crate::system::SignalHandling::Catch
        );
    }
}
