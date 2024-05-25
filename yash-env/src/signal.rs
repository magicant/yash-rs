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

//! Type definitions for signals
//!
//! Signals are a method of inter-process communication used to notify a process
//! that a specific event has occurred. This module provides a list of signals
//! defined by POSIX with additional support for some non-standard signals.
//!
//! This module defines two abstractions for signals: [`Name`] and [`Number`].
//! The `Name` type identifies a signal by its name, while the `Number` type
//! represents a signal by its number. This reflects the fact that different
//! systems may use different signal numbers for the same signal name and that
//! some exotic signals may not be available on all systems.
//!
//! A [`Name`] can represent a single signal name, such as `SIGINT`
//! ([`Name::Int`]), regardless of whether the signal is available on the
//! [`System`]. `Name`s are more useful for user-facing applications, as they
//! are easier to read and understand.
//!
//! A [`Number`] represents a signal that is available on the [`System`] by its
//! signal number. The number is guaranteed to be a positive integer, so it
//! optimizes the size of `Option<Number>`, etc. `Number`s are used in most
//! signal-related functions of the [`System`] to efficiently interact with the
//! underlying system calls.
//!
//! All proper signal names start with `"SIG"`. However, the names defined,
//! parsed, and displayed in this module do not include the `"SIG"` prefix.

#[cfg(doc)]
use crate::system::System;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::ffi::c_int;
use std::num::NonZeroI32;
use std::str::FromStr;
use strum::{EnumIter, IntoEnumIterator};
use thiserror::Error;

/// Raw signal number
///
/// This is a type alias for the raw signal number used by the underlying
/// system. POSIX requires valid signal numbers to be positive `c_int` values.
///
/// The current implementation of conversion between `RawNumber` and `Number`
/// assumes that `c_int` is a 32-bit signed integer type. This is a reasonable
/// assumption, as POSIX requires `c_int` to be at least 32 bits wide, but it
/// may break on systems where `c_int` is wider than 32 bits.
pub type RawNumber = c_int;

/// Signal name
///
/// This enum identifies a signal by its name. It can be used to represent
/// signals regardless of whether they are available on the [`System`].
///
/// Use the [`System::validate_signal`] function to obtain a `Name` from a
/// signal number. The [`System::signal_number_from_name`] function can be used
/// to convert a `Name` to a `Number`.
#[derive(Clone, Copy, Debug, EnumIter, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Name {
    /// `SIGABRT` (process abort signal)
    Abrt,
    /// `SIGALRM` (alarm clock)
    Alrm,
    /// `SIGBUS` (access to an undefined portion of a memory object)
    Bus,
    /// `SIGCHLD` (child process terminated, stopped, or continued)
    Chld,
    /// `SIGCLD` (child process terminated, stopped, or continued)
    Cld,
    /// `SIGCONT` (continue executing, if stopped)
    Cont,
    /// `SIGEMT` (emulation trap)
    Emt,
    /// `SIGFPE` (erroneous arithmetic operation)
    Fpe,
    /// `SIGHUP` (hangup)
    Hup,
    /// `SIGILL` (illegal instruction)
    Ill,
    /// `SIGINFO` (status request from keyboard)
    Info,
    /// `SIGINT` (interrupt)
    Int,
    /// `SIGIO` (I/O is possible on a file descriptor)
    Io,
    /// `SIGIOT` (I/O trap)
    Iot,
    /// `SIGKILL` (kill)
    Kill,
    /// `SIGLOST` (resource lost)
    Lost,
    /// `SIGPIPE` (write on a pipe with no one to read it)
    Pipe,
    /// `SIGPOLL` (pollable event)
    Poll,
    /// `SIGPROF` (profiling timer expired)
    Prof,
    /// `SIGPWR` (power failure)
    Pwr,
    /// `SIGQUIT` (quit)
    Quit,
    /// `SIGSEGV` (invalid memory reference)
    Segv,
    /// `SIGSTKFLT` (stack fault)
    Stkflt,
    /// `SIGSTOP` (stop executing)
    Stop,
    /// `SIGSYS` (bad system call)
    Sys,
    /// `SIGTERM` (termination)
    Term,
    /// `SIGTHR` (thread interrupt)
    Thr,
    /// `SIGTRAP` (trace trap)
    Trap,
    /// `SIGTSTP` (stop executing)
    Tstp,
    /// `SIGTTIN` (background process attempting read)
    Ttin,
    /// `SIGTTOU` (background process attempting write)
    Ttou,
    /// `SIGURG` (high bandwidth data is available at a socket)
    Urg,
    /// `SIGUSR1` (user-defined signal 1)
    Usr1,
    /// `SIGUSR2` (user-defined signal 2)
    Usr2,
    /// `SIGVTALRM` (virtual timer expired)
    Vtalrm,
    /// `SIGWINCH` (window size change)
    Winch,
    /// `SIGXCPU` (CPU time limit exceeded)
    Xcpu,
    /// `SIGXFSZ` (file size limit exceeded)
    Xfsz,

    /// Real-time signal with a number relative to the minimum real-time signal
    ///
    /// `Rtmin(n)` represents the real-time signal `SIGRTMIN + n`, where `n` is
    /// expected to be a non-negative integer between `0` and
    /// `SIGRTMAX - SIGRTMIN`.
    Rtmin(RawNumber),

    /// Real-time signal with a number relative to the maximum real-time signal
    ///
    /// `Rtmax(n)` represents the real-time signal `SIGRTMAX + n`, where `n` is
    /// expected to be a non-positive integer between `SIGRTMIN - SIGRTMAX` and
    /// `0`.
    Rtmax(RawNumber),
}

