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
use crate::job::ProcessState;
use std::cell::{Cell, LazyCell};
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
        let this = self.clone();
        let signal_mask = signal_mask.map(|mask| mask.to_vec());
        #[allow(clippy::await_holding_refcell_ref)] // False positive
        async move {
            let (old_mask, old_caught_signals, deadline) = {
                let state = &mut *this.state.borrow_mut();
                let proc = state
                    .processes
                    .get_mut(&this.process_id)
                    .expect("current process not found");

                let old_caught_signals = proc.caught_signals.len();

                let old_mask = match signal_mask {
                    None => None,
                    Some(new_mask) => {
                        let old_mask = proc
                            .blocked_signals()
                            .iter()
                            .copied()
                            .collect::<Vec<signal::Number>>();

                        let result = proc.block_signals(SigmaskOp::Set, &new_mask);
                        if result.process_state_changed {
                            let ppid = proc.ppid;
                            raise_sigchld(state, ppid);
                        }

                        Some(old_mask)
                    }
                };

                let deadline = match timeout {
                    // Don't require the now time if the timeout is zero or infinite
                    None | Some(Duration::ZERO) => None,
                    Some(timeout) => {
                        let now = state.now;
                        let now = now.expect("current time unspecified; cannot compute deadline");
                        Some(now + timeout)
                    }
                };

                (old_mask, old_caught_signals, deadline)
            };

            let waker = LazyCell::new(|| Rc::new(Cell::new(None)));

            let result = poll_fn(|context| {
                let state = &mut *this.state.borrow_mut();
                let proc = state
                    .processes
                    .get_mut(&this.process_id)
                    .expect("current process not found");

                // If the process is currently suspended, do nothing until resumed
                if let ProcessState::Halted(reason) = proc.state() {
                    if reason.is_stopped() {
                        // let waker = Rc::new(Cell::new(Some(context.waker().clone())));
                        waker.set(Some(context.waker().clone()));
                        proc.wake_on_resumption(Rc::downgrade(&waker));
                        return Poll::Pending;
                    }
                }

                // Check for delivered signals
                if proc.caught_signals.len() != old_caught_signals {
                    return Poll::Ready(Err(Errno::EINTR));
                }

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
                    readers.clear();
                    writers.clear();
                    return Poll::Ready(Ok(0));
                }

                // Register wakers for the expected events
                waker.set(Some(context.waker().clone()));
                proc.register_signal_waker(Rc::downgrade(&waker));
                for fd in readers.iter() {
                    let mut ofd = proc.fds()[fd].open_file_description.borrow_mut();
                    ofd.register_reader_waker(Rc::downgrade(&waker));
                }
                for fd in writers.iter() {
                    let mut ofd = proc.fds()[fd].open_file_description.borrow_mut();
                    ofd.register_writer_waker(Rc::downgrade(&waker));
                }
                if let Some(deadline) = deadline {
                    state.scheduled_wakers.push(deadline, Rc::downgrade(&waker));
                }
                Poll::Pending
            })
            .await;

            drop(waker);

            // Restore the previous signal mask
            if let Some(old_mask) = old_mask {
                let mut state = this.state.borrow_mut();
                let proc = state
                    .processes
                    .get_mut(&this.process_id)
                    .expect("current process not found");
                let result = proc.block_signals(SigmaskOp::Set, &old_mask);
                if result.process_state_changed {
                    let ppid = proc.ppid;
                    raise_sigchld(&mut state, ppid);
                    drop(state);
                    this.block_until_running().await;
                }
            }

            result
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Process;
    use super::super::{PIPE_BUF, PIPE_SIZE, SIGCHLD, SIGCONT, SIGTSTP};
    use super::*;
    use crate::job::Pid;
    use crate::system::{
        CaughtSignals as _, Close as _, Disposition, Pipe as _, Read as _, SendSignal as _,
        Sigaction as _, Sigmask as _, Write as _,
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

    #[test]
    fn select_on_pending_signal() {
        let system = system_for_catching_sigchld();
        let _ = system.current_process_mut().raise_signal(SIGCHLD);
        let mut readers = vec![];
        let mut writers = vec![];

        let mut select = pin!(system.select(&mut readers, &mut writers, None, Some(&[])));

        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Ready(Err(Errno::EINTR)));
        assert!(!woken.is_woken());
        assert_eq!(system.caught_signals(), [SIGCHLD]);
        // Check that the signal mask is the same as before the select call.
        let mut mask = Vec::new();
        system
            .sigmask(None, Some(&mut mask))
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(mask, [SIGCHLD]);
    }

    #[test]
    fn select_interrupted_by_signal() {
        let system = VirtualSystem::new();
        system.sigaction(SIGCHLD, Disposition::Catch).unwrap();
        let mut readers = vec![];
        let mut writers = vec![];

        let mut select = pin!(system.select(&mut readers, &mut writers, None, None));

        // Since no conditions were specified, the select call should block indefinitely.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());

        // Even if not woken, it must be safe to poll the future again,
        // and it should still not be ready.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());

        // When a signal is caught, the future should be woken up.
        _ = system.current_process_mut().raise_signal(SIGCHLD);
        assert!(woken.is_woken());

        // Polling the future should now return ready with an EINTR error.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Ready(Err(Errno::EINTR)));
        assert!(!woken.is_woken());
    }

    #[test]
    fn select_on_signal_delivered_while_waiting() {
        let system = system_for_catching_sigchld();
        let mut readers = vec![];
        let mut writers = vec![];

        let mut select = pin!(system.select(&mut readers, &mut writers, None, Some(&[])));

        // The future should not be ready yet, and it should not be woken up.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());
        // While waiting, the signal mask passed to select should be in effect
        let mut mask = Vec::new();
        system
            .sigmask(None, Some(&mut mask))
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(mask, []);

        // Even if not woken, it must be safe to poll the future again,
        // and it should still not be ready.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());

        // Raise a signal. The future should now be woken up.
        let _ = system.current_process_mut().raise_signal(SIGCHLD);
        assert!(woken.is_woken());

        // Polling the future should now return ready with an EINTR error.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Ready(Err(Errno::EINTR)));
        assert!(!woken.is_woken());
        assert_eq!(system.caught_signals(), [SIGCHLD]);
        // Check that the signal mask is the same as before the select call.
        let mut mask = Vec::new();
        system
            .sigmask(None, Some(&mut mask))
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(mask, [SIGCHLD]);
    }

    #[test]
    fn select_timeout() {
        let system = VirtualSystem::new();
        let now = Instant::now();
        system.state.borrow_mut().now = Some(now);

        // The first pipe is empty, so the reader is not ready.
        let (reader_1, _writer_1) = system.pipe().unwrap();
        // The second pipe is full, so the writer is not ready.
        let (_reader_2, writer_2) = system.pipe().unwrap();
        system
            .write(writer_2, &[0; PIPE_SIZE])
            .now_or_never()
            .unwrap()
            .unwrap();
        let mut readers = vec![reader_1];
        let mut writers = vec![writer_2];
        let timeout = Duration::new(42, 195);

        {
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
        // After a timeout, no readers or writers should be ready
        assert_eq!(readers, []);
        assert_eq!(writers, []);
    }

    fn virtual_system_with_parent_process() -> VirtualSystem {
        let system = VirtualSystem::new();
        let ppid = system.current_process().ppid;
        let mut parent = Process::with_parent_and_group(Pid(1), Pid(1));
        parent.set_disposition(SIGCHLD, Disposition::Catch);
        system.state.borrow_mut().processes.insert(ppid, parent);
        system
    }

    /// In this test case, SIGTSTP is blocked and pending when the `select` call
    /// is made. When the `select` call temporarily unblocks SIGTSTP, the
    /// pending signal should be delivered, which suspends the process. The
    /// `select` future should return pending until the process is resumed.
    #[test]
    fn select_returns_pending_while_process_is_suspended_1() {
        let system = virtual_system_with_parent_process();
        let now = Instant::now();
        system.state.borrow_mut().now = Some(now);
        system
            .sigmask(Some((SigmaskOp::Add, &[SIGTSTP])), None)
            .now_or_never()
            .unwrap()
            .unwrap();

        // Send SIGTSTP while it is blocked
        system.raise(SIGTSTP).now_or_never().unwrap().unwrap();

        let mut readers = vec![];
        let mut writers = vec![];

        let mut select = pin!(system.select(
            &mut readers,
            &mut writers,
            Some(Duration::from_secs(1)),
            Some(&[])
        ));

        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());

        // The select call temporarily unblocks SIGTSTP, so the pending signal should be delivered,
        // which suspends the process. The parent process should receive a SIGCHLD signal.
        {
            let state = system.state.borrow();
            let ppid = state.processes[&system.process_id].ppid;
            assert_eq!(state.processes[&ppid].caught_signals, [SIGCHLD]);
        }

        // Since the process is suspended, the future should not be ready even after the timeout
        system
            .state
            .borrow_mut()
            .advance_time(now + Duration::from_secs(2));
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());

        // When the process is resumed, the future should be woken up
        system.raise(SIGCONT).now_or_never().unwrap().unwrap();
        assert!(woken.is_woken());

        // Polling the future should now return ready
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Ready(Ok(0)));
    }

    /// In this test case, the process receives a SIGTSTP signal while it is
    /// waiting in the `select` call that temporarily blocks SIGTSTP. The
    /// pending signal should be delivered when `select` times out and unblocks
    /// SIGTSTP, which suspends the process. The `select` future should return
    /// pending until the process is resumed.
    #[test]
    fn select_returns_pending_while_process_is_suspended_2() {
        let system = virtual_system_with_parent_process();
        let now = Instant::now();
        system.state.borrow_mut().now = Some(now);

        let mut readers = vec![];
        let mut writers = vec![];

        let mut select = pin!(system.select(
            &mut readers,
            &mut writers,
            Some(Duration::from_secs(1)),
            Some(&[SIGTSTP])
        ));

        // Initially, the future should not be ready.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());

        // While waiting, SIGTSTP is blocked by the temporary signal mask.
        let mut mask = Vec::new();
        system
            .sigmask(None, Some(&mut mask))
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(mask, [SIGTSTP]);

        // Send SIGTSTP while it is blocked. It should remain pending.
        system.raise(SIGTSTP).now_or_never().unwrap().unwrap();
        assert!(!woken.is_woken());

        // Polling again should still be pending.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());

        // Advance time to the timeout. This wakes the future.
        system
            .state
            .borrow_mut()
            .advance_time(now + Duration::from_secs(1));
        assert!(woken.is_woken());

        // Timeout unblocks SIGTSTP and delivers it, suspending the process.
        // The future should remain pending until the process is resumed.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Pending);
        assert!(!woken.is_woken());

        // The parent process should have caught SIGCHLD for the suspension.
        {
            let state = system.state.borrow();
            let ppid = state.processes[&system.process_id].ppid;
            assert_eq!(state.processes[&ppid].caught_signals, [SIGCHLD]);
        }

        // Resuming the process should wake the future.
        system.raise(SIGCONT).now_or_never().unwrap().unwrap();
        assert!(woken.is_woken());

        // Polling the future should now return ready with timeout.
        let woken = Arc::new(WakeFlag::new());
        let waker = Waker::from(Arc::clone(&woken));
        let mut context = Context::from_waker(&waker);
        let poll = select.as_mut().poll(&mut context);
        assert_eq!(poll, Poll::Ready(Ok(0)));
    }
}
