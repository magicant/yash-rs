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

//! Implementation of pipeline semantics.

use super::Command;
use async_trait::async_trait;
use yash_env::exec::ExitStatus;
use yash_env::exec::Result;
use yash_env::Env;
use yash_syntax::syntax;

#[async_trait(?Send)]
impl Command for syntax::Pipeline {
    /// Executes the pipeline.
    ///
    /// # Executing commands
    ///
    /// If this pipeline contains one command, it is executed in the
    /// current shell execution environment.
    ///
    /// If the pipeline has more than one command, all the commands are executed
    /// concurrently. Every command is executed in a new subshell. The standard
    /// output of a command is connected to the standard input of the next
    /// command via a pipe, except for the standard output of the last command
    /// and the standard input of the first command, which are not modified.
    ///
    /// If the pipeline has no command, it is a no-op.
    ///
    /// # Exit status
    ///
    /// The exit status of the pipeline is that of the last command (or
    /// zero if no command). If the pipeline starts with an `!`, the exit status
    /// is inverted: zero becomes one, and non-zero becomes zero.
    ///
    /// In POSIX, the expected exit status is unclear when an inverted pipeline
    /// performs a jump as in `! return 42`. The behavior disagrees among
    /// existing shells. This implementation does not invert the exit status
    /// when the return value is `Err(Divert::...)`, which is different from
    /// yash 2.
    async fn execute(&self, env: &mut Env) -> Result {
        if !self.negation {
            return execute_commands_in_pipeline(env, &self.commands).await;
        }

        execute_commands_in_pipeline(env, &self.commands).await?;
        env.exit_status = if env.exit_status == ExitStatus::SUCCESS {
            ExitStatus::FAILURE
        } else {
            ExitStatus::SUCCESS
        };
        Ok(())
    }
}

async fn execute_commands_in_pipeline(env: &mut Env, commands: &[syntax::Command]) -> Result {
    match commands.len() {
        0 => {
            env.exit_status = ExitStatus::SUCCESS;
            Ok(())
        }
        1 => commands[0].execute(env).await,
        _ => execute_multi_command_pipeline(env, commands).await,
    }
}

async fn execute_multi_command_pipeline(env: &mut Env, commands: &[syntax::Command]) -> Result {
    // TODO avoid cloning commands for efficiency
    let mut pids = Vec::new();
    for command in commands.to_owned() {
        // TODO connect pipe FDs
        let subshell = env.start_subshell(|env| {
            Box::pin(async move {
                match command.execute(env).await {
                    Ok(()) => (),
                    Err(_) => todo!("subshell finished in Divert"),
                }
            })
        });

        match subshell.await {
            Ok(pid) => pids.push(pid),
            Err(_) => todo!("fork failed, report to stderr"),
        }
    }

    loop {
        use nix::sys::wait::WaitStatus::*;
        #[allow(deprecated)]
        match env.system.wait_sync().await {
            Ok(Exited(pid, exit_status)) => {
                if pid == *pids.last().unwrap() {
                    env.exit_status = ExitStatus(exit_status);
                    break Ok(());
                }
                // TODO should not ignore other PIDs
            }
            Ok(Signaled(pid, _signal, _core_dumped)) => {
                if pid == *pids.last().unwrap() {
                    env.exit_status = ExitStatus(128);
                    // TODO Convert signal to exit status
                }
                // TODO should not ignore other PIDs
            }
            _ => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::return_builtin;
    use futures::executor::block_on;
    use futures::executor::LocalPool;
    use std::rc::Rc;
    use yash_env::exec::Divert;
    use yash_env::exec::ExitStatus;
    use yash_env::VirtualSystem;

    #[test]
    fn empty_pipeline() {
        let mut env = Env::new_virtual();
        let pipeline = syntax::Pipeline {
            commands: vec![],
            negation: false,
        };
        let result = block_on(pipeline.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(0));
    }

    #[test]
    fn single_command_pipeline_returns_exit_status_intact_without_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let pipeline: syntax::Pipeline = "return -n 93".parse().unwrap();
        let result = block_on(pipeline.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(93));
    }

    #[test]
    fn single_command_pipeline_returns_exit_status_intact_with_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let pipeline: syntax::Pipeline = "return 37".parse().unwrap();
        let result = block_on(pipeline.execute(&mut env));
        assert_eq!(result, Err(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(37));
    }

    #[test]
    fn multi_command_pipeline_returns_last_command_exit_status() {
        let system = VirtualSystem::new();
        let mut executor = LocalPool::new();
        let mut state = system.state.borrow_mut();
        state.executor = Some(Rc::new(executor.spawner()));
        drop(state);

        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("return", return_builtin());
        let pipeline: syntax::Pipeline = "return -n 10 | return -n 20".parse().unwrap();
        let result = executor.run_until(pipeline.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(20));
    }

    #[test]
    fn inverting_exit_status_to_0_without_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let pipeline: syntax::Pipeline = "! return -n 42".parse().unwrap();
        let result = block_on(pipeline.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(0));
    }

    #[test]
    fn inverting_exit_status_to_1_without_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let pipeline: syntax::Pipeline = "! return -n 0".parse().unwrap();
        let result = block_on(pipeline.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(1));
    }

    #[test]
    fn not_inverting_exit_status_with_divert() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let pipeline: syntax::Pipeline = "! return 15".parse().unwrap();
        let result = block_on(pipeline.execute(&mut env));
        assert_eq!(result, Err(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(15));
    }
}