/// Compares two signal names.
///
/// The comparison is allowed only between two `Rtmin` values or two `Rtmax`
/// values. The comparison is based on the numerical value of the signal number.
impl PartialOrd for Name {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Self::Rtmin(a), Self::Rtmin(b)) | (Self::Rtmax(a), Self::Rtmax(b)) => a.partial_cmp(b),
            _ => None,
        }
    }
}

impl Name {
    /// Returns an iterator over all signal names.
    ///
    /// This is a convenience method that returns an iterator over all signal
    /// names. The iterator yields all signal names other than `Rtmin` and
    /// `Rtmax` in the alphabetical order, followed by `Rtmin(0)` and `Rtmax(0)`
    /// as the last two items.
    ///
    /// Note that the iterator works independently of the underlying system and
    /// does not check whether the signals are available on the system.
    #[inline(always)]
    pub fn iter() -> NameIter {
        <Self as IntoEnumIterator>::iter()
    }

    /// Returns the name as a string.
    ///
    /// For most signals, this function returns a static string that is the
    /// signal name in uppercase without the `"SIG"` prefix. For real-time
    /// signals `Rtmin(n)` and `Rtmax(n)` where `n` is non-zero, this function
    /// returns a dynamically allocated string that is `RTMIN` or `RTMAX`
    /// followed by the relative number `n`. Examples of the returned strings
    /// are `"TERM"`, `"RTMIN"`, and `"RTMAX-5"`.
    ///
    /// The returned name can be converted back to the signal using the
    /// [`FromStr`] implementation for `Name`.
    #[must_use]
    pub fn as_string(&self) -> Cow<'static, str> {
        match *self {
            Self::Abrt => Cow::Borrowed("ABRT"),
            Self::Alrm => Cow::Borrowed("ALRM"),
            Self::Bus => Cow::Borrowed("BUS"),
            Self::Chld => Cow::Borrowed("CHLD"),
            Self::Cld => Cow::Borrowed("CLD"),
            Self::Cont => Cow::Borrowed("CONT"),
            Self::Emt => Cow::Borrowed("EMT"),
            Self::Fpe => Cow::Borrowed("FPE"),
            Self::Hup => Cow::Borrowed("HUP"),
            Self::Ill => Cow::Borrowed("ILL"),
            Self::Info => Cow::Borrowed("INFO"),
            Self::Int => Cow::Borrowed("INT"),
            Self::Io => Cow::Borrowed("IO"),
            Self::Iot => Cow::Borrowed("IOT"),
            Self::Kill => Cow::Borrowed("KILL"),
            Self::Lost => Cow::Borrowed("LOST"),
            Self::Pipe => Cow::Borrowed("PIPE"),
            Self::Poll => Cow::Borrowed("POLL"),
            Self::Prof => Cow::Borrowed("PROF"),
            Self::Pwr => Cow::Borrowed("PWR"),
            Self::Quit => Cow::Borrowed("QUIT"),
            Self::Segv => Cow::Borrowed("SEGV"),
            Self::Stkflt => Cow::Borrowed("STKFLT"),
            Self::Stop => Cow::Borrowed("STOP"),
            Self::Sys => Cow::Borrowed("SYS"),
            Self::Term => Cow::Borrowed("TERM"),
            Self::Thr => Cow::Borrowed("THR"),
            Self::Trap => Cow::Borrowed("TRAP"),
            Self::Tstp => Cow::Borrowed("TSTP"),
            Self::Ttin => Cow::Borrowed("TTIN"),
            Self::Ttou => Cow::Borrowed("TTOU"),
            Self::Urg => Cow::Borrowed("URG"),
            Self::Usr1 => Cow::Borrowed("USR1"),
            Self::Usr2 => Cow::Borrowed("USR2"),
            Self::Vtalrm => Cow::Borrowed("VTALRM"),
            Self::Winch => Cow::Borrowed("WINCH"),
            Self::Xcpu => Cow::Borrowed("XCPU"),
            Self::Xfsz => Cow::Borrowed("XFSZ"),
            Self::Rtmin(0) => Cow::Borrowed("RTMIN"),
            Self::Rtmax(0) => Cow::Borrowed("RTMAX"),
            Self::Rtmin(n) => Cow::Owned(format!("RTMIN{n:+}")),
            Self::Rtmax(n) => Cow::Owned(format!("RTMAX{n:+}")),
        }
    }
}

