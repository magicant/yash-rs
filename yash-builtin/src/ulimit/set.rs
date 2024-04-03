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

use super::{Error, ResourceExt as _, SetLimitType, SetLimitValue};
use std::io::ErrorKind;
use yash_env::system::resource::{LimitPair, Resource, RLIM_INFINITY};
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
    new_limit: SetLimitValue,
) -> Result<(), super::Error> {
    let old_limits = env.getrlimit(resource).map_err(|e| {
        if e.kind() == ErrorKind::InvalidInput {
            Error::UnsupportedResource
        } else {
            Error::Unknown(e)
        }
    })?;

    let new_limit = match new_limit {
        SetLimitValue::Number(limit) => limit
            .checked_mul(resource.scale())
            .filter(|limit| *limit != RLIM_INFINITY)
            .ok_or(Error::Overflow)?,
        SetLimitValue::Unlimited => RLIM_INFINITY,
        SetLimitValue::CurrentSoft => old_limits.soft,
        SetLimitValue::CurrentHard => old_limits.hard,
    };

    let new_limits = match limit_type {
        SetLimitType::Soft => LimitPair {
            soft: new_limit,
            hard: old_limits.hard,
        },
        SetLimitType::Hard => LimitPair {
            soft: old_limits.soft,
            hard: new_limit,
        },
        SetLimitType::Both => LimitPair {
            soft: new_limit,
            hard: new_limit,
        },
    };

    if new_limits.soft_exceeds_hard() {
        return Err(Error::SoftLimitExceedsHardLimit);
    }

    match env.setrlimit(resource, new_limits) {
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
    use yash_env::system::resource::{rlim_t, RLIM_INFINITY};
    use yash_env::VirtualSystem;

    #[test]
    fn set_soft_to_zero() {
        let mut system = VirtualSystem::new();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::Number(0),
        )
        .unwrap();

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
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::Number(0),
        )
        .unwrap();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Hard,
            SetLimitValue::Number(0),
        )
        .unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(limits, LimitPair { soft: 0, hard: 0 });
    }

    #[test]
    fn set_both_to_zero() {
        let mut system = VirtualSystem::new();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Both,
            SetLimitValue::Number(0),
        )
        .unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(limits, LimitPair { soft: 0, hard: 0 });
    }

    #[test]
    fn set_soft_to_unlimited() {
        let mut system = VirtualSystem::new();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::Number(0),
        )
        .unwrap();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::Unlimited,
        )
        .unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(
            limits,
            LimitPair {
                soft: RLIM_INFINITY,
                hard: RLIM_INFINITY
            }
        );
    }

    #[test]
    fn set_soft_to_current_hard() {
        let mut system = VirtualSystem::new();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::Number(0),
        )
        .unwrap();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::Number(1),
        )
        .unwrap();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Hard,
            SetLimitValue::Number(10),
        )
        .unwrap();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::CurrentHard,
        )
        .unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(limits, LimitPair { soft: 10, hard: 10 });
    }

    #[test]
    fn set_hard_to_current_soft() {
        let mut system = VirtualSystem::new();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::Number(10),
        )
        .unwrap();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Hard,
            SetLimitValue::CurrentSoft,
        )
        .unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(limits, LimitPair { soft: 10, hard: 10 });
    }

    #[test]
    fn set_soft_keeps_hard_intact() {
        let mut system = VirtualSystem::new();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Both,
            SetLimitValue::Number(4),
        )
        .unwrap();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::Number(1),
        )
        .unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(limits, LimitPair { soft: 1, hard: 4 });
    }

    #[test]
    fn set_hard_keeps_soft_intact() {
        let mut system = VirtualSystem::new();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::Number(0),
        )
        .unwrap();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Hard,
            SetLimitValue::Number(4),
        )
        .unwrap();

        let limits = System::getrlimit(&system, Resource::CPU).unwrap();
        assert_eq!(limits, LimitPair { soft: 0, hard: 4 });
    }

    #[test]
    fn set_soft_finite_larger_than_hard() {
        let mut system = VirtualSystem::new();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Both,
            SetLimitValue::Number(4),
        )
        .unwrap();

        let result = set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::Number(5),
        );
        assert_matches!(result, Err(Error::SoftLimitExceedsHardLimit));
    }

    #[test]
    fn set_soft_infinite_larger_than_hard() {
        let mut system = VirtualSystem::new();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Both,
            SetLimitValue::Number(4),
        )
        .unwrap();

        let result = set(
            &mut system,
            Resource::CPU,
            SetLimitType::Soft,
            SetLimitValue::Unlimited,
        );
        assert_matches!(result, Err(Error::SoftLimitExceedsHardLimit));
    }

    #[test]
    fn set_scaled_limit() {
        let mut system = VirtualSystem::new();
        set(
            &mut system,
            Resource::FSIZE,
            SetLimitType::Both,
            SetLimitValue::Number(10),
        )
        .unwrap();

        let limits = System::getrlimit(&system, Resource::FSIZE).unwrap();
        assert_eq!(
            limits,
            LimitPair {
                soft: 5120, // 10 * Resource::FSIZE.scale()
                hard: 5120
            }
        );
    }

    #[test]
    fn set_scaled_limit_overflow() {
        let mut system = VirtualSystem::new();
        let result = set(
            &mut system,
            Resource::FSIZE,
            SetLimitType::Both,
            SetLimitValue::Number(rlim_t::MAX / 2),
        );
        assert_matches!(result, Err(Error::Overflow));
    }

    #[test]
    fn set_limit_number_equal_to_infinity() {
        let mut system = VirtualSystem::new();
        let result = set(
            &mut system,
            Resource::CPU,
            SetLimitType::Both,
            SetLimitValue::Number(RLIM_INFINITY),
        );
        assert_matches!(result, Err(Error::Overflow));
    }

    #[test]
    fn set_raising_hard() {
        let mut system = VirtualSystem::new();
        set(
            &mut system,
            Resource::CPU,
            SetLimitType::Both,
            SetLimitValue::Number(0),
        )
        .unwrap();

        let result = set(
            &mut system,
            Resource::CPU,
            SetLimitType::Hard,
            SetLimitValue::Number(1),
        );
        assert_matches!(result, Err(Error::NoPermissionToRaiseHardLimit));
    }

    #[test]
    fn set_unsupported_resource() {
        // TODO
    }
}
