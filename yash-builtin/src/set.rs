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
//! This module implements the [`set` built-in], which modifies shell options and
//! positional parameters.
//!
//! [`set` built-in]: https://magicant.github.io/yash-rs/builtins/set.html
//!
//! # Implementation notes
//!
//! See [`parse_short`] for a list of available short options and [`parse_long`]
//! to learn how long options are parsed.
//! Long options are [canonicalize]d before being passed to `parse_long`.

use crate::common::output;
use crate::common::report_error;
use std::fmt::Write;
use yash_env::Env;
use yash_env::builtin::Result;
use yash_env::option::State;
#[cfg(doc)]
use yash_env::option::canonicalize;
#[cfg(doc)]
use yash_env::option::parse_long;
#[cfg(doc)]
use yash_env::option::parse_short;
use yash_env::option::{Interactive, Monitor};
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::stack::Frame::Subshell;
use yash_env::variable::Scope::Global;
use yash_syntax::parser::lex::is_name;

/// Interpretation of command-line arguments that determine the behavior of the
/// set built-in
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Command {
    /// No arguments: print all variables
    PrintVariables,

    /// Single argument `-o`: print options (human-readable)
    PrintOptionsHumanReadable,

    /// Single argument `+o`: print options (machine-readable)
    PrintOptionsMachineReadable,

    /// Other: modify options and/or positional parameters
    Modify {
        /// Options to be modified
        options: Vec<(yash_env::option::Option, State)>,
        /// New positional parameters (unless `None`)
        positional_params: std::option::Option<Vec<Field>>,
    },
}

pub mod syntax;
// TODO pub mod semantics;

/// Enables or disables the internal dispositions for the "stopper" signals
/// depending on the `Interactive` and `Monitor` option states.
fn update_internal_dispositions_for_stoppers(env: &mut Env) {
    if env.options.get(Interactive) == State::On && env.options.get(Monitor) == State::On {
        env.traps
            .enable_internal_dispositions_for_stoppers(&mut env.system)
    } else {
        env.traps
            .disable_internal_dispositions_for_stoppers(&mut env.system)
    }
    .ok();
}

/// Ensures that the shell is in the foreground process group if the `Monitor`
/// option is enabled.
fn ensure_foreground(env: &mut Env) {
    if env.options.get(Monitor) == State::On {
        env.ensure_foreground().ok();
    }
}

/// Modifies shell options and positional parameters.
fn modify(
    env: &mut Env,
    options: Vec<(yash_env::option::Option, State)>,
    positional_params: Option<Vec<Field>>,
) {
    // Modify options
    let mut monitor_changed = false;
    for (option, state) in options {
        env.options.set(option, state);
        monitor_changed |= option == Monitor;
    }

    // Reinitialize job control
    if monitor_changed && !env.stack.contains(&Subshell) {
        // We ignore errors in theses functions because they are not essential
        // for updating the options.
        update_internal_dispositions_for_stoppers(env);
        ensure_foreground(env);
    }

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
        Ok(Command::PrintVariables) => {
            let mut vars: Vec<_> = env
                .variables
                .iter(Global)
                .filter(|(name, _)| is_name(name))
                .collect();
            // TODO apply current locale's collation
            vars.sort_unstable_by_key(|&(name, _)| name);

            let mut print = String::new();
            for (name, var) in vars {
                if let Some(value) = &var.value {
                    writeln!(print, "{}={}", name, value.quote()).unwrap();
                }
            }
            output(env, &print).await
        }

        Ok(Command::PrintOptionsHumanReadable) => {
            let mut print = String::new();
            for option in yash_env::option::Option::iter() {
                let state = env.options.get(option);
                writeln!(print, "{option:16} {state}").unwrap();
            }
            output(env, &print).await
        }

        Ok(Command::PrintOptionsMachineReadable) => {
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

        Ok(Command::Modify {
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
    use futures_util::FutureExt;
    use std::ops::ControlFlow::Continue;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::builtin::Builtin;
    use yash_env::builtin::Type::Special;
    use yash_env::option::Option::*;
    use yash_env::option::OptionSet;
    use yash_env::option::State::*;
    use yash_env::system::Disposition;
    use yash_env::system::r#virtual::SIGTSTP;
    use yash_env::variable::Scope;
    use yash_env::variable::Value;
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::assert_stdout;
    use yash_semantics::command::Command as _;
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
        let mut var = env.variables.get_or_new("bad=name", Scope::Global);
        var.assign("Oops!", None).unwrap();

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
pipefail         off
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
            Builtin::new(Special, |env, args| Box::pin(main(env, args))),
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
        let disposition = state.processes[&env.main_pid].disposition(SIGTSTP);
        assert_eq!(disposition, Disposition::Ignore);
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
        let disposition = state.processes[&env.main_pid].disposition(SIGTSTP);
        assert_eq!(disposition, Disposition::Default);
    }

    #[test]
    fn internal_dispositions_not_enabled_for_stoppers_in_non_interactive_shell() {
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
        let disposition = state.processes[&env.main_pid].disposition(SIGTSTP);
        assert_eq!(disposition, Disposition::Default);
    }

    #[test]
    fn internal_dispositions_not_enabled_for_stoppers_in_subshell() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let mut env = env.push_frame(Subshell);
        env.options.set(Interactive, On);
        let args = Field::dummies(["-m"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));
        let mut expected_options = OptionSet::default();
        expected_options.extend([Interactive, Monitor]);
        assert_eq!(env.options, expected_options);
        let state = state.borrow();
        let disposition = state.processes[&env.main_pid].disposition(SIGTSTP);
        assert_eq!(disposition, Disposition::Default);
    }

    // TODO Test the case when the -m option is enabled while the shell is not
    // in the foreground. This requires the correct implementation of the
    // `VirtualSystem::tcsetpgrp` method.
}
