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

//! Defines signals.
//!
//! Signals are a method of inter-process communication used to notify a process
//! that a specific event has occurred. This module provides a list of signals
//! defined by POSIX with additional support for some non-standard signals.
//!
//! Signals are represented by the [`Signal`] enum, which defines a set of
//! signal names that may (or may not) be used on the system. It also allows
//! representing real-time signals relatively to the minimum or maximum
//! real-time signal number. The type also provides methods for converting
//! signals to and from names.
//!
//! This module is defined independently of the system, so it does not operate
//! on signal numbers that are actually sent to processes. Non-standard signals
//! and real-time signals represented by `Signal` may not be available on the
//! real system.
//!
//! Proper signal names are all uppercase and starts with `"SIG"`. However, the
//! names defined, parsed, and displayed in this module do not include the
//! `"SIG"` prefix.

use std::borrow::Cow;
use std::ffi::c_int;

/// Signal
///
/// See the [module-level documentation](self) for details.
///
/// This type implements `PartialOrd` and `Ord` to allow sorting signals
/// alphabetically. Real-time signals are ordered after other signals. Note that
/// the ordering is not based on the actual signal numbers.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Signal {
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
    /// `RTMIN(n)` represents the real-time signal `SIGRTMIN + n`, where `n` is
    /// expected to be a non-negative integer between `0` and
    /// `SIGRTMAX - SIGRTMIN`.
    Rtmin(c_int),

    /// Real-time signal with a number relative to the maximum real-time signal
    ///
    /// `RTMAX(n)` represents the real-time signal `SIGRTMAX + n`, where `n` is
    /// expected to be a non-positive integer between `SIGRTMIN - SIGRTMAX` and
    /// `0`.
    Rtmax(c_int),

    /// Signal specified by a raw signal number
    ///
    /// This variant is a "backdoor" to allow specifying any signal by its raw
    /// signal number.
    Number(c_int),
}

/// List of all named signals
///
/// This list does not include the `Rtmin`, `Rtmax`, and `Number` variants.
pub const SIGNALS: &[Signal] = &[
    Signal::Abrt,
    Signal::Alrm,
    Signal::Bus,
    Signal::Chld,
    Signal::Cld,
    Signal::Cont,
    Signal::Emt,
    Signal::Fpe,
    Signal::Hup,
    Signal::Ill,
    Signal::Info,
    Signal::Int,
    Signal::Io,
    Signal::Iot,
    Signal::Kill,
    Signal::Lost,
    Signal::Pipe,
    Signal::Poll,
    Signal::Prof,
    Signal::Pwr,
    Signal::Quit,
    Signal::Segv,
    Signal::Stkflt,
    Signal::Stop,
    Signal::Sys,
    Signal::Term,
    Signal::Thr,
    Signal::Trap,
    Signal::Tstp,
    Signal::Ttin,
    Signal::Ttou,
    Signal::Urg,
    Signal::Usr1,
    Signal::Usr2,
    Signal::Vtalrm,
    Signal::Winch,
    Signal::Xcpu,
    Signal::Xfsz,
];

