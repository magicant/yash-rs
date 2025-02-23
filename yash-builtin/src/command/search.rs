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

//! Command search
//!
//! This module provides the search functionality of the `command` built-in.
//! It is based on the [`yash_semantics::command_search`] module, but it adds
//! the ability to select the category of the command to search for.

use super::Category;
use super::Search;
use std::ffi::CStr;
use std::rc::Rc;
use yash_env::Env;
use yash_env::builtin::Builtin;
use yash_env::function::Function;
use yash_env::system::System;
use yash_env::variable::Expansion;

/// Environment adapter for applying the search parameters
///
/// This type implements the [`yash_semantics::command_search::SearchEnv`] trait
/// by extracting results from the environment filtered by the search
/// parameters.
#[derive(Debug)]
pub struct SearchEnv<'a> {
    pub env: &'a mut Env,
    pub params: &'a Search,
}

impl yash_semantics::command_search::PathEnv for SearchEnv<'_> {
    /// Returns the path.
    ///
    /// If [`Search::standard_path`] is `true`, this function retrieves the
    /// standard path from the environment using [`System::confstr_path`].
    /// Otherwise, the value of the `$PATH` variable is returned.
    fn path(&self) -> Expansion<'_> {
        if self.params.standard_path {
            match self.env.system.confstr_path() {
                Ok(path) => match path.into_string() {
                    Ok(path) => path.into(),
                    Err(_) => Expansion::Unset,
                },
                Err(_) => Expansion::Unset,
            }
        } else {
            self.env.path()
        }
    }

    #[inline]
    fn is_executable_file(&self, path: &CStr) -> bool {
        self.env.is_executable_file(path)
    }
}

impl yash_semantics::command_search::SearchEnv for SearchEnv<'_> {
    fn builtin(&self, name: &str) -> Option<Builtin> {
        if self.params.categories.contains(Category::Builtin) {
            self.env.builtin(name)
        } else {
            None
        }
    }

    fn function(&self, name: &str) -> Option<&Rc<Function>> {
        if self.params.categories.contains(Category::Function) {
            self.env.function(name)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enumset::EnumSet;
    use yash_env::builtin::Type::Special;
    use yash_env::str::UnixString;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_env::variable::PATH;
    use yash_env::variable::Scope;
    use yash_semantics::command_search::PathEnv as _;
    use yash_semantics::command_search::SearchEnv as _;
    use yash_syntax::source::Location;
    use yash_syntax::syntax::FullCompoundCommand;

    #[test]
    fn standard_path() {
        let system = Box::new(VirtualSystem::new());
        system.state.borrow_mut().path = "/bin:/usr/bin:/std".into();
        let env = &mut Env::with_system(system);
        env.variables
            .get_or_new(PATH, Scope::Global)
            .assign("/usr/local/bin:/bin", None)
            .unwrap();
        let params = &Search {
            standard_path: true,
            ..Search::default_for_invoke()
        };
        let search_env = SearchEnv { env, params };

        let result = search_env.path();
        assert_eq!(result, Expansion::from("/bin:/usr/bin:/std"));
    }

    #[test]
    fn standard_path_with_confstr_error() {
        let system = Box::new(VirtualSystem::new());
        system.state.borrow_mut().path = "".into();
        let env = &mut Env::with_system(system);
        let params = &Search {
            standard_path: true,
            ..Search::default_for_invoke()
        };
        let search_env = SearchEnv { env, params };

        let result = search_env.path();
        assert_eq!(result, Expansion::Unset);
    }

    #[test]
    fn standard_path_with_invalid_utf8() {
        let system = Box::new(VirtualSystem::new());
        system.state.borrow_mut().path = UnixString::from_vec(vec![0x80]);
        let env = &mut Env::with_system(system);
        let params = &Search {
            standard_path: true,
            ..Search::default_for_invoke()
        };
        let search_env = SearchEnv { env, params };

        let result = search_env.path();
        assert_eq!(result, Expansion::Unset);
    }

    #[test]
    fn non_standard_path_scalar() {
        let system = Box::new(VirtualSystem::new());
        system.state.borrow_mut().path = "/bin:/usr/bin:/std".into();
        let env = &mut Env::with_system(system);
        env.variables
            .get_or_new(PATH, Scope::Global)
            .assign("/usr/local/bin:/bin", None)
            .unwrap();
        let params = &Search {
            standard_path: false,
            ..Search::default_for_invoke()
        };
        let search_env = SearchEnv { env, params };

        let result = search_env.path();
        assert_eq!(result, Expansion::from("/usr/local/bin:/bin"));
    }

    #[test]
    fn non_standard_path_array() {
        let array = vec!["/usr/local/bin".to_owned(), "/bin".to_owned()];

        let system = Box::new(VirtualSystem::new());
        system.state.borrow_mut().path = "/bin:/usr/bin:/std".into();
        let env = &mut Env::with_system(system);
        env.variables
            .get_or_new(PATH, Scope::Global)
            .assign(array.clone(), None)
            .unwrap();
        let params = &Search {
            standard_path: false,
            ..Search::default_for_invoke()
        };
        let search_env = SearchEnv { env, params };

        let result = search_env.path();
        assert_eq!(result, Expansion::from(array));
    }

    #[test]
    fn builtin_on() {
        let env = &mut Env::new_virtual();
        let builtin = Builtin::new(Special, |_, _| unreachable!());
        env.builtins.insert(":", builtin);
        let params = &Search {
            categories: Category::Builtin.into(),
            ..Search::default_for_invoke()
        };
        let search_env = SearchEnv { env, params };

        let result = search_env.builtin(":");
        assert_eq!(result, Some(builtin));
    }

    #[test]
    fn builtin_off() {
        let env = &mut Env::new_virtual();
        let builtin = Builtin::new(Special, |_, _| unreachable!());
        env.builtins.insert(":", builtin);
        let params = &Search {
            categories: EnumSet::empty(),
            ..Search::default_for_invoke()
        };
        let search_env = SearchEnv { env, params };

        let result = search_env.builtin(":");
        assert_eq!(result, None);
    }

    #[test]
    fn function_on() {
        let env = &mut Env::new_virtual();
        let command: FullCompoundCommand = "{ :; }".parse().unwrap();
        let location = Location::dummy("f");
        let function = Rc::new(Function::new("f", command, location));
        env.functions.define(Rc::clone(&function)).unwrap();
        let params = &Search {
            categories: Category::Function.into(),
            ..Search::default_for_invoke()
        };
        let search_env = SearchEnv { env, params };

        let result = search_env.function("f");
        assert_eq!(result, Some(&function));
    }

    #[test]
    fn function_off() {
        let env = &mut Env::new_virtual();
        let command: FullCompoundCommand = "{ :; }".parse().unwrap();
        let location = Location::dummy("f");
        env.functions
            .define(Function::new("f", command, location))
            .unwrap();
        let params = &Search {
            categories: EnumSet::empty(),
            ..Search::default_for_invoke()
        };
        let search_env = SearchEnv { env, params };

        let result = search_env.function("f");
        assert_eq!(result, None);
    }
}