impl std::fmt::Display for Name {
    /// Writes the signal name to the formatter.
    ///
    /// See [`Name::as_string`] for the format of the produced string.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_string().fmt(f)
    }
}

#[test]
fn test_name_to_string() {
    assert_eq!(Name::Term.to_string(), "TERM");
    assert_eq!(Name::Int.to_string(), "INT");
    assert_eq!(Name::Rtmin(0).to_string(), "RTMIN");
    assert_eq!(Name::Rtmax(0).to_string(), "RTMAX");
    assert_eq!(Name::Rtmin(1).to_string(), "RTMIN+1");
    assert_eq!(Name::Rtmin(20).to_string(), "RTMIN+20");
    assert_eq!(Name::Rtmax(-1).to_string(), "RTMAX-1");
    assert_eq!(Name::Rtmax(-20).to_string(), "RTMAX-20");
}

/// Error value for an unknown signal name
///
/// This error is used by the [`FromStr`] implementation for [`Name`] to
/// indicate that the input string does not match any known signal name.
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
pub struct UnknownNameError;

impl std::fmt::Display for UnknownNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unknown signal name")
    }
}

/// Parses a signal name from a string.
///
/// This function parses a signal name from a string. The input string is
/// expected to be an uppercase signal name without the `"SIG"` prefix. The
/// function returns the corresponding signal name if the input string matches a
/// known signal name. Otherwise, it returns an error.
///
/// See [`Name::as_string`] for the format of the input string.
impl FromStr for Name {
    type Err = UnknownNameError;

    fn from_str(s: &str) -> Result<Self, UnknownNameError> {
        match s {
            "ABRT" => Ok(Self::Abrt),
            "ALRM" => Ok(Self::Alrm),
            "BUS" => Ok(Self::Bus),
            "CHLD" => Ok(Self::Chld),
            "CLD" => Ok(Self::Cld),
            "CONT" => Ok(Self::Cont),
            "EMT" => Ok(Self::Emt),
            "FPE" => Ok(Self::Fpe),
            "HUP" => Ok(Self::Hup),
            "ILL" => Ok(Self::Ill),
            "INFO" => Ok(Self::Info),
            "INT" => Ok(Self::Int),
            "IO" => Ok(Self::Io),
            "IOT" => Ok(Self::Iot),
            "KILL" => Ok(Self::Kill),
            "LOST" => Ok(Self::Lost),
            "PIPE" => Ok(Self::Pipe),
            "POLL" => Ok(Self::Poll),
            "PROF" => Ok(Self::Prof),
            "PWR" => Ok(Self::Pwr),
            "QUIT" => Ok(Self::Quit),
            "SEGV" => Ok(Self::Segv),
            "STKFLT" => Ok(Self::Stkflt),
            "STOP" => Ok(Self::Stop),
            "SYS" => Ok(Self::Sys),
            "TERM" => Ok(Self::Term),
            "THR" => Ok(Self::Thr),
            "TRAP" => Ok(Self::Trap),
            "TSTP" => Ok(Self::Tstp),
            "TTIN" => Ok(Self::Ttin),
            "TTOU" => Ok(Self::Ttou),
            "URG" => Ok(Self::Urg),
            "USR1" => Ok(Self::Usr1),
            "USR2" => Ok(Self::Usr2),
            "VTALRM" => Ok(Self::Vtalrm),
            "WINCH" => Ok(Self::Winch),
            "XCPU" => Ok(Self::Xcpu),
            "XFSZ" => Ok(Self::Xfsz),

            "RTMIN" => Ok(Self::Rtmin(0)),
            "RTMAX" => Ok(Self::Rtmax(0)),
            _ => {
                if let Some(tail) = s.strip_prefix("RTMIN") {
                    if tail.starts_with(['+', '-']) {
                        if let Ok(n) = tail.parse() {
                            return Ok(Self::Rtmin(n));
                        }
                    }
                }
                if let Some(tail) = s.strip_prefix("RTMAX") {
                    if tail.starts_with(['+', '-']) {
                        if let Ok(n) = tail.parse() {
                            return Ok(Self::Rtmax(n));
                        }
                    }
                }
                Err(UnknownNameError)
            }
        }
    }
}

