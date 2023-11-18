// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Set built-in
//!
//! The **`set`** built-in modifies [shell options](yash_env::option) and
//! [positional parameters](yash_env::variable). It also can print a list of
//! current options or variables.
//!
//! # Description
//!
//! The built-in behaves differently depending on the invocation syntax.
//!
//! ## Printing variables
//!
//! ```sh
//! set
//! ```
//!
//! When executed without any arguments, the built-in prints a list of
//! [variables](yash_env::variable) visible in the current execution
//! environment. The list is formatted as a sequence of simple commands
//! performing an assignment that would restore the present variables if
//! executed (unless the assignment fails because of a read-only variable).
//! The list is ordered alphabetically.
//!
//! ## Printing options
//!
//! ```sh
//! set -o
//! ```
//!
//! If you specify the `-o` option as a unique argument to the set built-in, it
//! prints the current option settings in a human-readable format.
//!
//! ```sh
//! set +o
//! ```
//!
//! If you use the `+o` option instead, the printing lists shell commands that
//! would restore the current option settings if executed.
//!
//! ## Modifying shell options
//!
//! Other options modify [shell option](yash_env::option::Option) settings. They
//! can be specified in the short form like `-e` or the long form like `-o
//! errexit` and `--errexit`.
//!
//! You can also specify options starting with `+` in place of `-`, as in `+e`,
//! `+o errexit`, and `++errexit`. The `-` options turn on the corresponding
//! shell options while the `+` options turn off.
//!
//! See [`parse_short`] for a list of available short options and [`parse_long`]
//! to learn how long options are parsed.
//! Long options are [canonicalize]d before being passed to `parse_long`.
//!
//! You cannot modify the following options with the set built-in:
//!
//! - `CmdLine` (`-c`, `-o cmdline`)
//! - `Interactive` (`-i`, `-o interactive`)
//! - `Stdin` (`-s`, `-o stdin`)
//!
//! ## Modifying positional parameters
//!
//! If you specify one or more operands, they will be new positional parameters
//! in the current [context](yash_env::variable), replacing any existing
//! positional parameters.
//!
//! ## Option-operand separator
//!
//! As with other utilities conforming to POSIX XBD Utility Syntax Guidelines,
//! the set built-in accepts `--` as a separator between options and operands.
//! Additionally, you can separate them with `-` instead of `--`.
//!
//! If you place a separator without any operands, the built-in will clear all
//! positional parameters.
//!
//! # Exit status
//!
//! - 0: successful
//! - 1: error printing output
//! - 2: invalid options
//!
//! # Portability
//!
//! POSIX defines only the following option names:
//!
//! - `-a`, `-o allexport`
//! - `-b`, `-o notify`
//! - `-C`, `-o noclobber`
//! - `-e`, `-o errexit`
//! - `-f`, `-o noglob`
//! - `-h`
//! - `-m`, `-o monitor`
//! - `-n`, `-o noexec`
//! - `-u`, `-o nounset`
//! - `-v`, `-o verbose`
//! - `-x`, `-o xtrace`
//!
//! Other options (including non-canonicalized ones) are not portable. Also,
//! using the `no` prefix to negate an arbitrary option is not portable. For
//! example, `+o noexec` is portable, but `-o exec` is not.
//!
//! The output format of `set -o` and `set +o` depends on the shell.
//!
//! The semantics of `-` as an option-operand separator is unspecified in POSIX.
//! You should prefer `--`.
//!
//! Many (but not all) shells specially treat `+`, especially when it appears in
//! place of an option-operand separator. This behavior is not portable either.

use crate::common::output;
use crate::common::report_error;
use std::fmt::Write;
use yash_env::builtin::Result;
#[cfg(doc)]
use yash_env::option::canonicalize;
#[cfg(doc)]
use yash_env::option::parse_long;
#[cfg(doc)]
use yash_env::option::parse_short;
use yash_env::option::State;
use yash_env::option::{Interactive, Monitor};
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::variable::Scope::Global;
use yash_env::Env;

pub mod syntax;
// TODO pub mod semantics;

/// Enables or disables stopper handlers depending on the `Interactive` and
/// `Monitor` option states.
fn update_stopper_handlers(env: &mut Env) {
    if env.options.get(Interactive) == State::On && env.options.get(Monitor) == State::On {
        _ = env.traps.enable_stopper_handlers(&mut env.system)
    } else {
        _ = env.traps.disable_stopper_handlers(&mut env.system)
    }
}

/// Modifies shell options and positional parameters.
fn modify(
    env: &mut Env,
    options: Vec<(yash_env::option::Option, State)>,
    positional_params: Option<Vec<Field>>,
) {
    // Modify options
    for (option, state) in options {
        env.options.set(option, state);
    }
    update_stopper_handlers(env);

    // Modify positional parameters
    if let Some(fields) = positional_params {
        let params = env.variables.positional_params_mut();
        params.values = fields.into_iter().map(|f| f.value).collect();
        params.last_modified_location = env.stack.current_builtin().map(|b| b.name.origin.clone());
    }
}

