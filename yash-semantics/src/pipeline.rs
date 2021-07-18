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
    /// # Executing simple commands
    ///
    /// If this pipeline contains one simple command, it is executed in the
    /// current shell execution environment.
    ///
    /// TODO Elaborate
    ///
    /// If the pipeline has no simple command, it is a no-op.
    ///
    /// # Exit status
    ///
    /// The exit status of the pipeline is that of the last simple command (or
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
        _ => {
            // TODO execute multi-command pipeline
            commands[0].execute(env).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::return_builtin;
    use futures::executor::block_on;
    use yash_env::exec::Divert;
    use yash_env::exec::ExitStatus;

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
