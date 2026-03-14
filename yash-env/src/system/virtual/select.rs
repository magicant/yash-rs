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

//! Implementation of [`Select`] for [`VirtualSystem`]

use super::{
    Duration, Errno, Fd, Result, Select, SigmaskOp, TryInto, VirtualSystem, raise_sigchld, signal,
};
use std::ffi::c_int;

impl Select for VirtualSystem {
    /// Waits for a next event.
    ///
    /// The `VirtualSystem` implementation for this method does not actually
    /// block the calling thread. The method returns immediately in any case,
    /// except when temporarily changing the signal mask causes the process to
    /// stop, in which case the returned future will be pending until the
    /// process is running again.
    ///
    /// The `timeout` is ignored if this function returns because of a ready FD
    /// or a caught signal. Otherwise, the timeout is added to
    /// [`SystemState::now`](super::SystemState::now), which must not be `None`
    /// then.
    fn select(
        &self,
        readers: &mut Vec<Fd>,
        writers: &mut Vec<Fd>,
        timeout: Option<Duration>,
        signal_mask: Option<&[signal::Number]>,
    ) -> impl Future<Output = Result<c_int>> + use<> {
        let mut state_changed = false;
        let result;

        {
            let mut state = self.state.borrow_mut();
            let process = state
                .processes
                .get_mut(&self.process_id)
                .expect("current process not found");

            // Detect invalid FDs first. POSIX requires that the arguments are
            // not modified if an error occurs.
            let fds = readers.iter().chain(writers.iter());
            let bad_fd = { fds }.any(|fd| !process.fds().contains_key(fd));

            if bad_fd {
                result = Err(Errno::EBADF);
            } else {
                let mut caught = false;
                let mut parent_pid_for_sigchld = None;

                if let Some(signal_mask) = signal_mask {
                    let save_mask = process
                        .blocked_signals()
                        .iter()
                        .copied()
                        .collect::<Vec<signal::Number>>();
                    let result_1 = process.block_signals(SigmaskOp::Set, signal_mask);
                    let result_2 = process.block_signals(SigmaskOp::Set, &save_mask);
                    assert!(!result_2.delivered);
                    caught = result_1.caught;

                    if result_1.process_state_changed {
                        parent_pid_for_sigchld = Some(process.ppid);
                        state_changed = true;
                    }
                }

                if caught {
                    result = Err(Errno::EINTR);
                } else {
                    readers.retain(|fd| {
                        // We already checked that the FD is open, so it's safe
                        // to access by index.
                        let ofd = process.fds()[fd].open_file_description.borrow();
                        !ofd.is_readable() || ofd.is_ready_for_reading()
                    });
                    writers.retain(|fd| {
                        let ofd = process.fds()[fd].open_file_description.borrow();
                        !ofd.is_writable() || ofd.is_ready_for_writing()
                    });

                    let count = (readers.len() + writers.len()).try_into().unwrap();
                    if count == 0 {
                        if let Some(duration) = timeout {
                            if !duration.is_zero() {
                                let now = state.now.as_mut();
                                let now =
                                    now.expect("now time unspecified; cannot add timeout duration");
                                *now += duration;
                            }
                        }
                    }
                    result = Ok(count);
                }

                // NLL: process is no longer used after this point
                if let Some(parent_pid) = parent_pid_for_sigchld {
                    raise_sigchld(&mut state, parent_pid);
                }
            }
        }

        let system = self.clone();
        async move {
            if state_changed {
                system.block_until_running().await;
            }
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::SIGCHLD;
    use super::*;
    use crate::system::{
        CaughtSignals as _, Close as _, Disposition, Pipe as _, Sigaction as _, Sigmask as _,
        Write as _,
    };
    use futures_util::FutureExt as _;
    use std::time::Instant;

    #[test]
    fn select_regular_file_is_always_ready() {
        let system = VirtualSystem::new();
        let mut readers = vec![Fd::STDIN];
        let mut writers = vec![Fd::STDOUT, Fd::STDERR];

        let result = system
            .select(&mut readers, &mut writers, None, None)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(3));
        assert_eq!(readers, [Fd::STDIN]);
        assert_eq!(writers, [Fd::STDOUT, Fd::STDERR]);
    }

    #[test]
    fn select_pipe_reader_is_ready_if_writer_is_closed() {
        let system = VirtualSystem::new();
        let (reader, writer) = system.pipe().unwrap();
        system.close(writer).unwrap();
        let mut readers = vec![reader];
        let mut writers = vec![];

        let result = system
            .select(&mut readers, &mut writers, None, None)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(1));
        assert_eq!(readers, [reader]);
        assert_eq!(writers, []);
    }

