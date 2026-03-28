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

//! I/O within a virtual system.

use super::super::Errno;
use super::FdFlag;
use super::FileBody;
use super::Inode;
use enumset::EnumSet;
use std::cell::Cell;
use std::cell::RefCell;
use std::fmt::Debug;
use std::io::SeekFrom;
use std::rc::Rc;
use std::rc::Weak;
use std::task::Poll;
use std::task::Waker;

/// State of a file opened for reading and/or writing
#[derive(Clone, Debug)]
pub struct OpenFileDescription {
    /// File content and metadata
    file: Rc<RefCell<Inode>>,
    /// Position in bytes to perform next I/O operation at
    offset: usize,
    /// Whether this file is opened for reading
    is_readable: bool,
    /// Whether this file is opened for writing
    is_writable: bool,
    /// Whether this file is opened for appending
    is_appending: bool,
    // TODO is_nonblocking
}

impl Drop for OpenFileDescription {
    fn drop(&mut self) {
        let mut file = self.file.borrow_mut();
        file.body.close(self.is_readable, self.is_writable);
    }
}

impl OpenFileDescription {
    /// Creates a new open file description.
    pub(crate) fn new(
        file: Rc<RefCell<Inode>>,
        offset: usize,
        is_readable: bool,
        is_writable: bool,
        is_appending: bool,
    ) -> Self {
        file.borrow_mut().body.open(is_readable, is_writable);

        Self {
            file,
            offset,
            is_readable,
            is_writable,
            is_appending,
        }
    }

    /// Returns the i-node this open file description is operating on.
    #[must_use]
    pub(crate) fn file(&self) -> &Rc<RefCell<Inode>> {
        &self.file
    }

    /// Returns true if you can read from this open file description.
    #[must_use]
    pub fn is_readable(&self) -> bool {
        self.is_readable
    }

    /// Returns true if you can write to this open file description.
    #[must_use]
    pub fn is_writable(&self) -> bool {
        self.is_writable
    }

    /// Returns true if a read operation on this open file description would not
    /// block.
    #[must_use]
    pub fn is_ready_for_reading(&self) -> bool {
        // If this file is not readable, it is considered ready for reading
        // because any read operation on it would immediately fail.
        !self.is_readable || self.file.borrow().body.is_ready_for_reading()
    }

    /// Returns true if a write operation on this open file description would
    /// not block.
    #[must_use]
    pub fn is_ready_for_writing(&self) -> bool {
        // If this file is not writable, it is considered ready for writing
        // because any write operation on it would immediately fail.
        !self.is_writable || self.file.borrow().body.is_ready_for_writing()
    }

    /// Registers a waker to be woken up when this open file description becomes
    /// ready for reading.
    ///
    /// The caller should ensure that the OFD is not
    /// [ready for reading](Self::is_ready_for_reading) when calling this
    /// method, otherwise the waker may never be woken up.
    pub(super) fn register_reader_waker(&mut self, waker: Weak<Cell<Option<Waker>>>) {
        self.file.borrow_mut().body.register_reader_waker(waker);
    }

    /// Registers a waker to be woken up when this open file description becomes
    /// ready for writing.
    ///
    /// The caller should ensure that the OFD is not
    /// [ready for writing](Self::is_ready_for_writing) when calling this
    /// method, otherwise the waker may never be woken up.
    pub(super) fn register_writer_waker(&mut self, waker: Weak<Cell<Option<Waker>>>) {
        self.file.borrow_mut().body.register_writer_waker(waker);
    }

    /// Reads from this open file description.
    ///
    /// Returns the number of bytes successfully read.
    ///
    /// This function does not support blocking read. If the file is not ready
    /// for reading, it returns `Err(Errno::EAGAIN)`. Use
    /// [`poll_read`](Self::poll_read) for polling support.
    pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Errno> {
        match self.poll_read(buffer, Weak::new) {
            Poll::Ready(result) => result,
            Poll::Pending => Err(Errno::EAGAIN),
        }
    }

