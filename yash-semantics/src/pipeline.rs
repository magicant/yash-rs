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
use yash_env::io::Fd;
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
    // Start commands
    // TODO avoid cloning commands for efficiency
    let mut commands = commands.to_owned().into_iter().peekable();
    let mut pipes = PipeSet::new();
    let mut pids = Vec::new();
    while let Some(command) = commands.next() {
        let has_next = commands.peek().is_some();
        // TODO print some message if shift fails
        let _ = pipes.shift(env, has_next);
        let pipes2 = pipes;

        let subshell = env.start_subshell(move |env| {
            Box::pin(async move {
                // TODO print some message if move_to_stdin_stdout fails
                let _ = pipes2.move_to_stdin_stdout(env);

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

    // Await the last command
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

/// Set of pipe file descriptors that connect commands.
#[derive(Clone, Copy, Default)]
struct PipeSet {
    read_previous: Option<Fd>,
    /// Reader and writer to the next command.
    next: Option<(Fd, Fd)>,
}

impl PipeSet {
    fn new() -> Self {
        Self::default()
    }

    /// Updates the pipe set for the next command.
    ///
    /// Closes FDs that are no longer necessary and opens a new pipe if there is
    /// a next command.
    fn shift(&mut self, env: &mut Env, has_next: bool) -> nix::Result<()> {
        if let Some(fd) = self.read_previous {
            let _ = env.system.close(fd);
        }

        if let Some((reader, writer)) = self.next {
            let _ = env.system.close(writer);
            self.read_previous = Some(reader);
        } else {
            self.read_previous = None;
        }

        self.next = None;
        if has_next {
            self.next = Some(env.system.pipe()?);
        }

        Ok(())
    }

    /// Moves the pipe FDs to stdin/stdout and closes the FDs that are no longer
    /// necessary.
    fn move_to_stdin_stdout(self, env: &mut Env) -> nix::Result<()> {
        if let Some(reader) = self.read_previous {
            // TODO What if self.next.writer == stdin?
            if reader != Fd::STDIN {
                // TODO What if dup2 fails?
                let _ = env.system.dup2(reader, Fd::STDIN);
                let _ = env.system.close(reader);
            }
        }
        if let Some((reader, writer)) = self.next {
            if writer != Fd::STDOUT {
                // TODO What if dup2 fails?
                let _ = env.system.dup2(writer, Fd::STDOUT);
                let _ = env.system.close(writer);
            }
            let _ = env.system.close(reader);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::cat_builtin;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use futures::executor::block_on;
    use futures::executor::LocalPool;
    use std::path::Path;
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
    #[ignore] // TODO don't ignore this test case
    fn pipe_connects_commands_in_pipeline() {
        let system = VirtualSystem::new();
        let mut executor = LocalPool::new();
        let state = Rc::clone(&system.state);
        state.borrow_mut().executor = Some(Rc::new(executor.spawner()));

        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("cat", cat_builtin());
        let pipeline: syntax::Pipeline = "echo ok | cat | cat".parse().unwrap();
        let result = executor.run_until(pipeline.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);

        let state = state.borrow();
        let stdout = state
            .file_system
            .get(Path::new("/dev/stdout"))
            .unwrap()
            .borrow();
        assert_eq!(stdout.content, "ok\n".as_bytes());
        // TODO should also test stdin
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