impl Signal {
    /// Returns the name of the signal.
    ///
    /// For most signals, this function returns a static string that is the
    /// signal name in uppercase without the `"SIG"` prefix. For real-time
    /// signals `RTMIN(n)` and `RTMAX(n)` where `n` is non-zero, this function
    /// returns a dynamically allocated string that is `RTMIN` or `RTMAX`
    /// followed by the relative number `n`. For the `Number` variant, this
    /// function returns a dynamically allocated string that is the raw signal
    /// number. Examples of the returned strings are `"TERM"`, `"RTMIN"`,
    /// `"RTMAX-5"`, and `"42"`.
    ///
    /// The returned name can be converted back to the signal using the
    /// `FromStr` implementation for `Signal`.
    #[must_use]
    pub fn name(&self) -> Cow<'static, str> {
        match *self {
            Signal::Abrt => Cow::Borrowed("ABRT"),
            Signal::Alrm => Cow::Borrowed("ALRM"),
            Signal::Bus => Cow::Borrowed("BUS"),
            Signal::Chld => Cow::Borrowed("CHLD"),
            Signal::Cld => Cow::Borrowed("CLD"),
            Signal::Cont => Cow::Borrowed("CONT"),
            Signal::Emt => Cow::Borrowed("EMT"),
            Signal::Fpe => Cow::Borrowed("FPE"),
            Signal::Hup => Cow::Borrowed("HUP"),
            Signal::Ill => Cow::Borrowed("ILL"),
            Signal::Info => Cow::Borrowed("INFO"),
            Signal::Int => Cow::Borrowed("INT"),
            Signal::Io => Cow::Borrowed("IO"),
            Signal::Iot => Cow::Borrowed("IOT"),
            Signal::Kill => Cow::Borrowed("KILL"),
            Signal::Lost => Cow::Borrowed("LOST"),
            Signal::Pipe => Cow::Borrowed("PIPE"),
            Signal::Poll => Cow::Borrowed("POLL"),
            Signal::Prof => Cow::Borrowed("PROF"),
            Signal::Pwr => Cow::Borrowed("PWR"),
            Signal::Quit => Cow::Borrowed("QUIT"),
            Signal::Segv => Cow::Borrowed("SEGV"),
            Signal::Stkflt => Cow::Borrowed("STKFLT"),
            Signal::Stop => Cow::Borrowed("STOP"),
            Signal::Sys => Cow::Borrowed("SYS"),
            Signal::Term => Cow::Borrowed("TERM"),
            Signal::Thr => Cow::Borrowed("THR"),
            Signal::Trap => Cow::Borrowed("TRAP"),
            Signal::Tstp => Cow::Borrowed("TSTP"),
            Signal::Ttin => Cow::Borrowed("TTIN"),
            Signal::Ttou => Cow::Borrowed("TTOU"),
            Signal::Urg => Cow::Borrowed("URG"),
            Signal::Usr1 => Cow::Borrowed("USR1"),
            Signal::Usr2 => Cow::Borrowed("USR2"),
            Signal::Vtalrm => Cow::Borrowed("VTALRM"),
            Signal::Winch => Cow::Borrowed("WINCH"),
            Signal::Xcpu => Cow::Borrowed("XCPU"),
            Signal::Xfsz => Cow::Borrowed("XFSZ"),
            Signal::Rtmin(0) => Cow::Borrowed("RTMIN"),
            Signal::Rtmax(0) => Cow::Borrowed("RTMAX"),
            Signal::Rtmin(n) => Cow::Owned(format!("RTMIN{n:+}")),
            Signal::Rtmax(n) => Cow::Owned(format!("RTMAX{n:+}")),
            Signal::Number(n) => Cow::Owned(n.to_string()),
        }
    }
}

#[test]
fn test_name() {
    assert_eq!(Signal::Term.name(), "TERM");
    assert_eq!(Signal::Int.name(), "INT");
    assert_eq!(Signal::Rtmin(0).name(), "RTMIN");
    assert_eq!(Signal::Rtmax(0).name(), "RTMAX");
    assert_eq!(Signal::Rtmin(1).name(), "RTMIN+1");
    assert_eq!(Signal::Rtmin(20).name(), "RTMIN+20");
    assert_eq!(Signal::Rtmax(-1).name(), "RTMAX-1");
    assert_eq!(Signal::Rtmax(-20).name(), "RTMAX-20");
    assert_eq!(Signal::Number(42).name(), "42");
}

/// Prints the signal name.
///
/// See [`Signal::name`] for the format of the printed signal names.
impl std::fmt::Display for Signal {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name().fmt(f)
    }
}

/// Error returned when an invalid signal is specified.
///
/// This error is returned when a signal name is not recognized or a signal
/// is not supported on the system.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UnknownSignalError;