    /// Polls for the result of reading from this open file description.
    ///
    /// The `get_waker` parameter is a function that returns a weak reference to
    /// the waker of the current task. It is used to register the waker for
    /// pending read operations on files like FIFOs. The function is called only
    /// when the read operation would block, so it can be used to avoid
    /// unnecessary allocations of wakers when the operation can complete
    /// immediately. Since the waker is passed as a weak reference, the caller
    /// must ensure that there is a strong reference to the waker that lives at
    /// least until the file body wakes it up, otherwise the weak reference may
    /// become invalid and the task may not be woken up correctly. The waker is
    /// wrapped in `Cell<Option<Waker>>` to allow it to be shared among multiple
    /// wake conditions and to allow it to be taken by the first condition that
    /// wakes the task.
    ///
    /// The returned `Poll` indicates whether the read operation has completed
    /// or is still pending. If it is `Poll::Ready`, the contained `Result`
    /// indicates whether the read was successful and how many bytes were read,
    /// or if it failed with an error. If it is `Poll::Pending`, it means a
    /// waker has been registered and the caller should wait until it is woken
    /// up, when this method should be called again.
    pub fn poll_read<F>(&mut self, buffer: &mut [u8], get_waker: F) -> Poll<Result<usize, Errno>>
    where
        F: FnMut() -> Weak<Cell<Option<Waker>>>,
    {
        if !self.is_readable {
            return Poll::Ready(Err(Errno::EBADF));
        }

        let file = self.file.borrow_mut();
        let poll = { file }.body.poll_read(buffer, self.offset, get_waker);

        if let Poll::Ready(Ok(count)) = poll {
            self.offset += count;
        }

        poll
    }

    /// Writes to this open file description.
    ///
    /// Returns the number of bytes successfully written.
    pub fn write(&mut self, buffer: &[u8]) -> Result<usize, Errno> {
        match self.poll_write(buffer, Weak::new) {
            Poll::Ready(result) => result,
            Poll::Pending => Err(Errno::EAGAIN),
        }
    }

    /// Polls for the result of writing to this open file description.
    ///
    /// The `get_waker` parameter is a function that returns a weak reference to
    /// the waker of the current task. It is used to register the waker for
    /// pending write operations on files like FIFOs. The function is called
    /// only when the write operation would block, so it can be used to avoid
    /// unnecessary allocations of wakers when the operation can complete
    /// immediately. Since the waker is passed as a weak reference, the caller
    /// must ensure that there is a strong reference to the waker that lives at
    /// least until the file body wakes it up, otherwise the weak reference may
    /// become invalid and the task may not be woken up correctly. The waker is
    /// wrapped in `Cell<Option<Waker>>` to allow it to be shared among multiple
    /// wake conditions and to allow it to be taken by the first condition that
    /// wakes the task.
    ///
    /// The returned `Poll` indicates whether the write operation has completed
    /// or is still pending. If it is `Poll::Ready`, the contained `Result`
    /// indicates whether the write was successful and how many bytes were
    /// written, or if it failed with an error. If it is `Poll::Pending`, it
    /// means a waker has been registered and the caller should wait until it is
    /// woken up, when this method should be called again.
    pub fn poll_write<F>(&mut self, buffer: &[u8], get_waker: F) -> Poll<Result<usize, Errno>>
    where
        F: FnMut() -> Weak<Cell<Option<Waker>>>,
    {
        if !self.is_writable {
            return Poll::Ready(Err(Errno::EBADF));
        }

        let file = self.file.borrow_mut();
        let offset = if self.is_appending {
            file.body.size()
        } else {
            self.offset
        };

        let poll = { file }.body.poll_write(buffer, offset, get_waker);
        if let Poll::Ready(Ok(count)) = poll {
            self.offset = offset + count;
        }

        poll
    }