    #[test]
    fn select_pipe_reader_is_ready_if_something_has_been_written() {
        let system = VirtualSystem::new();
        let (reader, writer) = system.pipe().unwrap();
        system.write(writer, &[0]).now_or_never().unwrap().unwrap();
        let mut readers = vec![reader];
        let mut writers = vec![];

        let result = system
            .select(&mut readers, &mut writers, None, None)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(1));
        assert_eq!(readers, [reader]);
        assert_eq!(writers, []);
    }

    #[test]
    fn select_pipe_reader_is_not_ready_if_writer_has_written_nothing() {
        let system = VirtualSystem::new();
        let (reader, _writer) = system.pipe().unwrap();
        let mut readers = vec![reader];
        let mut writers = vec![];

        let result = system
            .select(&mut readers, &mut writers, None, None)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(0));
        assert_eq!(readers, []);
        assert_eq!(writers, []);
    }

    #[test]
    fn select_pipe_writer_is_ready_if_pipe_is_not_full() {
        let system = VirtualSystem::new();
        let (_reader, writer) = system.pipe().unwrap();
        let mut readers = vec![];
        let mut writers = vec![writer];

        let result = system
            .select(&mut readers, &mut writers, None, None)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(1));
        assert_eq!(readers, []);
        assert_eq!(writers, [writer]);
    }

    #[test]
    fn select_on_unreadable_fd() {
        let system = VirtualSystem::new();
        let (_reader, writer) = system.pipe().unwrap();
        let mut fds = vec![writer];
        let result = system
            .select(&mut fds, &mut vec![], None, None)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(1));
        assert_eq!(fds, [writer]);
    }

    #[test]
    fn select_on_unwritable_fd() {
        let system = VirtualSystem::new();
        let (reader, _writer) = system.pipe().unwrap();
        let mut fds = vec![reader];
        let result = system
            .select(&mut vec![], &mut fds, None, None)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(1));
        assert_eq!(fds, [reader]);
    }

    #[test]
    fn select_on_closed_fd() {
        let system = VirtualSystem::new();
        let result = system
            .select(&mut vec![Fd(17)], &mut vec![], None, None)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Err(Errno::EBADF));

        let result = system
            .select(&mut vec![], &mut vec![Fd(17)], None, None)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Err(Errno::EBADF));
    }

    fn system_for_catching_sigchld() -> VirtualSystem {
        let system = VirtualSystem::new();
        system
            .sigmask(Some((SigmaskOp::Add, &[SIGCHLD])), None)
            .now_or_never()
            .unwrap()
            .unwrap();
        system.sigaction(SIGCHLD, Disposition::Catch).unwrap();
        system
    }

    #[test]
    fn select_on_non_pending_signal() {
        let system = system_for_catching_sigchld();
        let result = system
            .select(&mut vec![], &mut vec![], None, Some(&[]))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(0));
        assert_eq!(system.caught_signals(), []);
    }

    #[test]
    fn select_on_pending_signal() {
        let system = system_for_catching_sigchld();
        let _ = system.current_process_mut().raise_signal(SIGCHLD);
        let result = system
            .select(&mut vec![], &mut vec![], None, Some(&[]))
            .now_or_never()
            .unwrap();
        assert_eq!(result, Err(Errno::EINTR));
        assert_eq!(system.caught_signals(), [SIGCHLD]);
    }

    #[test]
    fn select_timeout() {
        let system = VirtualSystem::new();
        let now = Instant::now();
        system.state.borrow_mut().now = Some(now);

        let (reader, _writer) = system.pipe().unwrap();
        let mut readers = vec![reader];
        let mut writers = vec![];
        let timeout = Duration::new(42, 195);

        let result = system
            .select(&mut readers, &mut writers, Some(timeout), None)
            .now_or_never()
            .unwrap();
        assert_eq!(result, Ok(0));
        assert_eq!(readers, []);
        assert_eq!(writers, []);
        assert_eq!(
            system.state.borrow().now,
            Some(now + Duration::new(42, 195))
        );
    }
}