/// Entry point for executing the `set` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    match syntax::parse(args) {
        Ok(syntax::Parse::PrintVariables) => {
            let mut vars: Vec<_> = env.variables.iter(Global).collect();
            // TODO apply current locale's collation
            vars.sort_unstable_by_key(|&(name, _)| name);

            let mut print = String::new();
            for (name, var) in vars {
                if let Some(value) = &var.value {
                    // TODO skip if the name contains a character inappropriate for a name
                    writeln!(print, "{}={}", name, value.quote()).unwrap();
                }
            }
            output(env, &print).await
        }

        Ok(syntax::Parse::PrintOptionsHumanReadable) => {
            let mut print = String::new();
            for option in yash_env::option::Option::iter() {
                let state = env.options.get(option);
                writeln!(print, "{option:16} {state}").unwrap();
            }
            output(env, &print).await
        }

        Ok(syntax::Parse::PrintOptionsMachineReadable) => {
            let mut print = String::new();
            for option in yash_env::option::Option::iter() {
                let skip = if option.is_modifiable() { "" } else { "#" };
                let flag = match env.options.get(option) {
                    State::On => '-',
                    State::Off => '+',
                };
                writeln!(print, "{skip}set {flag}o {option}").unwrap();
            }
            output(env, &print).await
        }

        Ok(syntax::Parse::Modify {
            options,
            positional_params,
        }) => {
            modify(env, options, positional_params);
            Result::new(ExitStatus::SUCCESS)
        }

        Err(error) => report_error(env, &error).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use crate::tests::assert_stdout;
    use futures_util::FutureExt;
    use std::ops::ControlFlow::Continue;
    use std::rc::Rc;
    use yash_env::builtin::Builtin;
    use yash_env::builtin::Type::Special;
    use yash_env::option::Option::*;
    use yash_env::option::OptionSet;
    use yash_env::option::State::*;
    use yash_env::system::SignalHandling;
    use yash_env::trap::Signal::SIGTSTP;
    use yash_env::variable::Scope;
    use yash_env::variable::Value;
    use yash_env::VirtualSystem;
    use yash_semantics::command::Command;
    use yash_syntax::syntax::List;

    #[test]
    fn printing_variables() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut var = env.variables.get_or_new("foo", Scope::Global);
        var.assign("value", None).unwrap();
        var.export(true);
        let mut var = env.variables.get_or_new("bar", Scope::Global);
        var.assign("Hello, world!", None).unwrap();
        let mut var = env.variables.get_or_new("baz", Scope::Global);
        var.assign(Value::array(["one", ""]), None).unwrap();

        let args = vec![];
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(stdout, "bar='Hello, world!'\nbaz=(one '')\nfoo=value\n")
        });
    }

    #[test]
    fn printing_options_human_readable() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.options.set(AllExport, On);
        env.options.set(Unset, Off);

        let args = Field::dummies(["-o"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        assert_stdout(&state, |stdout| {
            assert_eq!(
                stdout,
                "allexport        on
clobber          on
cmdline          off
errexit          off
exec             on
glob             on
hashondefinition off
ignoreeof        off
interactive      off
log              on
login            off
monitor          off
notify           off
posixlycorrect   off
stdin            off
unset            off
verbose          off
vi               off
xtrace           off
"
            )
        });
    }

    #[test]
    fn printing_options_machine_readable() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.options.set(Clobber, Off);
        env.options.set(Verbose, On);
        let options = env.options;

        let args = Field::dummies(["+o"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        // The output from `set +o` should be parsable
        let commands: List = assert_stdout(&state, |stdout| stdout.parse().unwrap());

        env.builtins.insert(
            "set",
            Builtin {
                r#type: Special,
                execute: |env, args| Box::pin(main(env, args)),
            },
        );
        env.options = Default::default();

        // Executing the parsed command should restore the previous options
        let result = commands.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);
        assert_eq!(env.options, options);

        // And there should be no errors doing that
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn setting_some_options() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["-a", "-n"]);
        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        let mut options = OptionSet::default();
        options.set(AllExport, On);
        options.set(Exec, Off);
        assert_eq!(env.options, options);
    }

    #[test]
    fn setting_some_positional_parameters() {
        let name = Field::dummy("set");
        let location = name.origin.clone();
        let is_special = true;
        let mut env = Env::new_virtual();
        let mut env = env.push_frame(yash_env::stack::Builtin { name, is_special }.into());
        let args = Field::dummies(["a", "b", "z"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        let params = env.variables.positional_params();
        assert_eq!(
            params.values,
            ["a".to_string(), "b".to_string(), "z".to_string()],
        );
        assert_eq!(params.last_modified_location, Some(location));
    }

    #[test]
    fn enabling_monitor_option() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.options.set(Interactive, On);
        let args = Field::dummies(["-m"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        let mut expected_options = OptionSet::default();
        expected_options.extend([Interactive, Monitor]);
        assert_eq!(env.options, expected_options);
        let state = state.borrow();
        let handling = state.processes[&env.main_pid].signal_handling(SIGTSTP);
        assert_eq!(handling, SignalHandling::Ignore);
    }

    #[test]
    fn disabling_monitor_option() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.options.set(Interactive, On);
        let args = Field::dummies(["-m"]);
        _ = main(&mut env, args).now_or_never().unwrap();
        let args = Field::dummies(["+m"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        let mut expected_options = OptionSet::default();
        expected_options.set(Interactive, On);
        assert_eq!(env.options, expected_options);
        let state = state.borrow();
        let handling = state.processes[&env.main_pid].signal_handling(SIGTSTP);
        assert_eq!(handling, SignalHandling::Default);
    }

    #[test]
    fn stopper_handlers_not_enabled_in_non_interactive_shell() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let args = Field::dummies(["-m"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        let mut expected_options = OptionSet::default();
        expected_options.set(Monitor, On);
        assert_eq!(env.options, expected_options);
        let state = state.borrow();
        let handling = state.processes[&env.main_pid].signal_handling(SIGTSTP);
        assert_eq!(handling, SignalHandling::Default);
    }
}
