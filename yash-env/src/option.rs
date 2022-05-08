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

//! Type definitions for shell options
//!
//! This module defines the [`OptionSet`] struct, a map from [`Option`] to
//! [`State`]. The option set represents whether each option is on or off.
//!
//! Note that `OptionSet` merely manages the state of options. It is not the
//! responsibility of `OptionSet` to change the behavior of the shell according
//! to the options.

use enumset::EnumSet;
use enumset::EnumSetIter;
use enumset::EnumSetType;
use std::borrow::Cow;
use std::fmt::Display;
use std::fmt::Formatter;
use std::ops::Not;
use std::str::FromStr;

/// State of an option: either enabled or disabled.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum State {
    /// Enabled.
    On,
    /// Disabled.
    Off,
}

pub use State::*;

/// Converts a state to a string (`on` or `off`).
impl Display for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            On => "on",
            Off => "off",
        };
        s.fmt(f)
    }
}

impl Not for State {
    type Output = Self;
    #[must_use]
    fn not(self) -> Self {
        match self {
            On => Off,
            Off => On,
        }
    }
}

/// Shell option
#[derive(Clone, Copy, Debug, EnumSetType, Eq, Hash, PartialEq)]
#[enumset(no_super_impls)]
#[non_exhaustive]
pub enum Option {
    /// Makes all variables exported when they are assigned.
    AllExport,
    /// Allows overwriting and truncating an existing file with the `>`
    /// redirection.
    Clobber,
    /// Executes a command string specified as a command line argument.
    CmdLine,
    /// Makes the shell to exit when a command returns a non-zero exit status.
    ErrExit,
    /// Makes the shell to actually run commands.
    Exec,
    /// Enables pathname expansion.
    Glob,
    /// Performs command search for each command in a function on its
    /// definition.
    HashOnDefinition,
    /// Prevents the interactive shell from exiting when the user enters an
    /// end-of-file.
    IgnoreEof,
    /// Enables features for interactive use.
    Interactive,
    /// Allows function definition commands to be recorded in the command
    /// history.
    Log,
    /// Sources the profile file on startup.
    Login,
    /// Enables job control.
    Monitor,
    /// Automatically reports the results of asynchronous jobs.
    Notify,
    /// Disables most non-POSIX extensions.
    PosixlyCorrect,
    /// Reads commands from the standard input.
    Stdin,
    /// Expands unset variables to an empty string rather than erroring out.
    Unset,
    /// Echos the input before parsing and executing.
    Verbose,
    /// Enables vi-like command line editing.
    Vi,
    /// Prints expanded words during command execution.
    XTrace,
}

pub use self::Option::*;

impl Option {
    /// Whether this option can be modified by the set built-in.
    ///
    /// Unmodifiable options can be set only on shell startup.
    #[must_use]
    pub fn is_modifiable(self) -> bool {
        !matches!(self, CmdLine | Interactive | Stdin)
    }

    /// Returns the option name, all in lower case without punctuations.
    ///
    /// This function returns a string like `"allexport"` and `"exec"`.
    pub fn long_name(self) -> &'static str {
        match self {
            AllExport => "allexport",
            Clobber => "clobber",
            CmdLine => "cmdline",
            ErrExit => "errexit",
            Exec => "exec",
            Glob => "glob",
            HashOnDefinition => "hashondefinition",
            IgnoreEof => "ignoreeof",
            Interactive => "interactive",
            Log => "log",
            Login => "login",
            Monitor => "monitor",
            Notify => "notify",
            PosixlyCorrect => "posixlycorrect",
            Stdin => "stdin",
            Unset => "unset",
            Verbose => "verbose",
            Vi => "vi",
            XTrace => "xtrace",
        }
    }
}

/// Prints the option name, all in lower case without punctuations.
impl Display for Option {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.long_name().fmt(f)
    }
}

/// Error type indicating that the input string does not name a valid option.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FromStrError {
    /// The input string does not match any option name.
    NoSuchOption,
    /// The input string is a prefix of more than one valid option name.
    Ambiguous,
}