    /// Moves the file offset and returns the new offset.
    pub fn seek(&mut self, position: SeekFrom) -> Result<usize, Errno> {
        let len = match &self.file.borrow().body {
            FileBody::Regular { content, .. } => content.len(),
            FileBody::Directory { files, .. } => files.len(),
            FileBody::Fifo { .. } => return Err(Errno::ESPIPE),
            FileBody::Symlink { .. } | FileBody::Terminal { .. } => return Err(Errno::ENOTSUP),
        };

        let new_offset = match position {
            SeekFrom::Start(offset) => offset.try_into().ok(),
            SeekFrom::Current(offset) => offset
                .try_into()
                .ok()
                .and_then(|offset| self.offset.checked_add_signed(offset)),
            SeekFrom::End(offset) => offset
                .try_into()
                .ok()
                .and_then(|offset| len.checked_add_signed(offset)),
        };

        let new_offset = new_offset.ok_or(Errno::EINVAL)?;
        self.offset = new_offset;
        Ok(new_offset)
    }

    /// Returns the i-node this open file description is operating on.
    #[must_use]
    pub fn inode(&self) -> &Rc<RefCell<Inode>> {
        &self.file
    }
}

/// State of a file descriptor.
#[derive(Clone, Debug)]
pub struct FdBody {
    /// Underlying open file description.
    pub open_file_description: Rc<RefCell<OpenFileDescription>>,
    /// Flags for this file descriptor
    pub flags: EnumSet<FdFlag>,
}

impl PartialEq for FdBody {
    fn eq(&self, rhs: &Self) -> bool {
        Rc::ptr_eq(&self.open_file_description, &rhs.open_file_description)
            && self.flags == rhs.flags
    }
}

impl Eq for FdBody {}

#[cfg(test)]
mod tests {
    use super::super::Mode;
    use super::*;
    use assert_matches::assert_matches;
    use std::collections::VecDeque;

