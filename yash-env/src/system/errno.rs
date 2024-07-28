// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Error values
//!
//! This module provides the [`Errno`] type, which is a thin wrapper around
//! the `errno` value returned from underlying system calls.

/// Raw error value
///
/// Currently, this is a `i32` value on all platforms. In the future, some
/// platforms may possibly appear that use a different type for error values, so
/// this type is used to abstract over the underlying type. For the best
/// compatibility, you should not assume that this type is an `i32` on all
/// platforms.
pub type RawErrno = i32;

/// Error value
///
/// This is a new type pattern around the [raw error value](RawErrno). The
/// advantage of using this type is that it is more type-safe than using the
/// raw error value directly. Compared to [`std::io::Error`], this type is
/// more lightweight and implements the `Copy` trait, so it is more suitable
/// for use in low-level [system](super::System) functions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct Errno(pub RawErrno);

impl Errno {
    /// Dummy error value that does not equal any real error value
    ///
    /// This value is defined as `0`.
    pub const NO_ERROR: Self = Self(0);
}

#[doc = include_str!("errno.md")]
#[cfg(unix)]
impl Errno {
    /// Argument list too long
    pub const E2BIG: Self = Self(nix::libc::E2BIG as _);
    /// Permission denied
    pub const EACCES: Self = Self(nix::libc::EACCES as _);
    /// Address in use.
    pub const EADDRINUSE: Self = Self(nix::libc::EADDRINUSE as _);
    /// Address not available
    pub const EADDRNOTAVAIL: Self = Self(nix::libc::EADDRNOTAVAIL as _);
    /// Address family not supported
    pub const EAFNOSUPPORT: Self = Self(nix::libc::EAFNOSUPPORT as _);
    /// Resource unavailable, try again (may be the same value as [`EWOULDBLOCK`](Self::EWOULDBLOCK))
    pub const EAGAIN: Self = Self(nix::libc::EAGAIN as _);
    /// Connection already in progress
    pub const EALREADY: Self = Self(nix::libc::EALREADY as _);
    /// Bad file descriptor
    pub const EBADF: Self = Self(nix::libc::EBADF as _);
    /// Bad message
    pub const EBADMSG: Self = Self(nix::libc::EBADMSG as _);
    /// Device or resource busy
    pub const EBUSY: Self = Self(nix::libc::EBUSY as _);
    /// Operation canceled
    pub const ECANCELED: Self = Self(nix::libc::ECANCELED as _);
    /// No child processes
    pub const ECHILD: Self = Self(nix::libc::ECHILD as _);
    /// Connection aborted
    pub const ECONNABORTED: Self = Self(nix::libc::ECONNABORTED as _);
    /// Connection refused
    pub const ECONNREFUSED: Self = Self(nix::libc::ECONNREFUSED as _);
    /// Connection reset
    pub const ECONNRESET: Self = Self(nix::libc::ECONNRESET as _);
    /// Resource deadlock would occur
    pub const EDEADLK: Self = Self(nix::libc::EDEADLK as _);
    /// Destination address required
    pub const EDESTADDRREQ: Self = Self(nix::libc::EDESTADDRREQ as _);
    /// Mathematics argument out of domain of function
    pub const EDOM: Self = Self(nix::libc::EDOM as _);
    /// Reserved
    pub const EDQUOT: Self = Self(nix::libc::EDQUOT as _);
    /// File exists
    pub const EEXIST: Self = Self(nix::libc::EEXIST as _);
    /// Bad address
    pub const EFAULT: Self = Self(nix::libc::EFAULT as _);
    /// File too large
    pub const EFBIG: Self = Self(nix::libc::EFBIG as _);
    /// Host is unreachable
    pub const EHOSTUNREACH: Self = Self(nix::libc::EHOSTUNREACH as _);
    /// Identifier removed
    pub const EIDRM: Self = Self(nix::libc::EIDRM as _);
    /// Illegal byte sequence
    pub const EILSEQ: Self = Self(nix::libc::EILSEQ as _);
    /// Operation in progress
    pub const EINPROGRESS: Self = Self(nix::libc::EINPROGRESS as _);
    /// Interrupted function
    pub const EINTR: Self = Self(nix::libc::EINTR as _);
    /// Invalid argument
    pub const EINVAL: Self = Self(nix::libc::EINVAL as _);
    /// I/O error
    pub const EIO: Self = Self(nix::libc::EIO as _);
    /// Socket is connected
    pub const EISCONN: Self = Self(nix::libc::EISCONN as _);
    /// Is a directory
    pub const EISDIR: Self = Self(nix::libc::EISDIR as _);
    /// Too many levels of symbolic links
    pub const ELOOP: Self = Self(nix::libc::ELOOP as _);
    /// File descriptor value too large
    pub const EMFILE: Self = Self(nix::libc::EMFILE as _);
    /// Too many links
    pub const EMLINK: Self = Self(nix::libc::EMLINK as _);
    /// Message too large
    pub const EMSGSIZE: Self = Self(nix::libc::EMSGSIZE as _);
    // Not supported on every platform /// Reserved
    // pub const EMULTIHOP: Self = Self(nix::libc::EMULTIHOP as _);
    /// Filename too long
    pub const ENAMETOOLONG: Self = Self(nix::libc::ENAMETOOLONG as _);
    /// Network is down
    pub const ENETDOWN: Self = Self(nix::libc::ENETDOWN as _);
    /// Connection aborted by network
    pub const ENETRESET: Self = Self(nix::libc::ENETRESET as _);
    /// Network unreachable
    pub const ENETUNREACH: Self = Self(nix::libc::ENETUNREACH as _);
    /// Too many files open in system
    pub const ENFILE: Self = Self(nix::libc::ENFILE as _);
    /// No buffer space available
    pub const ENOBUFS: Self = Self(nix::libc::ENOBUFS as _);
    // Not supported on every platform /// No message is available on the STREAM head read queue
    // pub const ENODATA: Self = Self(nix::libc::ENODATA as _);
    /// No such device
    pub const ENODEV: Self = Self(nix::libc::ENODEV as _);
    /// No such file or directory
    pub const ENOENT: Self = Self(nix::libc::ENOENT as _);
    /// Executable file format error
    pub const ENOEXEC: Self = Self(nix::libc::ENOEXEC as _);
    /// No locks available
    pub const ENOLCK: Self = Self(nix::libc::ENOLCK as _);
    // Not supported on every platform /// Reserved
    // pub const ENOLINK: Self = Self(nix::libc::ENOLINK as _);
    /// Not enough space
    pub const ENOMEM: Self = Self(nix::libc::ENOMEM as _);
    /// No message of the desired type
    pub const ENOMSG: Self = Self(nix::libc::ENOMSG as _);
    /// Protocol not available
    pub const ENOPROTOOPT: Self = Self(nix::libc::ENOPROTOOPT as _);
    /// No space left on device
    pub const ENOSPC: Self = Self(nix::libc::ENOSPC as _);
    // Obsolete: Not supported /// No STREAM resources
    // pub const ENOSR: Self = Self(nix::libc::ENOSR as _);
    // Obsolete: Not supported /// Not a STREAM
    // pub const ENOSTR: Self = Self(nix::libc::ENOSTR as _);
    /// Functionality not supported
    pub const ENOSYS: Self = Self(nix::libc::ENOSYS as _);
    /// The socket is not connected
    pub const ENOTCONN: Self = Self(nix::libc::ENOTCONN as _);
    /// Not a directory or a symbolic link to a directory
    pub const ENOTDIR: Self = Self(nix::libc::ENOTDIR as _);
    /// Directory not empty
    pub const ENOTEMPTY: Self = Self(nix::libc::ENOTEMPTY as _);
    // Not supported on every platform /// State not recoverable
    // pub const ENOTRECOVERABLE: Self = Self(nix::libc::ENOTRECOVERABLE as _);
    /// Not a socket
    pub const ENOTSOCK: Self = Self(nix::libc::ENOTSOCK as _);
    /// Not supported (may be the same value as [`EOPNOTSUPP`](Self::EOPNOTSUPP))
    pub const ENOTSUP: Self = Self(nix::libc::ENOTSUP as _);
    /// Inappropriate I/O control operation
    pub const ENOTTY: Self = Self(nix::libc::ENOTTY as _);
    /// No such device or address
    pub const ENXIO: Self = Self(nix::libc::ENXIO as _);
    /// Operation not supported on socket (may be the same value as [`ENOTSUP`](Self::ENOTSUP))
    pub const EOPNOTSUPP: Self = Self(nix::libc::EOPNOTSUPP as _);
    /// Value too large to be stored in data type
    pub const EOVERFLOW: Self = Self(nix::libc::EOVERFLOW as _);
    // Not supported on every platform /// Previous owner died
    // pub const EOWNERDEAD: Self = Self(nix::libc::EOWNERDEAD as _);
    /// Operation not permitted
    pub const EPERM: Self = Self(nix::libc::EPERM as _);
    /// Broken pipe
    pub const EPIPE: Self = Self(nix::libc::EPIPE as _);
    /// Protocol error
    pub const EPROTO: Self = Self(nix::libc::EPROTO as _);
    /// Protocol not supported
    pub const EPROTONOSUPPORT: Self = Self(nix::libc::EPROTONOSUPPORT as _);
    /// Protocol wrong type for socket
    pub const EPROTOTYPE: Self = Self(nix::libc::EPROTOTYPE as _);
    /// Result too large
    pub const ERANGE: Self = Self(nix::libc::ERANGE as _);
    /// Read-only file system
    pub const EROFS: Self = Self(nix::libc::EROFS as _);
    /// Invalid seek
    pub const ESPIPE: Self = Self(nix::libc::ESPIPE as _);
    /// No such process
    pub const ESRCH: Self = Self(nix::libc::ESRCH as _);
    /// Reserved
    pub const ESTALE: Self = Self(nix::libc::ESTALE as _);
    // Obsolete: Not supported /// Stream ioctl() timeout
    // pub const ETIME: Self = Self(nix::libc::ETIME as _);
    /// Connection timed out
    pub const ETIMEDOUT: Self = Self(nix::libc::ETIMEDOUT as _);
    /// Text file busy
    pub const ETXTBSY: Self = Self(nix::libc::ETXTBSY as _);
    /// Operation would block (may be the same value as [`EAGAIN`](Self::EAGAIN))
    pub const EWOULDBLOCK: Self = Self(nix::libc::EWOULDBLOCK as _);
    /// Cross-device link
    pub const EXDEV: Self = Self(nix::libc::EXDEV as _);
}

