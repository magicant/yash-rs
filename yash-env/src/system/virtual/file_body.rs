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
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::rc::Rc;
use std::task::Waker;

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
}
