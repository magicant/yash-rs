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

//! Operations on file contents

use super::super::FileType;
use super::Inode;
use super::wake_all;
use crate::path::PathBuf;
use crate::str::UnixStr;
use crate::system::Errno;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;
use std::task::Poll::{Pending, Ready};
use std::task::{Context, Poll, Waker};

/// Maximum number of bytes guaranteed to be atomic when writing to a pipe.
///
/// This value is for the virtual system implementation.
/// The real system may have a different configuration.
pub const PIPE_BUF: usize = 512;

/// Maximum number of bytes a pipe can hold at a time.
///
/// This value is for the virtual system implementation.
/// The real system may have a different configuration.
pub const PIPE_SIZE: usize = PIPE_BUF * 2;

/// Filetype-specific content of a file
#[derive(Clone, derive_more::Debug, derive_more::Eq, derive_more::PartialEq)]
#[non_exhaustive]
pub enum FileBody {
    /// Regular file
    Regular {
        /// File content
        content: Vec<u8>,
        /// Whether this file is a native binary that can be exec'ed
        is_native_executable: bool,
    },
    /// Directory
    Directory {
        /// Files contained in this directory
        ///
        /// The keys of the hashmap are filenames without any parent directory
        /// components. The hashmap does not contain "." or "..".
        files: HashMap<Rc<UnixStr>, Rc<RefCell<Inode>>>,
        // The hash map contents are reference-counted to allow making cheap
        // copies of them, which is especially handy when traversing entries.
    },
    /// Named pipe
    Fifo {
        /// Content of the pipe
        content: VecDeque<u8>,
        /// Number of open file descriptions reading from this pipe
        readers: usize,
        /// Number of open file descriptions writing to this pipe
        writers: usize,
        /// Wakers of tasks waiting to open the pipe for reading or writing
        ///
        /// A reader and a writer of a pipe are opened synchronously: when a
        /// task attempts to open the pipe for reading, it will wait until
        /// another task opens the pipe for writing, and vice versa. This field
        /// is used to store the wakers of tasks waiting to open the pipe, so
        /// that they can be notified when a new reader or writer is opened.
        #[eq(ignore)]
        #[partial_eq(ignore)]
        pending_open_wakers: Vec<Waker>,
        /// Wakers of tasks waiting to read from the pipe
        ///
        /// When a task attempts to read from an empty pipe, it will wait until
        /// another task writes to the pipe. This field is used to store the
        /// wakers of such tasks, so that they can be notified when new content
        /// is written.
        ///
        /// The waker is wrapped in `Rc<Cell<Option<Waker>>>` to allow it to be
        /// shared among multiple wake conditions like timeouts and signals, and
        /// to allow it to be taken when waking up the task. When the waker is
        /// `None`, it means the task has already been woken up (possibly by
        /// other conditions) and the item can be removed from the queue.
        #[debug("[{} wakers]", pending_read_wakers.len())]
        #[eq(ignore)]
        #[partial_eq(ignore)]
        pending_read_wakers: Vec<Rc<Cell<Option<Waker>>>>,
        /// Wakers of tasks waiting to write to the pipe
        ///
        /// When a task attempts to write to a full pipe, it will wait until
        /// another task reads from the pipe. This field is used to store the
        /// wakers of such tasks, so that they can be notified when content is
        /// read and space is available for writing.
        ///
        /// See the comment on `pending_read_wakers` for the reason why the
        /// waker is wrapped in `Rc<Cell<Option<Waker>>>`.
        #[debug("[{} wakers]", pending_write_wakers.len())]
        #[eq(ignore)]
        #[partial_eq(ignore)]
        pending_write_wakers: Vec<Rc<Cell<Option<Waker>>>>,
    },
    /// Symbolic link
    Symlink {
        /// Path to the file referenced by this symlink
        target: PathBuf,
    },
    /// Terminal device
    ///
    /// This is a dummy device that works like a regular file.
    Terminal {
        /// Virtual file content
        content: Vec<u8>,
    },
    // TODO Other filetypes
}

/// The default file body is an empty regular file.
impl Default for FileBody {
    fn default() -> Self {
        FileBody::Regular {
            content: Vec::default(),
            is_native_executable: bool::default(),
        }
    }
}

