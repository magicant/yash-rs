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

//! Showing resource limits

use super::Error;
use super::ResourceExt as _;
use super::ShowLimitType;
use std::fmt::Write as _;
use yash_env::system::resource::rlim_t;
use yash_env::system::resource::LimitPair;
use yash_env::system::resource::Resource;
use yash_env::system::resource::RLIM_INFINITY;
use yash_env::system::Errno;

/// A wrapper for `rlim_t` that implements `Display`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Limit {
    value: rlim_t,
    scale: rlim_t,
}

impl std::fmt::Display for Limit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.value == RLIM_INFINITY {
            "unlimited".fmt(f)
        } else {
            (self.value / self.scale).fmt(f)
        }
    }
}

/// Shows the current limits for all resources.
///
/// Returns a string that contains the current limits for all resources.
/// Each line shows the option, description, and limit for a resource.
pub fn show_all<F>(mut getrlimit: F, limit_type: ShowLimitType) -> String
where
    F: FnMut(Resource) -> Result<LimitPair, Errno>,
{
    let mut result = String::with_capacity(1024);
    for &resource in Resource::ALL {
        let Ok(limits) = getrlimit(resource) else {
            continue;
        };
        let option = resource.option();
        let desc = resource.description();
        let value = match limit_type {
            ShowLimitType::Soft => limits.soft,
            ShowLimitType::Hard => limits.hard,
        };
        let scale = resource.scale();
        let limit = Limit { value, scale };
        writeln!(result, "-{option}: {desc:<32} {limit}").unwrap();
    }
    result
}

/// Shows the current limits for the specified resource.
///
/// Returns a string that contains the current limit for the specified resource,
/// followed by a newline.
pub fn show_one<F>(
    getrlimit: F,
    resource: Resource,
    limit_type: ShowLimitType,
) -> Result<String, Error>
where
    F: FnOnce(Resource) -> Result<LimitPair, Errno>,
{
    match getrlimit(resource) {
        Ok(limits) => {
            let value = match limit_type {
                ShowLimitType::Soft => limits.soft,
                ShowLimitType::Hard => limits.hard,
            };
            let scale = resource.scale();
            let limit = Limit { value, scale };
            Ok(format!("{}\n", limit))
        }

        Err(Errno::EINVAL) => Err(Error::UnsupportedResource),
        Err(errno) => Err(Error::Unknown(errno)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use yash_env::system::Errno;

    #[test]
    fn show_all_infinity() {
        let getrlimit = |_: Resource| {
            Ok(LimitPair {
                soft: RLIM_INFINITY,
                hard: RLIM_INFINITY,
            })
        };
        let result = show_all(getrlimit, ShowLimitType::Soft);
        assert_eq!(
            result,
            "-v: virtual address space size (KiB) unlimited\n\
             -c: core dump size (512-byte blocks) unlimited\n\
             -t: CPU time (seconds)               unlimited\n\
             -d: data segment size (KiB)          unlimited\n\
             -f: file size (512-byte blocks)      unlimited\n\
             -k: number of kqueues                unlimited\n\
             -x: number of file locks             unlimited\n\
             -l: locked memory size (KiB)         unlimited\n\
             -q: message queue size (bytes)       unlimited\n\
             -e: process priority (20 - nice)     unlimited\n\
             -n: number of open files             unlimited\n\
             -u: number of processes              unlimited\n\
             -m: resident set size (KiB)          unlimited\n\
             -r: real-time priority               unlimited\n\
             -R: real-time timeout (microseconds) unlimited\n\
             -b: socket buffer size (bytes)       unlimited\n\
             -i: number of pending signals        unlimited\n\
             -s: stack size (KiB)                 unlimited\n\
             -w: swap space size (KiB)            unlimited\n"
        );
    }

    #[test]
    fn show_all_soft_finite() {
        let getrlimit = |resource: Resource| {
            if resource == Resource::CPU {
                Ok(LimitPair { soft: 5, hard: 12 })
            } else {
                Err(Errno::EINVAL)
            }
        };
        let result = show_all(getrlimit, ShowLimitType::Soft);
        assert_eq!(result, "-t: CPU time (seconds)               5\n");
    }

    #[test]
    fn show_all_soft_finite_scaled() {
        let getrlimit = |resource: Resource| {
            if resource == Resource::DATA {
                Ok(LimitPair {
                    soft: 5 << 10,
                    hard: 12 << 10,
                })
            } else {
                Err(Errno::EINVAL)
            }
        };
        let result = show_all(getrlimit, ShowLimitType::Soft);
        assert_eq!(result, "-d: data segment size (KiB)          5\n");
    }

    #[test]
    fn show_all_hard_finite() {
        let getrlimit = |resource: Resource| {
            if resource == Resource::CPU {
                Ok(LimitPair { soft: 5, hard: 12 })
            } else {
                Err(Errno::EINVAL)
            }
        };
        let result = show_all(getrlimit, ShowLimitType::Hard);
        assert_eq!(result, "-t: CPU time (seconds)               12\n");
    }

    #[test]
    fn show_one_infinite() {
        let getrlimit = |_: Resource| {
            Ok(LimitPair {
                soft: RLIM_INFINITY,
                hard: RLIM_INFINITY,
            })
        };
        let result = show_one(getrlimit, Resource::DATA, ShowLimitType::Soft).unwrap();
        assert_eq!(result, "unlimited\n");
    }

    #[test]
    fn show_one_soft_finite_scaled() {
        let getrlimit = |resource: Resource| {
            assert_eq!(resource, Resource::DATA);
            Ok(LimitPair {
                soft: 5 << 10,
                hard: 12 << 10,
            })
        };
        let result = show_one(getrlimit, Resource::DATA, ShowLimitType::Soft).unwrap();
        assert_eq!(result, "5\n");
    }

    #[test]
    fn show_one_hard_finite() {
        let getrlimit = |resource: Resource| {
            assert_eq!(resource, Resource::CPU);
            Ok(LimitPair { soft: 5, hard: 12 })
        };
        let result = show_one(getrlimit, Resource::CPU, ShowLimitType::Hard).unwrap();
        assert_eq!(result, "12\n");
    }

    #[test]
    fn show_one_unsupported_resource() {
        let getrlimit = |_: Resource| Err(Errno::EINVAL);
        let result = show_one(getrlimit, Resource::CPU, ShowLimitType::Soft);
        assert_matches!(result, Err(Error::UnsupportedResource));
    }
}
