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
use std::cell::Cell;
use std::ffi::c_int;
use std::future::poll_fn;
use std::rc::Rc;
use std::task::Poll;

impl Select for VirtualSystem {
    /// Waits for a next event.
    ///
    /// The `VirtualSystem` implementation for this method simulates the
    /// blocking behavior of `select` by returning a future that becomes ready
    /// when the specified FDs are ready, the timeout expires, or a signal is
    /// delivered. However, it does not actually block the calling thread.
    /// Instead, it relies on the caller to poll the returned future to
    /// determine when the event occurs. This design allows the `VirtualSystem`
    /// to be used in asynchronous contexts without blocking the entire thread,
    /// while still providing the expected behavior of `select`.
    fn select<'a>(
        &self,
        readers: &'a mut Vec<Fd>,
        writers: &'a mut Vec<Fd>,
        timeout: Option<Duration>,
        signal_mask: Option<&[signal::Number]>,
    ) -> impl Future<Output = Result<c_int>> + use<'a> {
        // let mut state_changed = false;
        // let result;

        // {
        //     let mut state = self.state.borrow_mut();
        //     let process = state
        //         .processes
        //         .get_mut(&self.process_id)
        //         .expect("current process not found");

        //     // Detect invalid FDs first. POSIX requires that the arguments are
        //     // not modified if an error occurs.
        //     let fds = readers.iter().chain(writers.iter());
        //     let bad_fd = { fds }.any(|fd| !process.fds().contains_key(fd));

        //     if bad_fd {
        //         result = Err(Errno::EBADF);
        //     } else {
        //         let mut caught = false;
        //         let mut parent_pid_for_sigchld = None;

        //         if let Some(signal_mask) = signal_mask {
        //             let save_mask = process
        //                 .blocked_signals()
        //                 .iter()
        //                 .copied()
        //                 .collect::<Vec<signal::Number>>();
        //             let result_1 = process.block_signals(SigmaskOp::Set, signal_mask);
        //             let result_2 = process.block_signals(SigmaskOp::Set, &save_mask);
        //             assert!(!result_2.delivered);
        //             caught = result_1.caught;

        //             if result_1.process_state_changed {
        //                 parent_pid_for_sigchld = Some(process.ppid);
        //                 state_changed = true;
        //             }
        //         }

        //         if caught {
        //             result = Err(Errno::EINTR);
        //         } else {
        //             readers.retain(|fd| {
        //                 // We already checked that the FD is open, so it's safe
        //                 // to access by index.
        //                 let ofd = process.fds()[fd].open_file_description.borrow();
        //                 !ofd.is_readable() || ofd.is_ready_for_reading()
        //             });
        //             writers.retain(|fd| {
        //                 let ofd = process.fds()[fd].open_file_description.borrow();
        //                 !ofd.is_writable() || ofd.is_ready_for_writing()
        //             });

        //             let count = (readers.len() + writers.len()).try_into().unwrap();
        //             if count == 0 {
        //                 if let Some(duration) = timeout {
        //                     if !duration.is_zero() {
        //                         let now = state.now.as_mut();
        //                         let now =
        //                             now.expect("now time unspecified; cannot add timeout duration");
        //                         *now += duration;
        //                     }
        //                 }
        //             }
        //             result = Ok(count);
        //         }

        //         // NLL: process is no longer used after this point
        //         if let Some(parent_pid) = parent_pid_for_sigchld {
        //             raise_sigchld(&mut state, parent_pid);
        //         }
        //     }
        // }

        // let system = self.clone();
        // async move {
        //     if state_changed {
        //         system.block_until_running().await;
        //     }
        //     result
        // }

        let this = self.clone();
        async move {
            let deadline = match timeout {
                // Don't require the now time if the timeout is zero or infinite
                None | Some(Duration::ZERO) => None,
                Some(timeout) => {
                    let now = this.state.borrow().now;
                    let now = now.expect("current time unspecified; cannot compute deadline");
                    Some(now + timeout)
                }
            };

            poll_fn(|context| {
                let state = &mut *this.state.borrow_mut();
                let proc = state
                    .processes
                    .get_mut(&this.process_id)
                    .expect("current process not found");

                // Find ready FDs
                let mut ready_readers = Vec::new();
                let mut ready_writers = Vec::new();
                for fd in readers.iter().cloned() {
                    let Some(fd_body) = proc.fds().get(&fd) else {
                        return Poll::Ready(Err(Errno::EBADF));
                    };
                    let ofd = fd_body.open_file_description.borrow();
                    if ofd.is_ready_for_reading() {
                        ready_readers.push(fd);
                    }
                }
                for fd in writers.iter().cloned() {
                    let Some(fd_body) = proc.fds().get(&fd) else {
                        return Poll::Ready(Err(Errno::EBADF));
                    };
                    let ofd = fd_body.open_file_description.borrow();
                    if ofd.is_ready_for_writing() {
                        ready_writers.push(fd);
                    }
                }
                let count = (ready_readers.len() + ready_writers.len())
                    .try_into()
                    .unwrap();
                if count > 0 {
                    *readers = ready_readers;
                    *writers = ready_writers;
                    return Poll::Ready(Ok(count));
                }

                // Check for the deadline
                let expired = match deadline {
                    None => timeout == Some(Duration::ZERO),
                    Some(deadline) => {
                        let now = state.now;
                        let now = now.expect("current time unspecified; cannot check timeout");
                        now >= deadline
                    }
                };
                if expired {
                    return Poll::Ready(Ok(0));
                }

                // Register wakers for the expected events
                let waker = Rc::new(Cell::new(Some(context.waker().clone())));
                if let Some(deadline) = deadline {
                    state.scheduled_wakers.push(deadline, waker.clone());
                }
                for fd in readers.iter().cloned() {
                    let mut ofd = proc.fds()[&fd].open_file_description.borrow_mut();
                    ofd.register_reader_waker(waker.clone());
                }
                for fd in writers.iter().cloned() {
                    let mut ofd = proc.fds()[&fd].open_file_description.borrow_mut();
                    ofd.register_writer_waker(waker.clone());
                }
                Poll::Pending
            })
            .await
            // TODO Apply signal mask
            // TODO Re-implement block_until_running
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::SIGCHLD;
    use super::*;
    use crate::system::r#virtual::{PIPE_BUF, PIPE_SIZE};
    use crate::system::{
        CaughtSignals as _, Close as _, Disposition, Pipe as _, Read as _, Sigaction as _,
        Sigmask as _, Write as _,
    };
    use crate::test_helper::WakeFlag;
    use futures_util::FutureExt as _;
    use std::pin::pin;
    use std::sync::Arc;
    use std::task::{Context, Waker};
    use std::time::Instant;

    #[test]
    fn select_with_no_condition_blocks_forever() {
        let system = VirtualSystem::new();
        let mut readers = vec![];
        let mut writers = vec![];
        let mut select = pin!(system.select(&mut readers, &mut writers, None, None));

        // Polling the future should return pending, and it should not be woken up.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());
    }

    #[test]
    fn select_with_zero_timeout_returns_immediately() {
        let system = VirtualSystem::new();
        let mut readers = vec![];
        let mut writers = vec![];
        let mut select =
            pin!(system.select(&mut readers, &mut writers, Some(Duration::ZERO), None));

        // Polling the future should return ready immediately with a timeout result.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Ready(Ok(0)));
        assert!(!woken.is_woken());
    }

    #[test]
    fn select_regular_file_is_always_ready() {
        let system = VirtualSystem::new();
        let mut readers = vec![Fd::STDIN];
        let mut writers = vec![Fd::STDOUT, Fd::STDERR];
        {
            let mut select = pin!(system.select(&mut readers, &mut writers, None, None));

            let woken = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&woken));
            let mut context = Context::from_waker(&waker);
            let poll = select.as_mut().poll(&mut context);
            assert_eq!(poll, Poll::Ready(Ok(3)));
            assert!(!woken.is_woken());
        }
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
        {
            let mut select = pin!(system.select(&mut readers, &mut writers, None, None));

            let woken = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&woken));
            let mut context = Context::from_waker(&waker);
            let poll = select.as_mut().poll(&mut context);
            assert_eq!(poll, Poll::Ready(Ok(1)));
            assert!(!woken.is_woken());
        }
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
        {
            let mut select = pin!(system.select(&mut readers, &mut writers, None, None));

            let woken = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&woken));
            let mut context = Context::from_waker(&waker);
            let poll = select.as_mut().poll(&mut context);
            assert_eq!(poll, Poll::Ready(Ok(1)));
            assert!(!woken.is_woken());
        }
        assert_eq!(readers, [reader]);
        assert_eq!(writers, []);
    }

    #[test]
    fn select_pipe_reader_gets_ready_when_some_data_is_written() {
        let system = VirtualSystem::new();
        let (reader, writer) = system.pipe().unwrap();
        let mut readers = vec![reader];
        let mut writers = vec![];
        {
            let mut select = pin!(system.select(&mut readers, &mut writers, None, None));

            // Nothing has been written yet, so the future should not be ready,
            // and it should not be woken up.
            let woken = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&woken));
            let mut context = Context::from_waker(&waker);
            let poll = select.as_mut().poll(&mut context);
            assert_eq!(poll, Poll::Pending);
            assert!(!woken.is_woken());

            // Write some data to the pipe. The future should now be woken up.
            system.write(writer, &[0]).now_or_never().unwrap().unwrap();
            assert!(woken.is_woken());

            // Polling the future should now return ready with the reader FD.
            let woken = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&woken));
            let mut context = Context::from_waker(&waker);
            let poll = select.as_mut().poll(&mut context);
            assert_eq!(poll, Poll::Ready(Ok(1)));
            assert!(!woken.is_woken());
        }
        assert_eq!(readers, [reader]);
        assert_eq!(writers, []);
    }

    #[test]
    fn select_pipe_writer_is_ready_if_pipe_is_not_full() {
        let system = VirtualSystem::new();
        let (_reader, writer) = system.pipe().unwrap();
        let mut readers = vec![];
        let mut writers = vec![writer];
        {
            let mut select = pin!(system.select(&mut readers, &mut writers, None, None));

            let woken = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&woken));
            let mut context = Context::from_waker(&waker);
            let poll = select.as_mut().poll(&mut context);
            assert_eq!(poll, Poll::Ready(Ok(1)));
            assert!(!woken.is_woken());
        }
        assert_eq!(readers, []);
        assert_eq!(writers, [writer]);
    }

    #[test]
    fn select_pipe_writer_gets_ready_when_some_data_is_read() {
        let system = VirtualSystem::new();
        let (reader, writer) = system.pipe().unwrap();
        let mut readers = vec![];
        let mut writers = vec![writer];

        // Fill the pipe buffer.
        system
            .write(writer, &[0; PIPE_SIZE])
            .now_or_never()
            .unwrap()
            .unwrap();

        {
            let mut select = pin!(system.select(&mut readers, &mut writers, None, None));

            // The pipe is full, so the future should not be ready, and it should not be woken up.
            let woken = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&woken));
            let mut context = Context::from_waker(&waker);
            let poll = select.as_mut().poll(&mut context);
            assert_eq!(poll, Poll::Pending);
            assert!(!woken.is_woken());

            // Read some data from the pipe. The future should now be woken up.
            system
                .read(reader, &mut [0; PIPE_BUF])
                .now_or_never()
                .unwrap()
                .unwrap();
            assert!(woken.is_woken());

            // Polling the future should now return ready with the writer FD.
            let woken = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&woken));
            let mut context = Context::from_waker(&waker);
            let poll = select.as_mut().poll(&mut context);
            assert_eq!(poll, Poll::Ready(Ok(1)));
            assert!(!woken.is_woken());
        }
        assert_eq!(readers, []);
        assert_eq!(writers, [writer]);
    }

    #[ignore = "todo: temporarily ignored"]
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

    #[ignore = "todo: temporarily ignored"]
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
    fn select_on_invalid_fd_for_readers() {
        let system = VirtualSystem::new();
        let mut readers = vec![Fd(17)];
        let mut writers = vec![];
        {
            let mut select = pin!(system.select(&mut readers, &mut writers, None, None));

            let woken = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&woken));
            let mut context = Context::from_waker(&waker);
            let poll = select.as_mut().poll(&mut context);
            assert_eq!(poll, Poll::Ready(Err(Errno::EBADF)));
            assert!(!woken.is_woken());
        }
        assert_eq!(readers, [Fd(17)]);
        assert_eq!(writers, []);
    }

    #[test]
    fn select_on_invalid_fd_for_writers() {
        let system = VirtualSystem::new();
        let mut readers = vec![];
        let mut writers = vec![Fd(17)];
        {
            let mut select = pin!(system.select(&mut readers, &mut writers, None, None));

            let woken = Arc::new(WakeFlag::new());
            let waker = Waker::from(Arc::clone(&woken));
            let mut context = Context::from_waker(&waker);
            let poll = select.as_mut().poll(&mut context);
            assert_eq!(poll, Poll::Ready(Err(Errno::EBADF)));
            assert!(!woken.is_woken());
        }
        assert_eq!(readers, []);
        assert_eq!(writers, [Fd(17)]);
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

    #[ignore = "todo: temporarily ignored"]
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

    #[ignore = "todo: temporarily ignored"]
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

    #[ignore = "todo: temporarily ignored"]
    #[test]
    fn select_timeout() {
        let system = VirtualSystem::new();
        let now = Instant::now();
        system.state.borrow_mut().now = Some(now);

        let (reader, _writer) = system.pipe().unwrap();
        let mut readers = vec![reader];
        let mut writers = vec![];
        let timeout = Duration::new(42, 195);

        let mut select = pin!(system.select(&mut readers, &mut writers, Some(timeout), None));

        // On the first poll, the timeout should not have expired yet,
        // and the future should not be woken up.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());

        // Advance time by 42 seconds. The timeout is not yet reached.
        let time_before_timeout = now + Duration::new(42, 0);
        system.state.borrow_mut().advance_time(time_before_timeout);
        assert!(!woken.is_woken());

        // Even if not woken, it must be safe to poll the future again,
        // and it should still not be ready.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());

        // Advance time by another 195 nanoseconds.
        // The timeout should now be reached, and the future should be woken up.
        system.state.borrow_mut().advance_time(now + timeout);
        assert!(woken.is_woken());

        // Polling the future should now return ready with a timeout result.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Ready(Ok(0)));
    }
}
