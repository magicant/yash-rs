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

//! Tests related to FIFO files

use super::tests::virtual_system_with_executor;
use super::*;
use assert_matches::assert_matches;
use futures_util::FutureExt;

fn create_fifo(system: &VirtualSystem) {
    system
        .state
        .borrow_mut()
        .file_system
        .save(
            "/myfifo",
            Rc::new(RefCell::new(Inode {
                body: FileBody::Fifo {
                    content: VecDeque::new(),
                    readers: 0,
                    writers: 0,
                    awaiters: Vec::new(),
                },
                permissions: Mode::empty(),
            })),
        )
        .unwrap();
}

#[test]
fn open_fifo_reading_then_writing_blocking() {
    let (system, mut executor) = virtual_system_with_executor();
    create_fifo(&system);

    // Open the FIFO for reading in a concurrent task
    let system_2 = system.clone();
    let read_open_result = Rc::new(Cell::new(None));
    let read_open_result_2 = read_open_result.clone();
    executor
        .spawner()
        .spawn(Box::pin(async move {
            let result = system_2
                .open(
                    c"/myfifo",
                    OfdAccess::ReadOnly,
                    EnumSet::empty(),
                    Mode::empty(),
                )
                .await;
            read_open_result_2.set(Some(result));
        }))
        .unwrap();

    // The reader should be blocked until a writer opens the FIFO
    executor.run_until_stalled();
    assert_eq!(read_open_result.get(), None);

    // Open the FIFO for writing in another concurrent task
    let write_open_result = Rc::new(Cell::new(None));
    let write_open_result_2 = write_open_result.clone();
    executor
        .spawner()
        .spawn(Box::pin(async move {
            let result = system
                .open(
                    c"/myfifo",
                    OfdAccess::WriteOnly,
                    EnumSet::empty(),
                    Mode::empty(),
                )
                .await;
            write_open_result_2.set(Some(result));
        }))
        .unwrap();

    // Now both reader and writer should be unblocked
    executor.run_until_stalled();
    assert_matches!(
        (read_open_result.get(), write_open_result.get()),
        (Some(Ok(read_fd)), Some(Ok(write_fd))) if read_fd != write_fd
    )
}

#[test]
fn open_fifo_writing_then_reading_blocking() {
    let (system, mut executor) = virtual_system_with_executor();
    create_fifo(&system);

    // Open the FIFO for writing in a concurrent task
    let system_2 = system.clone();
    let write_open_result = Rc::new(Cell::new(None));
    let write_open_result_2 = write_open_result.clone();
    executor
        .spawner()
        .spawn(Box::pin(async move {
            let result = system_2
                .open(
                    c"/myfifo",
                    OfdAccess::WriteOnly,
                    EnumSet::empty(),
                    Mode::empty(),
                )
                .await;
            write_open_result_2.set(Some(result));
        }))
        .unwrap();

    // The writer should be blocked until a reader opens the FIFO
    executor.run_until_stalled();
    assert_eq!(write_open_result.get(), None);

    // Open the FIFO for reading in another concurrent task
    let read_open_result = Rc::new(Cell::new(None));
    let read_open_result_2 = read_open_result.clone();
    executor
        .spawner()
        .spawn(Box::pin(async move {
            let result = system
                .open(
                    c"/myfifo",
                    OfdAccess::ReadOnly,
                    EnumSet::empty(),
                    Mode::empty(),
                )
                .await;
            read_open_result_2.set(Some(result));
        }))
        .unwrap();

    // Now both reader and writer should be unblocked
    executor.run_until_stalled();
    assert_matches!(
        (write_open_result.get(), read_open_result.get()),
        (Some(Ok(write_fd)), Some(Ok(read_fd))) if write_fd != read_fd
    );
}

#[test]
fn open_fifo_reading_and_writing_does_not_block() {
    let system = VirtualSystem::new();
    create_fifo(&system);

    // Open the FIFO for reading and writing
    let result = system
        .open(
            c"/myfifo",
            OfdAccess::ReadWrite,
            EnumSet::empty(),
            Mode::empty(),
        )
        .now_or_never()
        .unwrap();

    assert_matches!(result, Ok(_));
}

#[test]
fn open_fifo_reading_nonblocking() {
    let system = VirtualSystem::new();
    create_fifo(&system);

    // Open the FIFO for reading with O_NONBLOCK
    let result = system
        .open(
            c"/myfifo",
            OfdAccess::ReadOnly,
            EnumSet::only(OpenFlag::NonBlock),
            Mode::empty(),
        )
        .now_or_never()
        .unwrap();

    assert_matches!(result, Ok(_));
}

#[test]
fn open_fifo_writing_nonblocking_without_readers() {
    let system = VirtualSystem::new();
    create_fifo(&system);

    // Open the FIFO for writing with O_NONBLOCK without readers
    // POSIX specifies that this should fail with ENXIO
    let result = system
        .open(
            c"/myfifo",
            OfdAccess::WriteOnly,
            EnumSet::only(OpenFlag::NonBlock),
            Mode::empty(),
        )
        .now_or_never()
        .unwrap();

    assert_matches!(result, Err(Errno::ENXIO));
}

#[test]
fn open_fifo_writing_nonblocking_with_readers() {
    let system = VirtualSystem::new();
    create_fifo(&system);

    // First, open the FIFO for reading
    let read_result = system
        .open(
            c"/myfifo",
            OfdAccess::ReadOnly,
            EnumSet::only(OpenFlag::NonBlock),
            Mode::empty(),
        )
        .now_or_never()
        .unwrap();
    assert_matches!(read_result, Ok(_));

    // Now open the FIFO for writing with O_NONBLOCK (should succeed)
    let write_result = system
        .open(
            c"/myfifo",
            OfdAccess::WriteOnly,
            EnumSet::only(OpenFlag::NonBlock),
            Mode::empty(),
        )
        .now_or_never()
        .unwrap();

    assert_matches!(write_result, Ok(_));
}
