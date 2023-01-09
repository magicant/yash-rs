// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Exec built-in
//!
//! TODO Elaborate

use std::future::Future;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::semantics::Field;
use yash_env::Env;

/// Implements the exec built-in.
pub async fn builtin_body(_env: &mut Env, _args: Vec<Field>) -> Result {
    // TODO Implement exec built-in
    let mut result = Result::default();
    result.retain_redirs();
    result
}

/// Wrapper of [`builtin_body`] that returns the future in a pinned box.
pub fn builtin_main(env: &mut Env, args: Vec<Field>) -> Pin<Box<dyn Future<Output = Result> + '_>> {
    Box::pin(builtin_body(env, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use yash_semantics::ExitStatus;

    #[test]
    fn without_args() {
        let mut env = Env::new_virtual();
        let result = builtin_body(&mut env, vec![]).now_or_never().unwrap();
        assert_eq!(result.exit_status(), ExitStatus::SUCCESS);
        assert!(result.should_retain_redirs());
    }
}