pub use FromStrError::*;

/// Parses an option name.
///
/// The input string should be a canonical option name, that is, all the
/// characters should be lowercase and there should be no punctuations or other
/// irrelevant characters. You can [canonicalize] the name before parsing it.
///
/// The option name may be abbreviated as long as it is an unambiguous prefix of
/// a valid option name. For example, `Option::from_str("clob")` will return
/// `Ok(Clobber)` like `Option::from_str("clobber")`. If the name is ambiguous,
/// `from_str` returns `Err(Ambiguous)`. A full option name is never considered
/// ambiguous. For example, `"log"` is not ambiguous even though it is also a
/// prefix of another valid option `"login"`.
///
/// Note that new options may be added in the future, which can turn an
/// unambiguous option name into an ambiguous one. You should use full option
/// names for maximum compatibility.
impl FromStr for Option {
    type Err = FromStrError;
    fn from_str(name: &str) -> Result<Self, FromStrError> {
        const OPTIONS: &[(&str, Option)] = &[
            ("allexport", AllExport),
            ("clobber", Clobber),
            ("cmdline", CmdLine),
            ("errexit", ErrExit),
            ("exec", Exec),
            ("glob", Glob),
            ("hashondefinition", HashOnDefinition),
            ("ignoreeof", IgnoreEof),
            ("interactive", Interactive),
            ("log", Log),
            ("login", Login),
            ("monitor", Monitor),
            ("notify", Notify),
            ("posixlycorrect", PosixlyCorrect),
            ("stdin", Stdin),
            ("unset", Unset),
            ("verbose", Verbose),
            ("vi", Vi),
            ("xtrace", XTrace),
        ];

        match OPTIONS.binary_search_by_key(&name, |&(full_name, _option)| full_name) {
            Ok(index) => Ok(OPTIONS[index].1),
            Err(index) => {
                let mut options = OPTIONS[index..]
                    .iter()
                    .filter(|&(full_name, _option)| full_name.starts_with(name));
                match options.next() {
                    Some(first) => match options.next() {
                        Some(_second) => Err(Ambiguous),
                        None => Ok(first.1),
                    },
                    None => Err(NoSuchOption),
                }
            }
        }
    }
}

/// Parses a short option name.
///
/// This function parses the following single-character option names.
///
/// ```
/// # use yash_env::option::*;
/// assert_eq!(parse_short('a'), Some((AllExport, On)));
/// assert_eq!(parse_short('b'), Some((Notify, On)));
/// assert_eq!(parse_short('C'), Some((Clobber, Off)));
/// assert_eq!(parse_short('c'), Some((CmdLine, On)));
/// assert_eq!(parse_short('e'), Some((ErrExit, On)));
/// assert_eq!(parse_short('f'), Some((Glob, Off)));
/// assert_eq!(parse_short('h'), Some((HashOnDefinition, On)));
/// assert_eq!(parse_short('i'), Some((Interactive, On)));
/// assert_eq!(parse_short('l'), Some((Login, On)));
/// assert_eq!(parse_short('m'), Some((Monitor, On)));
/// assert_eq!(parse_short('n'), Some((Exec, Off)));
/// assert_eq!(parse_short('s'), Some((Stdin, On)));
/// assert_eq!(parse_short('u'), Some((Unset, Off)));
/// assert_eq!(parse_short('v'), Some((Verbose, On)));
/// assert_eq!(parse_short('x'), Some((XTrace, On)));
/// ```
///
/// The name argument is case-sensitive.
///
/// This function returns `None` if the argument does not match any of the short
/// option names above. Note that new names may be added in the future and it is
/// not considered a breaking API change.
#[must_use]
pub fn parse_short(name: char) -> std::option::Option<(self::Option, State)> {
    match name {
        'a' => Some((AllExport, On)),
        'b' => Some((Notify, On)),
        'C' => Some((Clobber, Off)),
        'c' => Some((CmdLine, On)),
        'e' => Some((ErrExit, On)),
        'f' => Some((Glob, Off)),
        'h' => Some((HashOnDefinition, On)),
        'i' => Some((Interactive, On)),
        'l' => Some((Login, On)),
        'm' => Some((Monitor, On)),
        'n' => Some((Exec, Off)),
        's' => Some((Stdin, On)),
        'u' => Some((Unset, Off)),
        'v' => Some((Verbose, On)),
        'x' => Some((XTrace, On)),
        _ => None,
    }
}