    #[test]
    fn regular_file_read_unreadable() {
        let mut open_file = OpenFileDescription {
            file: Rc::new(RefCell::new(Inode::new([]))),
            offset: 0,
            is_readable: false,
            is_writable: false,
            is_appending: false,
        };

        let mut buffer = [0];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Err(Errno::EBADF));
    }

    #[test]
    fn regular_file_read_more_than_content() {
        let mut open_file = OpenFileDescription {
            file: Rc::new(RefCell::new(Inode::new([1, 2, 3]))),
            offset: 1,
            is_readable: true,
            is_writable: false,
            is_appending: false,
        };

        let mut buffer = [0; 3];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Ok(2));
        assert_eq!(open_file.offset, 3);
        assert_eq!(buffer[..2], [2, 3]);
    }

    #[test]
    fn regular_file_write_unwritable() {
        let mut open_file = OpenFileDescription {
            file: Rc::new(RefCell::new(Inode::new([]))),
            offset: 0,
            is_readable: false,
            is_writable: false,
            is_appending: false,
        };

        let result = open_file.write(&[0]);
        assert_eq!(result, Err(Errno::EBADF));
    }

    #[test]
    fn regular_file_write_more_than_content() {
        let mut open_file = OpenFileDescription {
            file: Rc::new(RefCell::new(Inode::new([1, 2, 3]))),
            offset: 1,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };

        let result = open_file.write(&[9, 8, 7, 6]);
        assert_eq!(result, Ok(4));
        assert_eq!(open_file.offset, 5);
        assert_matches!(
            &open_file.file.borrow().body,
            FileBody::Regular { content, .. } => {
                assert_eq!(content[..], [1, 9, 8, 7, 6]);
            }
        );
    }

    #[test]
    fn regular_file_write_appending() {
        let mut open_file = OpenFileDescription {
            file: Rc::new(RefCell::new(Inode::new([1, 2, 3]))),
            offset: 1,
            is_readable: false,
            is_writable: true,
            is_appending: true,
        };

        let result = open_file.write(&[4, 5]);
        assert_eq!(result, Ok(2));
        assert_eq!(open_file.offset, 5);
        assert_matches!(
            &open_file.file.borrow().body,
            FileBody::Regular { content, .. } => {
                assert_eq!(content[..], [1, 2, 3, 4, 5]);
            }
        );
    }

    #[test]
    fn regular_file_seek_from_start() {
        let mut open_file = OpenFileDescription {
            file: Rc::new(RefCell::new(Inode::new([]))),
            offset: 3,
            is_readable: true,
            is_writable: true,
            is_appending: false,
        };

        let result = open_file.seek(SeekFrom::Start(10));
        assert_eq!(result, Ok(10));
        assert_eq!(open_file.offset, 10);

        let result = open_file.seek(SeekFrom::Start(0));
        assert_eq!(result, Ok(0));
        assert_eq!(open_file.offset, 0);

        let result = open_file.seek(SeekFrom::Start(3));
        assert_eq!(result, Ok(3));
        assert_eq!(open_file.offset, 3);
    }

    #[test]
    fn regular_file_seek_from_current() {
        let mut open_file = OpenFileDescription {
            file: Rc::new(RefCell::new(Inode::new([]))),
            offset: 5,
            is_readable: true,
            is_writable: true,
            is_appending: false,
        };

        let result = open_file.seek(SeekFrom::Current(10));
        assert_eq!(result, Ok(15));
        assert_eq!(open_file.offset, 15);

        let result = open_file.seek(SeekFrom::Current(0));
        assert_eq!(result, Ok(15));
        assert_eq!(open_file.offset, 15);

        let result = open_file.seek(SeekFrom::Current(-5));
        assert_eq!(result, Ok(10));
        assert_eq!(open_file.offset, 10);

        let result = open_file.seek(SeekFrom::Current(-11));
        assert_eq!(result, Err(Errno::EINVAL));
        assert_eq!(open_file.offset, 10);
    }

    #[test]
    fn regular_file_seek_from_end() {
        let mut open_file = OpenFileDescription {
            file: Rc::new(RefCell::new(Inode::new([1, 2, 3]))),
            offset: 2,
            is_readable: true,
            is_writable: true,
            is_appending: false,
        };

        let result = open_file.seek(SeekFrom::End(7));
        assert_eq!(result, Ok(10));
        assert_eq!(open_file.offset, 10);

        let result = open_file.seek(SeekFrom::End(0));
        assert_eq!(result, Ok(3));
        assert_eq!(open_file.offset, 3);

        let result = open_file.seek(SeekFrom::End(-3));
        assert_eq!(result, Ok(0));
        assert_eq!(open_file.offset, 0);

        let result = open_file.seek(SeekFrom::End(-4));
        assert_eq!(result, Err(Errno::EINVAL));
        assert_eq!(open_file.offset, 0);
    }

    #[test]
    fn fifo_reader_drop() {
        let file = Rc::new(RefCell::new(Inode {
            body: FileBody::Fifo {
                content: VecDeque::new(),
                readers: 1,
                writers: 1,
                pending_open_wakers: Vec::new(),
                pending_read_wakers: Vec::new(),
                pending_write_wakers: Vec::new(),
            },
            permissions: Mode::default(),
        }));
        let open_file = OpenFileDescription {
            file: Rc::clone(&file),
            offset: 0,
            is_readable: true,
            is_writable: false,
            is_appending: false,
        };
        drop(open_file);

        assert_matches!(&file.borrow().body, FileBody::Fifo { readers, writers, .. } => {
            assert_eq!(*readers, 0);
            assert_eq!(*writers, 1);
        });
    }

    #[test]
    fn fifo_writer_drop() {
        let file = Rc::new(RefCell::new(Inode {
            body: FileBody::Fifo {
                content: VecDeque::new(),
                readers: 1,
                writers: 1,
                pending_open_wakers: Vec::new(),
                pending_read_wakers: Vec::new(),
                pending_write_wakers: Vec::new(),
            },
            permissions: Mode::default(),
        }));
        let open_file = OpenFileDescription {
            file: Rc::clone(&file),
            offset: 0,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };
        drop(open_file);

        assert_matches!(&file.borrow().body, FileBody::Fifo { readers, writers, .. } => {
            assert_eq!(*readers, 1);
            assert_eq!(*writers, 0);
        });
    }
}
