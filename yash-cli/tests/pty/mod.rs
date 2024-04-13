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

//! Pseudo-terminal handling for scripted tests
//!
//! This module contains functionality to run a test subject in a
//! pseudo-terminal and communicate with it via the master side of the
//! pseudo-terminal. This is used for tests that depends on terminal
//! facilities such as job control.

use crate::run_with_preexec;
use nix::fcntl::{open, OFlag};
use nix::libc;
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt, PtyMaster};
use nix::sys::stat::Mode;
use nix::unistd::{close, getpgrp, setsid, tcgetpgrp};
use std::ffi::c_int;
use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Runs a test subject in a pseudo-terminal.
pub fn run_with_pty(name: &str) {
    let master = prepare_pty_master();
    let slave_path = pty_slave_path(&master);
    let slave = open_pty_slave(&slave_path);
    let raw_master = master.as_raw_fd();
    let raw_slave = slave.as_raw_fd();

    unsafe {
        run_with_preexec(name, move || {
            close(raw_master)?;
            prepare_as_slave(&slave_path)?;
            close(raw_slave)?;
            Ok(())
        });
    }
}

/// Prepares the master side of a pseudo-terminal.
fn prepare_pty_master() -> PtyMaster {
    let master = posix_openpt(OFlag::O_RDWR | OFlag::O_NOCTTY).expect("posix_openpt failed");
    grantpt(&master).expect("grantpt failed");
    unlockpt(&master).expect("unlockpt failed");
    master
}

/// Mutex to serialize access to `ptsname`, which is not thread-safe.
static PTSNAME_MUTEX: Mutex<()> = Mutex::new(());

/// Returns the path of the slave side of a pseudo-terminal.
fn pty_slave_path(master: &PtyMaster) -> PathBuf {
    let _lock = PTSNAME_MUTEX.lock().expect("PTSNAME_MUTEX poisoned");
    unsafe { ptsname(master) }.expect("ptsname failed").into()
}

/// Opens the slave side of a pseudo-terminal.
fn open_pty_slave(path: &Path) -> OwnedFd {
    let raw_fd = open(path, OFlag::O_RDWR | OFlag::O_NOCTTY, Mode::empty()).expect("open failed");
    unsafe { OwnedFd::from_raw_fd(raw_fd) }
}

/// Prepares the slave side of a pseudo-terminal.
///
/// No memory allocation or panicking is allowed in this function because it is
/// called in a child process.
fn prepare_as_slave(slave_path: &Path) -> nix::Result<()> {
    setsid()?;

    // How to become the controlling process of a slave pseudo-terminal is
    // implementation-dependent. We support two implementation schemes:
    // (1) A process automatically becomes the controlling process when it
    // first opens the terminal.
    // (2) A process needs to use the TIOCSCTTY ioctl system call.
    // There is a race condition in both schemes: an unrelated process could
    // become the controlling process before we do, in which case the slave is
    // not our controlling terminal and therefore we should abort.
    let raw_fd = open(slave_path, OFlag::O_RDWR, Mode::empty())?;
    // Although TIOCSCTTY is available in many Unix-like systems, it may not be
    // available in some systems. Please report if you encounter such a system.
    unsafe { libc::ioctl(raw_fd, libc::TIOCSCTTY as _, 0 as c_int) };

    if tcgetpgrp(raw_fd) == Ok(getpgrp()) {
        Ok(())
    } else {
        Err(nix::Error::ENOTSUP)
    }
}