/// Iterator of options
///
/// This iterator yields all available options in alphabetical order.
///
/// An `Iter` can be created by [`Option::iter()`].
#[derive(Clone, Debug)]
pub struct Iter {
    inner: EnumSetIter<Option>,
}

impl Iterator for Iter {
    type Item = Option;
    fn next(&mut self) -> std::option::Option<self::Option> {
        self.inner.next()
    }
    fn size_hint(&self) -> (usize, std::option::Option<usize>) {
        self.inner.size_hint()
    }
}

impl DoubleEndedIterator for Iter {
    fn next_back(&mut self) -> std::option::Option<self::Option> {
        self.inner.next_back()
    }
}

impl ExactSizeIterator for Iter {}

impl Option {
    /// Creates an iterator that yields all available options in alphabetical
    /// order.
    pub fn iter() -> Iter {
        Iter {
            inner: EnumSet::<Option>::all().iter(),
        }
    }
}

/// Parses a long option name.
///
/// This function is similar to `impl FromStr for Option`, but allows prefixing
/// the option name with `no` to negate the state.
///
/// ```
/// # use yash_env::option::{parse_long, FromStrError::NoSuchOption, Option::*, State::*};
/// assert_eq!(parse_long("notify"), Ok((Notify, On)));
/// assert_eq!(parse_long("nonotify"), Ok((Notify, Off)));
/// assert_eq!(parse_long("tify"), Err(NoSuchOption));
/// ```
///
/// Note that new options may be added in the future, which can turn an
/// unambiguous option name into an ambiguous one. You should use full option
/// names for forward compatibility.
///
/// You cannot parse a short option name with this function. Use [`parse_short`]
/// for that purpose.
pub fn parse_long(name: &str) -> Result<(Option, State), FromStrError> {
    if "no".starts_with(name) {
        return Err(Ambiguous);
    }

    let intact = Option::from_str(name);
    let without_no = name
        .strip_prefix("no")
        .ok_or(NoSuchOption)
        .and_then(Option::from_str);

    match (intact, without_no) {
        (Ok(option), Err(NoSuchOption)) => Ok((option, On)),
        (Err(NoSuchOption), Ok(option)) => Ok((option, Off)),
        (Err(Ambiguous), _) | (_, Err(Ambiguous)) => Err(Ambiguous),
        _ => Err(NoSuchOption),
    }
}

/// Canonicalize an option name.
///
/// This function converts the string to lower case and removes non-alphanumeric
/// characters. Exceptionally, this function does not convert non-ASCII
/// uppercase characters because they will not constitute a valid option name
/// anyway.
pub fn canonicalize(name: &str) -> Cow<'_, str> {
    if name
        .chars()
        .all(|c| c.is_alphanumeric() && !c.is_ascii_uppercase())
    {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(
            name.chars()
                .filter(|c| c.is_alphanumeric())
                .map(|c| c.to_ascii_lowercase())
                .collect(),
        )
    }
}

/// Set of the shell options and their states.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OptionSet {
    enabled_options: EnumSet<Option>,
}

/// Defines the default option set.
///
/// Note that the default set is not empty. The following options are enabled by
/// default: `Clobber`, `Exec`, `Glob`, `Log`, `Unset`
impl Default for OptionSet {
    fn default() -> Self {
        let enabled_options = Clobber | Exec | Glob | Log | Unset;
        OptionSet { enabled_options }
    }
}

impl OptionSet {
    /// Creates an option set with all options disabled.
    pub fn empty() -> Self {
        OptionSet {
            enabled_options: EnumSet::empty(),
        }
    }

