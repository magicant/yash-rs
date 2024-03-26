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

//! Setting resource limits

use super::{Error, SetLimitType};
use std::io::ErrorKind;
use yash_env::system::resource::{rlim_t, LimitPair, Resource};
use yash_env::System;

/// Environment for setting resource limits
///
/// This trait is a subset of [`System`] that is used for
/// setting resource limits.
pub trait Env {
    /// See [`System::getrlimit`]
    fn getrlimit(&self, resource: Resource) -> Result<LimitPair, std::io::Error>;
    /// See [`System::setrlimit`]
    fn setrlimit(&mut self, resource: Resource, limits: LimitPair) -> Result<(), std::io::Error>;
}

impl<T: System> Env for T {
    #[inline(always)]
    fn getrlimit(&self, resource: Resource) -> Result<LimitPair, std::io::Error> {
        System::getrlimit(self, resource)
    }

    #[inline(always)]
    fn setrlimit(&mut self, resource: Resource, limits: LimitPair) -> Result<(), std::io::Error> {
        System::setrlimit(self, resource, limits)
    }
}

/// Sets the limit for a specific resource.
pub fn set<E: Env>(
    env: &mut E,
    resource: Resource,
    limit_type: SetLimitType,
    limit: rlim_t,
) -> Result<(), super::Error> {
    let limits = env.getrlimit(resource).map_err(|e| {
        if e.kind() == ErrorKind::InvalidInput {
            Error::UnsupportedResource
        } else {
            Error::Unknown(e)
        }
    })?;
    let limits = match limit_type {
        SetLimitType::Soft => LimitPair {
            soft: limit,
            hard: limits.hard,
        },
        SetLimitType::Hard => LimitPair {
            soft: limits.soft,
            hard: limit,
        },
        SetLimitType::Both => LimitPair {
            soft: limit,
            hard: limit,
        },
    };

    if limits.soft_exceeds_hard() {
        return Err(Error::SoftLimitExceedsHardLimit);
    }

    match env.setrlimit(resource, limits) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::PermissionDenied => {
            Err(Error::NoPermissionToRaiseHardLimit)
        }
        Err(e) => Err(Error::Unknown(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use yash_env::system::resource::RLIM_INFINITY;
    use yash_env::VirtualSystem;

    #[test]
    fn set_soft_to_zero() {
        let mut system = VirtualSystem::new();
        set(&mut system, Resource::CPU, SetLimitType::Soft, 0).unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(
            limits,
            LimitPair {
                soft: 0,
                hard: RLIM_INFINITY
            }
        );
    }

    #[test]
    fn set_hard_to_zero() {
        let mut system = VirtualSystem::new();
        set(&mut system, Resource::CPU, SetLimitType::Soft, 0).unwrap();
        set(&mut system, Resource::CPU, SetLimitType::Hard, 0).unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(limits, LimitPair { soft: 0, hard: 0 });
    }

    #[test]
    fn set_both_to_zero() {
        let mut system = VirtualSystem::new();
        set(&mut system, Resource::CPU, SetLimitType::Both, 0).unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(limits, LimitPair { soft: 0, hard: 0 });
    }

    #[test]
    fn set_soft_keeps_hard_intact() {
        let mut system = VirtualSystem::new();
        set(&mut system, Resource::CPU, SetLimitType::Both, 4).unwrap();
        set(&mut system, Resource::CPU, SetLimitType::Soft, 1).unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(limits, LimitPair { soft: 1, hard: 4 });
    }

    #[test]
    fn set_hard_keeps_soft_intact() {
        let mut system = VirtualSystem::new();
        set(&mut system, Resource::CPU, SetLimitType::Soft, 0).unwrap();
        set(&mut system, Resource::CPU, SetLimitType::Hard, 4).unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(limits, LimitPair { soft: 0, hard: 4 });
    }

    #[test]
    fn set_soft_finite_larger_than_hard() {
        let mut system = VirtualSystem::new();
        set(&mut system, Resource::CPU, SetLimitType::Both, 4).unwrap();

        let result = set(&mut system, Resource::CPU, SetLimitType::Soft, 5);
        assert_matches!(result, Err(Error::SoftLimitExceedsHardLimit));
    }

    #[test]
    fn set_soft_infinite_larger_than_hard() {
        let mut system = VirtualSystem::new();
        set(&mut system, Resource::CPU, SetLimitType::Both, 4).unwrap();

        let result = set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            RLIM_INFINITY,
        );
        assert_matches!(result, Err(Error::SoftLimitExceedsHardLimit));
    }

    #[test]
    fn set_raising_hard() {
        let mut system = VirtualSystem::new();
        set(&mut system, Resource::CPU, SetLimitType::Both, 0).unwrap();

        let result = set(&mut system, Resource::CPU, SetLimitType::Hard, 1);
        assert_matches!(result, Err(Error::NoPermissionToRaiseHardLimit));
    }
}