/// Parses a signal name.
///
/// This implementation supports parsing signal names in uppercase without the
/// `"SIG"` prefix. It also supports parsing real-time signals with relative
/// numbers, and raw signal numbers. See [`Signal::name`] for the format of the
/// signal names that can be parsed.
///
/// For raw signal numbers, this implementation accepts any non-negative
/// integer regardless of whether it is a valid signal number.
impl std::str::FromStr for Signal {
    type Err = UnknownSignalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ABRT" => Ok(Signal::Abrt),
            "ALRM" => Ok(Signal::Alrm),
            "BUS" => Ok(Signal::Bus),
            "CHLD" => Ok(Signal::Chld),
            "CLD" => Ok(Signal::Cld),
            "CONT" => Ok(Signal::Cont),
            "EMT" => Ok(Signal::Emt),
            "FPE" => Ok(Signal::Fpe),
            "HUP" => Ok(Signal::Hup),
            "ILL" => Ok(Signal::Ill),
            "INFO" => Ok(Signal::Info),
            "INT" => Ok(Signal::Int),
            "IO" => Ok(Signal::Io),
            "IOT" => Ok(Signal::Iot),
            "KILL" => Ok(Signal::Kill),
            "LOST" => Ok(Signal::Lost),
            "PIPE" => Ok(Signal::Pipe),
            "POLL" => Ok(Signal::Poll),
            "PROF" => Ok(Signal::Prof),
            "PWR" => Ok(Signal::Pwr),
            "QUIT" => Ok(Signal::Quit),
            "SEGV" => Ok(Signal::Segv),
            "STKFLT" => Ok(Signal::Stkflt),
            "STOP" => Ok(Signal::Stop),
            "SYS" => Ok(Signal::Sys),
            "TERM" => Ok(Signal::Term),
            "THR" => Ok(Signal::Thr),
            "TRAP" => Ok(Signal::Trap),
            "TSTP" => Ok(Signal::Tstp),
            "TTIN" => Ok(Signal::Ttin),
            "TTOU" => Ok(Signal::Ttou),
            "URG" => Ok(Signal::Urg),
            "USR1" => Ok(Signal::Usr1),
            "USR2" => Ok(Signal::Usr2),
            "VTALRM" => Ok(Signal::Vtalrm),
            "WINCH" => Ok(Signal::Winch),
            "XCPU" => Ok(Signal::Xcpu),
            "XFSZ" => Ok(Signal::Xfsz),

            "RTMIN" => Ok(Signal::Rtmin(0)),
            "RTMAX" => Ok(Signal::Rtmax(0)),
            _ => {
                if let Some(tail) = s.strip_prefix("RTMIN") {
                    if tail.starts_with(['+', '-']) {
                        if let Ok(n) = tail.parse() {
                            return Ok(Signal::Rtmin(n));
                        }
                    }
                }
                if let Some(tail) = s.strip_prefix("RTMAX") {
                    if tail.starts_with(['+', '-']) {
                        if let Ok(n) = tail.parse() {
                            return Ok(Signal::Rtmax(n));
                        }
                    }
                }
                if let Ok(n) = s.parse() {
                    // Any valid signal number is positive, but we also accept
                    // zero to help parsing the alias `0` for `EXIT`.
                    if n >= 0 {
                        return Ok(Signal::Number(n));
                    }
                }
                Err(UnknownSignalError)
            }
        }
    }
}

#[test]
fn test_from_str() {
    assert_eq!("ABRT".parse(), Ok(Signal::Abrt));
    assert_eq!("INT".parse(), Ok(Signal::Int));
    assert_eq!("QUIT".parse(), Ok(Signal::Quit));

    assert_eq!("RTMIN".parse(), Ok(Signal::Rtmin(0)));
    assert_eq!("RTMIN+0".parse(), Ok(Signal::Rtmin(0)));
    assert_eq!("RTMIN+1".parse(), Ok(Signal::Rtmin(1)));

    assert_eq!("RTMAX".parse(), Ok(Signal::Rtmax(0)));
    assert_eq!("RTMAX-0".parse(), Ok(Signal::Rtmax(0)));
    assert_eq!("RTMAX-1".parse(), Ok(Signal::Rtmax(-1)));

    assert_eq!("000".parse(), Ok(Signal::Number(0)));
    assert_eq!("1".parse(), Ok(Signal::Number(1)));
    assert_eq!("42".parse(), Ok(Signal::Number(42)));

    assert_eq!("".parse::<Signal>(), Err(UnknownSignalError));
    assert_eq!("FOO".parse::<Signal>(), Err(UnknownSignalError));
    assert_eq!("int".parse::<Signal>(), Err(UnknownSignalError));
    assert_eq!("RTMIN0".parse::<Signal>(), Err(UnknownSignalError));
    assert_eq!("RTMIN+".parse::<Signal>(), Err(UnknownSignalError));
    assert_eq!("RTMAX0".parse::<Signal>(), Err(UnknownSignalError));
    assert_eq!("RTMAX-".parse::<Signal>(), Err(UnknownSignalError));
    assert_eq!("-1".parse::<Signal>(), Err(UnknownSignalError));
}