impl FileBody {
    /// Creates a regular file body with the given content.
    #[must_use]
    pub fn new<T: Into<Vec<u8>>>(bytes: T) -> Self {
        FileBody::Regular {
            content: bytes.into(),
            is_native_executable: false,
        }
    }

    /// Returns the type of the file.
    #[must_use]
    pub const fn r#type(&self) -> FileType {
        match self {
            Self::Regular { .. } => FileType::Regular,
            Self::Directory { .. } => FileType::Directory,
            Self::Fifo { .. } => FileType::Fifo,
            Self::Symlink { .. } => FileType::Symlink,
            Self::Terminal { .. } => FileType::CharacterDevice,
        }
    }

    /// Returns the size of the file.
    #[must_use]
    pub fn size(&self) -> usize {
        match self {
            Self::Regular { content, .. } => content.len(),
            Self::Directory { files } => files.len(),
            Self::Fifo { content, .. } => content.len(),
            Self::Symlink { target } => target.as_unix_str().len(),
            Self::Terminal { .. } => 0,
        }
    }

    /// Notifies the file body that a new open file description has been opened
    /// for this file, with the given access mode.
    pub(super) fn open(&mut self, is_readable: bool, is_writable: bool) {
        if let Self::Fifo {
            readers,
            writers,
            pending_open_wakers,
            ..
        } = self
        {
            if is_readable {
                *readers += 1;
            }
            if is_writable {
                *writers += 1;
            }
            pending_open_wakers.drain(..).for_each(Waker::wake);
        }
    }

    /// Notifies the file body that an open file description has been closed for
    /// this file, with the given access mode.
    pub(super) fn close(&mut self, is_readable: bool, is_writable: bool) {
        if let Self::Fifo {
            readers,
            writers,
            pending_read_wakers,
            pending_write_wakers,
            ..
        } = self
        {
            if is_readable {
                *readers -= 1;
                if *readers == 0 {
                    // Let writers know that there are no readers,
                    // so they can return an error instead of blocking.
                    wake_all(pending_write_wakers);
                }
            }
            if is_writable {
                *writers -= 1;
                if *writers == 0 {
                    // Let readers know that there are no writers,
                    // so they can return EOF instead of blocking.
                    wake_all(pending_read_wakers);
                }
            }
        }
    }

    /// Returns whether the file supports seeking.
    #[must_use]
    pub fn is_seekable(&self) -> bool {
        match self {
            Self::Regular { .. } => true,
            Self::Directory { .. } => false,
            Self::Fifo { .. } => false,
            Self::Symlink { .. } => false,
            Self::Terminal { .. } => false,
        }
    }

    /// Returns true if a read operation on this file would not block.
    #[must_use]
    pub(super) fn is_ready_for_reading(&self) -> bool {
        match self {
            Self::Regular { .. }
            | Self::Directory { .. }
            | Self::Terminal { .. }
            | Self::Symlink { .. } => true,
            Self::Fifo {
                content, writers, ..
            } => *writers == 0 || !content.is_empty(),
        }
    }

    /// Returns true if a write operation on this file would not block.
    #[must_use]
    pub(super) fn is_ready_for_writing(&self) -> bool {
        match self {
            Self::Regular { .. }
            | Self::Directory { .. }
            | Self::Terminal { .. }
            | Self::Symlink { .. } => true,
            Self::Fifo {
                content, readers, ..
            } => *readers == 0 || PIPE_SIZE - content.len() >= PIPE_BUF,
        }
    }

    /// Registers a waker to be woken up when this file becomes ready for reading.
    pub(super) fn register_reader_waker(&mut self, waker: Rc<Cell<Option<Waker>>>) {
        if let Self::Fifo {
            pending_read_wakers,
            ..
        } = self
        {
            pending_read_wakers.push(waker);
        }
    }

    /// Registers a waker to be woken up when this file becomes ready for writing.
    pub(super) fn register_writer_waker(&mut self, waker: Rc<Cell<Option<Waker>>>) {
        if let Self::Fifo {
            pending_write_wakers,
            ..
        } = self
        {
            pending_write_wakers.push(waker);
        }
    }

    /// Polls for the result of a read operation on this file.
    ///
    /// The `offset` parameter is the offset from which to read, and is only
    /// relevant for seekable files. For non-seekable files, it can be ignored
    /// or set to any value.
    ///
    /// The returned `Poll` indicates whether the read operation has completed
    /// or is still pending. If it is `Poll::Ready`, the contained `Result`
    /// indicates whether the read was successful and how many bytes were read,
    /// or if it failed with an error. If it is `Poll::Pending`, it means a
    /// waker has been registered and the caller should wait until it is woken
    /// up, when this method should be called again.
    pub(super) fn poll_read(
        &mut self,
        context: &Context<'_>,
        mut buffer: &mut [u8],
        offset: usize,
    ) -> Poll<Result<usize, Errno>> {
        match self {
            FileBody::Regular { content, .. } | FileBody::Terminal { content } => {
                let len = content.len();
                if offset >= len {
                    return Ready(Ok(0));
                }
                let limit = len - offset;
                if buffer.len() > limit {
                    buffer = &mut buffer[..limit];
                }
                let count = buffer.len();
                let src = &content[offset..][..count];
                buffer.copy_from_slice(src);
                Ready(Ok(count))
            }

            FileBody::Fifo {
                content,
                writers,
                pending_read_wakers,
                pending_write_wakers,
                ..
            } => {
                let limit = content.len();
                if limit == 0 && *writers > 0 {
                    // Block until any writer writes to the pipe or all writers are closed.
                    pending_read_wakers.push(Rc::new(Cell::new(Some(context.waker().clone()))));
                    return Pending;
                }

                let mut count = 0;
                for to in buffer {
                    if let Some(from) = content.pop_front() {
                        *to = from;
                        count += 1;
                    } else {
                        break;
                    }
                }
                wake_all(pending_write_wakers);
                Ready(Ok(count))
            }

            FileBody::Directory { .. } => Ready(Err(Errno::EISDIR)),

            FileBody::Symlink { target: _ } => Ready(Err(Errno::ENOTSUP)),
        }
    }

    /// Polls for the result of a write operation on this file.
    ///
    /// The `offset` parameter is the offset to which to write, and is only
    /// relevant for seekable files. For non-seekable files, it can be ignored
    /// or set to any value.
    ///
    /// The returned `Poll` indicates whether the write operation has completed
    /// or is still pending. If it is `Poll::Ready`, the contained `Result`
    /// indicates whether the write was successful and how many bytes were
    /// written, or if it failed with an error. If it is `Poll::Pending`, it
    /// means a waker has been registered and the caller should wait until it is
    /// woken up, when this method should be called again.
    pub(super) fn poll_write(
        &mut self,
        context: &Context<'_>,
        mut buffer: &[u8],
        offset: usize,
    ) -> Poll<Result<usize, Errno>> {
        match self {
            FileBody::Regular { content, .. } | FileBody::Terminal { content } => {
                let len = content.len();
                let count = buffer.len();
                if offset > len {
                    let zeroes = offset - len;
                    content.reserve(zeroes + count);
                    content.resize_with(offset, u8::default);
                }
                let limit = count.min(content.len() - offset);
                let dst = &mut content[offset..][..limit];
                dst.copy_from_slice(&buffer[..limit]);
                content.reserve(count - limit);
                content.extend(&buffer[limit..]);
                Ready(Ok(count))
            }

            FileBody::Fifo {
                content,
                readers,
                pending_read_wakers,
                pending_write_wakers,
                ..
            } => {
                if *readers == 0 {
                    // TODO SIGPIPE
                    return Ready(Err(Errno::EPIPE));
                }
                let room = PIPE_SIZE - content.len();
                if room < buffer.len() {
                    if room == 0 || buffer.len() <= PIPE_BUF {
                        // Block until any reader reads from the pipe or all readers are closed.
                        pending_write_wakers
                            .push(Rc::new(Cell::new(Some(context.waker().clone()))));
                        return Pending;
                    }
                    buffer = &buffer[..room];
                }
                content.reserve_exact(room);
                content.extend(buffer);
                debug_assert!(content.len() <= PIPE_SIZE);
                wake_all(pending_read_wakers);
                Ready(Ok(buffer.len()))
            }

            FileBody::Directory { .. } => Ready(Err(Errno::EISDIR)),

            FileBody::Symlink { target: _ } => Ready(Err(Errno::ENOTSUP)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helper::WakeFlag;
    use assert_matches::assert_matches;
    use std::sync::Arc;

    #[test]
    fn fifo_file_body_open_increments_readers_and_writers() {
        let mut body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 0,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };

        body.open(true, false);
        assert_matches!(
            &body,
            FileBody::Fifo { readers, writers, .. } if *readers == 1 && *writers == 0
        );

        body.open(false, true);
        assert_matches!(
            &body,
            FileBody::Fifo { readers, writers, .. } if *readers == 1 && *writers == 1
        );

        body.open(true, true);
        assert_matches!(
            &body,
            FileBody::Fifo { readers, writers, .. } if *readers == 2 && *writers == 2
        );
    }

    #[test]
    fn fifo_file_body_open_wakes_pending_open_wakers() {
        let wake_flag_1 = Arc::new(WakeFlag::new());
        let wake_flag_2 = Arc::new(WakeFlag::new());
        let waker_1 = Waker::from(wake_flag_1.clone());
        let waker_2 = Waker::from(wake_flag_2.clone());
        let mut body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 0,
            writers: 0,
            pending_open_wakers: vec![waker_1, waker_2],
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        body.open(true, false);
        assert!(wake_flag_1.is_woken());
        assert!(wake_flag_2.is_woken());
        assert_matches!(
            &body,
            FileBody::Fifo { pending_open_wakers, .. } if pending_open_wakers.is_empty()
        );
    }

    #[test]
    fn fifo_file_body_close_decrements_readers_and_writers() {
        let mut body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 2,
            writers: 2,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };

        body.close(true, false);
        assert_matches!(
            &body,
            FileBody::Fifo { readers, writers, .. } if *readers == 1 && *writers == 2
        );

        body.close(false, true);
        assert_matches!(
            &body,
            FileBody::Fifo { readers, writers, .. } if *readers == 1 && *writers == 1
        );

        body.close(true, true);
        assert_matches!(
            &body,
            FileBody::Fifo { readers, writers, .. } if *readers == 0 && *writers == 0
        );
    }

    #[test]
    fn fifo_file_body_wakes_pending_read_wakers_if_no_writers_remain() {
        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 1,
            writers: 2,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: vec![Rc::new(Cell::new(Some(waker)))],
            pending_write_wakers: Vec::new(),
        };

        // One writer is closed, but there is still another writer,
        // so the pending read waker should not be woken up.
        body.close(false, true);
        assert!(!wake_flag.is_woken());

        // The other writer is closed, so the pending read waker should be woken up.
        body.close(false, true);
        assert!(wake_flag.is_woken());
    }

    #[test]
    fn fifo_file_body_wakes_pending_write_wakers_if_no_readers_remain() {
        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let mut body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 2,
            writers: 1,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: vec![Rc::new(Cell::new(Some(waker)))],
        };

        // One reader is closed, but there is still another reader,
        // so the pending write waker should not be woken up.
        body.close(true, false);
        assert!(!wake_flag.is_woken());

        // The other reader is closed, so the pending write waker should be woken up.
        body.close(true, false);
        assert!(wake_flag.is_woken());
    }

    #[test]
    fn fifo_file_body_is_ready_for_reading() {
        // When there are no writers, the FIFO is always ready for reading
        // since it will return EOF.
        let body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 0,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        assert!(body.is_ready_for_reading());

        // When there are writers, the FIFO is ready for reading if and only if
        // it has content.
        let body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 0,
            writers: 1,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        assert!(!body.is_ready_for_reading());
        let body = FileBody::Fifo {
            content: VecDeque::from([0]),
            readers: 0,
            writers: 1,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        assert!(body.is_ready_for_reading());
    }

    #[test]
    fn fifo_file_body_is_ready_for_writing() {
        // When there are no readers, the FIFO is always ready for writing
        // since it will return EPIPE.
        let body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 0,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        assert!(body.is_ready_for_writing());

        // When there are readers, the FIFO is ready for writing if and only if
        // it has enough space for at least one atomic write.
        let body = FileBody::Fifo {
            content: VecDeque::from([0; PIPE_SIZE - PIPE_BUF]),
            readers: 1,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        assert!(body.is_ready_for_writing());
        let body = FileBody::Fifo {
            content: VecDeque::from([0; PIPE_SIZE - PIPE_BUF + 1]),
            readers: 1,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        assert!(!body.is_ready_for_writing());
    }

    #[test]
    fn regular_file_body_read_beyond_file_length() {
        let mut body = FileBody::new(b"hello");
        let context = Context::from_waker(Waker::noop());
        let mut buffer = [0; 10];
        assert_eq!(body.poll_read(&context, &mut buffer, 5), Ready(Ok(0)));
        assert_eq!(body.poll_read(&context, &mut buffer, 10), Ready(Ok(0)));
    }

    #[test]
    fn regular_file_body_read_more_than_content() {
        let mut body = FileBody::new(b"hello");
        let context = Context::from_waker(Waker::noop());
        let mut buffer = [0; 10];
        assert_eq!(body.poll_read(&context, &mut buffer, 2), Ready(Ok(3)));
        assert_eq!(&buffer[..3], b"llo");
    }

    #[test]
    fn regular_file_body_read_less_than_content() {
        let mut body = FileBody::new(b"hello");
        let context = Context::from_waker(Waker::noop());
        let mut buffer = [0; 3];
        assert_eq!(body.poll_read(&context, &mut buffer, 1), Ready(Ok(3)));
        assert_eq!(&buffer, b"ell");
    }

    #[test]
    fn fifo_file_body_read_eof() {
        // With no writers, the FIFO returns EOF.
        let mut body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 0,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        let context = Context::from_waker(Waker::noop());
        let mut buffer = [0; 10];
        assert_eq!(body.poll_read(&context, &mut buffer, 0), Ready(Ok(0)));
    }

    #[test]
    fn fifo_file_body_read_empty() {
        // The FIFO content is empty but there are writers that may write to it,
        // so the read operation would block.
        let mut body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 1,
            writers: 1,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        let mut buffer = [0; 10];

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let context = Context::from_waker(&waker);
        let poll = body.poll_read(&context, &mut buffer, 0);
        assert_eq!(poll, Pending);
        assert!(!wake_flag.is_woken());

        // When another task writes to the FIFO, the read operation should be woken up.
        let context = Context::from_waker(Waker::noop());
        let poll = body.poll_write(&context, b"hello", 0);
        assert_eq!(poll, Ready(Ok(5)));
        assert!(wake_flag.is_woken());

        // After being woken up, the read operation should succeed and read the new content.
        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let context = Context::from_waker(&waker);
        let poll = body.poll_read(&context, &mut buffer, 0);
        assert_eq!(poll, Ready(Ok(5)));
        assert_eq!(&buffer[..5], b"hello");
        assert!(!wake_flag.is_woken());
    }

    #[test]
    fn fifo_file_body_read_non_empty() {
        let mut body = FileBody::Fifo {
            content: VecDeque::from(*b"hello"),
            readers: 0,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        let context = Context::from_waker(Waker::noop());
        let mut buffer = [0; 10];
        assert_eq!(body.poll_read(&context, &mut buffer, 0), Ready(Ok(5)));
        assert_eq!(&buffer[..5], b"hello");
    }

    #[test]
    fn regular_file_body_write_less_than_content() {
        let mut body = FileBody::new(b"hello");
        let context = Context::from_waker(Waker::noop());
        let buffer = b"ipp";
        assert_eq!(body.poll_write(&context, buffer, 1), Ready(Ok(3)));
        assert_eq!(body, FileBody::new(b"hippo"));
    }

    #[test]
    fn regular_file_body_write_more_than_content() {
        let mut body = FileBody::new(b"hello");
        let context = Context::from_waker(Waker::noop());
        let buffer = b"icopter";
        assert_eq!(body.poll_write(&context, buffer, 3), Ready(Ok(7)));
        assert_eq!(body, FileBody::new(b"helicopter"));
    }

    #[test]
    fn regular_file_body_write_beyond_file_length() {
        let mut body = FileBody::new(b"hello");
        let context = Context::from_waker(Waker::noop());
        let buffer = b"world";
        assert_eq!(body.poll_write(&context, buffer, 7), Ready(Ok(5)));
        assert_eq!(body, FileBody::new(b"hello\0\0world"));
    }

    #[test]
    fn fifo_file_body_write_closed() {
        // When there are no readers, the FIFO returns EPIPE error.
        let mut body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 0,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        let context = Context::from_waker(Waker::noop());
        let buffer = b"hello";
        assert_eq!(
            body.poll_write(&context, buffer, 0),
            Ready(Err(Errno::EPIPE))
        );
    }

    #[test]
    fn fifo_file_body_write_atomic_empty() {
        // When the FIFO has enough space for an atomic write and there are
        // readers that may read from it, the write operation should succeed.
        let mut body = FileBody::Fifo {
            content: VecDeque::from([0; PIPE_SIZE - PIPE_BUF]),
            readers: 1,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        let context = Context::from_waker(Waker::noop());
        let buffer = [0; PIPE_BUF];
        assert_eq!(body.poll_write(&context, &buffer, 0), Ready(Ok(PIPE_BUF)));
    }

    #[test]
    fn fifo_file_body_write_atomic_full() {
        // When the FIFO does not have enough space for an atomic write but
        // there are readers that may read from it, the write operation would
        // block.
        let mut body = FileBody::Fifo {
            content: VecDeque::from([0; PIPE_SIZE - PIPE_BUF + 1]),
            readers: 1,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        let buffer = [0; PIPE_BUF];

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let context = Context::from_waker(&waker);
        let poll = body.poll_write(&context, &buffer, 0);
        assert_eq!(poll, Pending);
        assert!(!wake_flag.is_woken());

        // When another task reads from the FIFO, the write operation should be woken up.
        let context = Context::from_waker(Waker::noop());
        let mut read_buffer = [0; 1];
        let poll = body.poll_read(&context, &mut read_buffer, 0);
        assert_eq!(poll, Ready(Ok(1)));
        assert!(wake_flag.is_woken());

        // After being woken up, the write operation successfully writes the content to the FIFO.
        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let context = Context::from_waker(&waker);
        let poll = body.poll_write(&context, &buffer, 0);
        assert_eq!(poll, Ready(Ok(PIPE_BUF)));
        assert!(!wake_flag.is_woken());
    }

    #[test]
    fn fifo_file_body_write_non_atomic_empty() {
        // When the write size exceeds PIPE_BUF, the FIFO has space for at least
        // one byte, but there are readers that may read from the FIFO, the
        // write operation should succeed and write as much as possible.
        let mut body = FileBody::Fifo {
            content: VecDeque::from([0; PIPE_SIZE - 1]),
            readers: 1,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        let context = Context::from_waker(Waker::noop());
        let buffer = [0; PIPE_BUF + 1];
        assert_eq!(body.poll_write(&context, &buffer, 0), Ready(Ok(1)));
    }

    #[test]
    fn fifo_file_body_write_non_atomic_full() {
        // When the write size exceeds PIPE_BUF, the FIFO is full, and there are
        // readers that may read from it, the write operation should block until
        // there is space for at least one byte to be written.
        let mut body = FileBody::Fifo {
            content: VecDeque::from([0; PIPE_SIZE]),
            readers: 1,
            writers: 0,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        let buffer = [0; PIPE_BUF + 1];

        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let context = Context::from_waker(&waker);
        let poll = body.poll_write(&context, &buffer, 0);
        assert_eq!(poll, Pending);
        assert!(!wake_flag.is_woken());

        // When another task reads from the FIFO, the write operation should be woken up.
        let context = Context::from_waker(Waker::noop());
        let mut read_buffer = [0; 1];
        let poll = body.poll_read(&context, &mut read_buffer, 0);
        assert_eq!(poll, Ready(Ok(1)));
        assert!(wake_flag.is_woken());

        // After being woken up, the write operation successfully writes one byte to the FIFO.
        let wake_flag = Arc::new(WakeFlag::new());
        let waker = Waker::from(wake_flag.clone());
        let context = Context::from_waker(&waker);
        let poll = body.poll_write(&context, &buffer, 0);
        assert_eq!(poll, Ready(Ok(1)));
        assert!(!wake_flag.is_woken());
    }
}
