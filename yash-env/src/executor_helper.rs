// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
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

//! Implementation of traits defined in this crate for external items

use std::pin::Pin;

/// Allows `Spawner` to be used as an `Executor` in the virtual system.
///
/// Remember that `yash_executor::Spawner` is for single-threaded processes.
/// It is not safe to use it in a multi-threaded context, e.g. by spawning a
/// task that creates threads and uses wakers from the executor in those
/// threads.
impl<'a> crate::system::r#virtual::Executor for yash_executor::Spawner<'a> {
    fn spawn(
        &self,
        task: Pin<Box<dyn Future<Output = ()>>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // SAFETY: Actually this is not safe if the task creates a thread and
        // a waker from the executor is used in the thread. However, the shell
        // process must be single-threaded to work correctly, so we assume the
        // task does not create threads.
        (unsafe { self.spawn_pinned(task) })
            .map_err(|_| "failed to spawn task: the executor has been dropped".into())
    }
}