#[doc = include_str!("errno.md")]
#[cfg(not(unix))]
impl Errno {
    /// Argument list too long
    pub const E2BIG: Self = Self(1);
    /// Permission denied
    pub const EACCES: Self = Self(2);
    /// Address in use.
    pub const EADDRINUSE: Self = Self(3);
    /// Address not available
    pub const EADDRNOTAVAIL: Self = Self(4);
    /// Address family not supported
    pub const EAFNOSUPPORT: Self = Self(5);
    /// Resource unavailable, try again (may be the same value as [`EWOULDBLOCK`](Self::EWOULDBLOCK))
    pub const EAGAIN: Self = Self(6);
    /// Connection already in progress
    pub const EALREADY: Self = Self(7);
    /// Bad file descriptor
    pub const EBADF: Self = Self(8);
    /// Bad message
    pub const EBADMSG: Self = Self(9);
    /// Device or resource busy
    pub const EBUSY: Self = Self(10);
    /// Operation canceled
    pub const ECANCELED: Self = Self(11);
    /// No child processes
    pub const ECHILD: Self = Self(12);
    /// Connection aborted
    pub const ECONNABORTED: Self = Self(13);
    /// Connection refused
    pub const ECONNREFUSED: Self = Self(14);
    /// Connection reset
    pub const ECONNRESET: Self = Self(15);
    /// Resource deadlock would occur
    pub const EDEADLK: Self = Self(16);
    /// Destination address required
    pub const EDESTADDRREQ: Self = Self(17);
    /// Mathematics argument out of domain of function
    pub const EDOM: Self = Self(18);
    /// Reserved
    pub const EDQUOT: Self = Self(19);
    /// File exists
    pub const EEXIST: Self = Self(20);
    /// Bad address
    pub const EFAULT: Self = Self(21);
    /// File too large
    pub const EFBIG: Self = Self(22);
    /// Host is unreachable
    pub const EHOSTUNREACH: Self = Self(23);
    /// Identifier removed
    pub const EIDRM: Self = Self(24);
    /// Illegal byte sequence
    pub const EILSEQ: Self = Self(25);
    /// Operation in progress
    pub const EINPROGRESS: Self = Self(26);
    /// Interrupted function
    pub const EINTR: Self = Self(27);
    /// Invalid argument
    pub const EINVAL: Self = Self(28);
    /// I/O error
    pub const EIO: Self = Self(29);
    /// Socket is connected
    pub const EISCONN: Self = Self(30);
    /// Is a directory
    pub const EISDIR: Self = Self(31);
    /// Too many levels of symbolic links
    pub const ELOOP: Self = Self(32);
    /// File descriptor value too large
    pub const EMFILE: Self = Self(33);
    /// Too many links
    pub const EMLINK: Self = Self(34);
    /// Message too large
    pub const EMSGSIZE: Self = Self(35);
    // Not supported on every platform /// Reserved
    // pub const EMULTIHOP: Self = Self(36);
    /// Filename too long
    pub const ENAMETOOLONG: Self = Self(37);
    /// Network is down
    pub const ENETDOWN: Self = Self(38);
    /// Connection aborted by network
    pub const ENETRESET: Self = Self(39);
    /// Network unreachable
    pub const ENETUNREACH: Self = Self(40);
    /// Too many files open in system
    pub const ENFILE: Self = Self(41);
    /// No buffer space available
    pub const ENOBUFS: Self = Self(42);
    // Not supported on every platform /// No message is available on the STREAM head read queue
    // pub const ENODATA: Self = Self(43);
    /// No such device
    pub const ENODEV: Self = Self(44);
    /// No such file or directory
    pub const ENOENT: Self = Self(45);
    /// Executable file format error
    pub const ENOEXEC: Self = Self(46);
    /// No locks available
    pub const ENOLCK: Self = Self(47);
    // Not supported on every platform /// Reserved
    // pub const ENOLINK: Self = Self(48);
    /// Not enough space
    pub const ENOMEM: Self = Self(49);
    /// No message of the desired type
    pub const ENOMSG: Self = Self(50);
    /// Protocol not available
    pub const ENOPROTOOPT: Self = Self(51);
    /// No space left on device
    pub const ENOSPC: Self = Self(52);
    // Obsolete: Not supported /// No STREAM resources
    // pub const ENOSR: Self = Self(53);
    // Obsolete: Not supported /// Not a STREAM
    // pub const ENOSTR: Self = Self(54);
    /// Functionality not supported
    pub const ENOSYS: Self = Self(55);
    /// The socket is not connected
    pub const ENOTCONN: Self = Self(56);
    /// Not a directory or a symbolic link to a directory
    pub const ENOTDIR: Self = Self(57);
    /// Directory not empty
    pub const ENOTEMPTY: Self = Self(58);
    // Not supported on every platform /// State not recoverable
    // pub const ENOTRECOVERABLE: Self = Self(59);
    /// Not a socket
    pub const ENOTSOCK: Self = Self(60);
    /// Not supported (may be the same value as [`EOPNOTSUPP`](Self::EOPNOTSUPP))
    pub const ENOTSUP: Self = Self(61);
    /// Inappropriate I/O control operation
    pub const ENOTTY: Self = Self(62);
    /// No such device or address
    pub const ENXIO: Self = Self(63);
    /// Operation not supported on socket (may be the same value as [`ENOTSUP`](Self::ENOTSUP))
    pub const EOPNOTSUPP: Self = Self(64);
    /// Value too large to be stored in data type
    pub const EOVERFLOW: Self = Self(65);
    // Not supported on every platform /// Previous owner died
    // pub const EOWNERDEAD: Self = Self(66);
    /// Operation not permitted
    pub const EPERM: Self = Self(67);
    /// Broken pipe
    pub const EPIPE: Self = Self(68);
    /// Protocol error
    pub const EPROTO: Self = Self(69);
    /// Protocol not supported
    pub const EPROTONOSUPPORT: Self = Self(70);
    /// Protocol wrong type for socket
    pub const EPROTOTYPE: Self = Self(71);
    /// Result too large
    pub const ERANGE: Self = Self(72);
    /// Read-only file system
    pub const EROFS: Self = Self(73);
    /// Invalid seek
    pub const ESPIPE: Self = Self(74);
    /// No such process
    pub const ESRCH: Self = Self(75);
    /// Reserved
    pub const ESTALE: Self = Self(76);
    // Obsolete: Not supported /// Stream ioctl() timeout
    // pub const ETIME: Self = Self(77);
    /// Connection timed out
    pub const ETIMEDOUT: Self = Self(78);
    /// Text file busy
    pub const ETXTBSY: Self = Self(79);
    /// Operation would block (may be the same value as [`EAGAIN`](Self::EAGAIN))
    pub const EWOULDBLOCK: Self = Self(80);
    /// Cross-device link
    pub const EXDEV: Self = Self(81);
}

impl From<Errno> for RawErrno {
    #[inline]
    fn from(errno: Errno) -> Self {
        errno.0
    }
}

impl From<RawErrno> for Errno {
    #[inline]
    fn from(errno: RawErrno) -> Self {
        Self(errno)
    }
}

impl From<Errno> for nix::Error {
    #[inline]
    fn from(errno: Errno) -> Self {
        Self::from_i32(errno.0)
    }
}

impl From<nix::Error> for Errno {
    #[inline]
    fn from(error: nix::Error) -> Self {
        Self(error as RawErrno)
    }
}

impl From<Errno> for std::io::Error {
    #[inline]
    fn from(errno: Errno) -> Self {
        std::io::Error::from_raw_os_error(errno.0)
    }
}

impl std::fmt::Display for Errno {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO Consider using libc::strerror
        std::io::Error::from(*self).fmt(f)
    }
}

impl std::error::Error for Errno {}

// `From<std::io::Error> for Errno` is not implemented because
// `std::io::Error::raw_os_error` returns `Option<i32>` and it is not
// always possible to convert it to `Errno`.

/// Type alias for a result that uses [`Errno`] as the error type.
pub type Result<T> = std::result::Result<T, Errno>;
