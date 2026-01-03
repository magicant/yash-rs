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
use yash_env::system::Errno;
use yash_env::system::resource::{GetRlimit, INFINITY, LimitPair, Resource, SetRlimit};

/// Sets the limit for a specific resource.
pub fn set<E: GetRlimit + SetRlimit>(
    env: &mut E,
    resource: Resource,
    limit_type: SetLimitType,
    new_limit: SetLimitValue,
) -> Result<(), super::Error> {
    let old_limits = env.getrlimit(resource).map_err(|errno| {
        if errno == Errno::EINVAL {
            Error::UnsupportedResource
        } else {
            Error::Unknown(errno)
        }
    })?;

    let new_limit = match new_limit {
        SetLimitValue::Number(limit) => limit
            .checked_mul(resource.scale())
            .filter(|limit| *limit != INFINITY)
            .ok_or(Error::Overflow)?,
        SetLimitValue::Unlimited => INFINITY,
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
        Err(Errno::EPERM) => Err(Error::NoPermissionToRaiseHardLimit),
        Err(errno) => Err(Error::Unknown(errno)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use yash_env::VirtualSystem;
    use yash_env::system::resource::Limit;

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

        let limits = system.getrlimit(Resource::CPU).unwrap();
        assert_eq!(
            limits,
            LimitPair {
                soft: 0,
                hard: INFINITY
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

        let limits = system.getrlimit(Resource::CPU).unwrap();
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

        let limits = system.getrlimit(Resource::CPU).unwrap();
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

        let limits = system.getrlimit(Resource::CPU).unwrap();
        assert_eq!(
            limits,
            LimitPair {
                soft: INFINITY,
                hard: INFINITY
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

        let limits = system.getrlimit(Resource::CPU).unwrap();
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

        let limits = system.getrlimit(Resource::CPU).unwrap();
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

        let limits = system.getrlimit(Resource::CPU).unwrap();
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

        let limits = system.getrlimit(Resource::CPU).unwrap();
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

        let limits = system.getrlimit(Resource::FSIZE).unwrap();
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
            SetLimitValue::Number(Limit::MAX / 2),
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
            SetLimitValue::Number(INFINITY),
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
        struct ResourcelessSystem;
        impl GetRlimit for ResourcelessSystem {
            fn getrlimit(&self, _resource: Resource) -> Result<LimitPair, Errno> {
                Err(Errno::EINVAL)
            }
        }
        impl SetRlimit for ResourcelessSystem {
            fn setrlimit(&self, _resource: Resource, _limits: LimitPair) -> Result<(), Errno> {
                Err(Errno::EINVAL)
            }
        }

        let mut system = ResourcelessSystem;
        let result = set(
            &mut system,
            Resource::CPU,
            SetLimitType::Both,
            SetLimitValue::Number(0),
        );
        assert_matches!(result, Err(Error::UnsupportedResource));
    }
}