    /// Creates an option set with all options enabled.
    pub fn all() -> Self {
        OptionSet {
            enabled_options: EnumSet::all(),
        }
    }

    /// Returns the current state of the option.
    pub fn get(&self, option: Option) -> State {
        if self.enabled_options.contains(option) {
            On
        } else {
            Off
        }
    }

    /// Changes an option's state.
    ///
    /// Some options should not be changed after the shell startup, but that
    /// does not affect the behavior of this function.
    pub fn set(&mut self, option: Option, state: State) {
        match state {
            On => self.enabled_options.insert(option),
            Off => self.enabled_options.remove(option),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_and_from_str_round_trip() {
        for option in EnumSet::<Option>::all() {
            let name = option.to_string();
            assert_eq!(Option::from_str(&name), Ok(option));
        }
    }

    #[test]
    fn from_str_unambiguous_abbreviation() {
        assert_eq!(Option::from_str("allexpor"), Ok(AllExport));
        assert_eq!(Option::from_str("a"), Ok(AllExport));
        assert_eq!(Option::from_str("n"), Ok(Notify));
    }

    #[test]
    fn from_str_ambiguous_abbreviation() {
        assert_eq!(Option::from_str(""), Err(Ambiguous));
        assert_eq!(Option::from_str("c"), Err(Ambiguous));
        assert_eq!(Option::from_str("lo"), Err(Ambiguous));
    }

    #[test]
    fn from_str_no_match() {
        assert_eq!(Option::from_str("vim"), Err(NoSuchOption));
        assert_eq!(Option::from_str("0"), Err(NoSuchOption));
        assert_eq!(Option::from_str("LOG"), Err(NoSuchOption));
    }

    #[test]
    fn display_and_parse_round_trip() {
        for option in EnumSet::<Option>::all() {
            let name = option.to_string();
            assert_eq!(parse_long(&name), Ok((option, On)));
        }
    }

    #[test]
    fn display_and_parse_negated_round_trip() {
        for option in EnumSet::<Option>::all() {
            let name = format!("no{option}");
            assert_eq!(parse_long(&name), Ok((option, Off)));
        }
    }

    #[test]
    fn parse_unambiguous_abbreviation() {
        assert_eq!(parse_long("allexpor"), Ok((AllExport, On)));
        assert_eq!(parse_long("not"), Ok((Notify, On)));
        assert_eq!(parse_long("non"), Ok((Notify, Off)));
        assert_eq!(parse_long("un"), Ok((Unset, On)));
        assert_eq!(parse_long("noun"), Ok((Unset, Off)));
    }

    #[test]
    fn parse_ambiguous_abbreviation() {
        assert_eq!(parse_long(""), Err(Ambiguous));
        assert_eq!(parse_long("n"), Err(Ambiguous));
        assert_eq!(parse_long("no"), Err(Ambiguous));
        assert_eq!(parse_long("noe"), Err(Ambiguous));
        assert_eq!(parse_long("e"), Err(Ambiguous));
        assert_eq!(parse_long("nolo"), Err(Ambiguous));
    }

    #[test]
    fn parse_no_match() {
        assert_eq!(parse_long("vim"), Err(NoSuchOption));
        assert_eq!(parse_long("0"), Err(NoSuchOption));
        assert_eq!(parse_long("novim"), Err(NoSuchOption));
        assert_eq!(parse_long("no0"), Err(NoSuchOption));
        assert_eq!(parse_long("LOG"), Err(NoSuchOption));
    }

    #[test]
    fn test_canonicalize() {
        assert_eq!(canonicalize(""), "");
        assert_eq!(canonicalize("POSIXlyCorrect"), "posixlycorrect");
        assert_eq!(canonicalize(" log "), "log");
        assert_eq!(canonicalize("gLoB"), "glob");
        assert_eq!(canonicalize("no-notify"), "nonotify");
        assert_eq!(canonicalize(" no  such_Option "), "nosuchoption");
        assert_eq!(canonicalize("Ａｂｃ"), "Ａｂｃ");
    }
}