#[test]
fn test_name_from_str() {
    assert_eq!("ABRT".parse(), Ok(Name::Abrt));
    assert_eq!("INT".parse(), Ok(Name::Int));
    assert_eq!("QUIT".parse(), Ok(Name::Quit));

    assert_eq!("RTMIN".parse(), Ok(Name::Rtmin(0)));
    assert_eq!("RTMIN+0".parse(), Ok(Name::Rtmin(0)));
    assert_eq!("RTMIN+1".parse(), Ok(Name::Rtmin(1)));

    assert_eq!("RTMAX".parse(), Ok(Name::Rtmax(0)));
    assert_eq!("RTMAX-0".parse(), Ok(Name::Rtmax(0)));
    assert_eq!("RTMAX-1".parse(), Ok(Name::Rtmax(-1)));

    assert_eq!("".parse::<Name>(), Err(UnknownNameError));
    assert_eq!("FOO".parse::<Name>(), Err(UnknownNameError));
    assert_eq!("int".parse::<Name>(), Err(UnknownNameError));
    assert_eq!("RTMIN0".parse::<Name>(), Err(UnknownNameError));
    assert_eq!("RTMIN+".parse::<Name>(), Err(UnknownNameError));
    assert_eq!("RTMAX0".parse::<Name>(), Err(UnknownNameError));
    assert_eq!("RTMAX-".parse::<Name>(), Err(UnknownNameError));
    assert_eq!("2".parse::<Name>(), Err(UnknownNameError));
}

/// Signal number
///
/// This is a wrapper type for signal numbers. It is guaranteed to be a positive
/// integer, so it optimizes the size of `Option<Number>`, etc.
///
/// To make sure that all `Number`s are valid, you can only obtain a `Number`
/// from an instance of [`System`]. Use the [`System::validate_signal`] and
/// [`System::signal_number_from_name`] methods to create a `Number` from a raw
/// signal number or a signal name, respectively.
///
/// Signal numbers are specific to the underlying system. Passing a signal
/// number obtained from [`RealSystem`] to [`VirtualSystem`] (or vice versa) is
/// not supported and may result in unexpected behavior, though it is not
/// checked by the type system.
///
/// [`RealSystem`]: crate::system::real::RealSystem
/// [`VirtualSystem`]: crate::system::virtual::VirtualSystem
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialOrd, PartialEq)]
#[repr(transparent)]
pub struct Number(NonZeroI32);

impl Number {
    /// Returns the raw signal number.
    #[inline(always)]
    #[must_use]
    pub fn as_raw(self) -> RawNumber {
        self.0.get()
    }

    /// Returns the raw signal number as a `NonZeroI32`.
    #[inline(always)]
    #[must_use]
    pub fn as_raw_non_zero(self) -> NonZeroI32 {
        self.0
    }

    /// Creates a new `Number` from a raw signal number.
    ///
    /// This is a backdoor method that allows creating a `Number` from an
    /// arbitrary raw signal number. The caller must ensure that the raw signal
    /// number is a valid signal number.
    ///
    /// This function is not marked `unsafe` because creating an invalid
    /// `Number` does not lead to undefined behavior. However, it is not
    /// recommended to use this function unless you are sure that the raw signal
    /// number is valid. To make sure that all `Number`s are valid, use the
    /// [`System::validate_signal`] and [`System::signal_number_from_name`]
    /// methods instead.
    #[inline(always)]
    #[must_use]
    pub fn from_raw_unchecked(raw: NonZeroI32) -> Self {
        Self(raw)
    }
}

impl std::fmt::Display for Number {
    #[inline(always)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::fmt::Binary for Number {
    #[inline(always)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::fmt::Octal for Number {
    #[inline(always)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::fmt::LowerHex for Number {
    #[inline(always)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::fmt::UpperHex for Number {
    #[inline(always)]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
