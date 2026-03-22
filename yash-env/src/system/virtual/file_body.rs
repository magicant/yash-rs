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
use crate::path::PathBuf;
use crate::str::UnixStr;
use crate::system::Errno;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;
use std::task::Poll::Ready;
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

    /// Returns true if a read operation on this open file description would not
    /// block.
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

    /// Returns true if a write operation on this open file description would
    /// not block.
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
        _context: &mut Context<'_>,
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
                content, writers, ..
            } => {
                let limit = content.len();
                if limit == 0 && *writers > 0 {
                    // TODO: Support blocking read
                    return Ready(Err(Errno::EAGAIN));
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
                Ready(Ok(count))
            }

            FileBody::Directory { .. } => Ready(Err(Errno::EISDIR)),

            FileBody::Symlink { target: _ } => Ready(Err(Errno::ENOTSUP)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut context = Context::from_waker(Waker::noop());
        let mut buffer = [0; 10];
        assert_eq!(body.poll_read(&mut context, &mut buffer, 5), Ready(Ok(0)));
        assert_eq!(body.poll_read(&mut context, &mut buffer, 10), Ready(Ok(0)));
    }

    #[test]
    fn regular_file_body_read_more_than_content() {
        let mut body = FileBody::new(b"hello");
        let mut context = Context::from_waker(Waker::noop());
        let mut buffer = [0; 10];
        assert_eq!(body.poll_read(&mut context, &mut buffer, 2), Ready(Ok(3)));
        assert_eq!(&buffer[..3], b"llo");
    }

    #[test]
    fn regular_file_body_read_less_than_content() {
        let mut body = FileBody::new(b"hello");
        let mut context = Context::from_waker(Waker::noop());
        let mut buffer = [0; 3];
        assert_eq!(body.poll_read(&mut context, &mut buffer, 1), Ready(Ok(3)));
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
        let mut context = Context::from_waker(Waker::noop());
        let mut buffer = [0; 10];
        assert_eq!(body.poll_read(&mut context, &mut buffer, 0), Ready(Ok(0)));
    }

    #[test]
    fn fifo_file_body_read_empty() {
        // The FIFO content is empty but there are writers that may write to it,
        // so the read operation would block.
        let mut body = FileBody::Fifo {
            content: VecDeque::new(),
            readers: 0,
            writers: 1,
            pending_open_wakers: Vec::new(),
            pending_read_wakers: Vec::new(),
            pending_write_wakers: Vec::new(),
        };
        let mut context = Context::from_waker(Waker::noop());
        let mut buffer = [0; 10];
        assert_eq!(
            body.poll_read(&mut context, &mut buffer, 0),
            Ready(Err(Errno::EAGAIN))
        );
        // TODO: Test blocking read once it is implemented
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
        let mut context = Context::from_waker(Waker::noop());
        let mut buffer = [0; 10];
        assert_eq!(body.poll_read(&mut context, &mut buffer, 0), Ready(Ok(5)));
        assert_eq!(&buffer[..5], b"hello");
    }
}
