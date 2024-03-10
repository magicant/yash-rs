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

//! Formatting the result of the times built-in

use yash_env::system::Times;

/// Formats a single time.
///
/// This function panics if the `ticks_per_second` is zero.
fn format_one_time<W>(ticks: u64, ticks_per_second: u64, result: &mut W) -> std::fmt::Result
where
    W: std::fmt::Write,
{
    let seconds = ticks / ticks_per_second;
    let minutes = seconds / 60;
    let sub_minute_ticks = ticks - minutes * 60 * ticks_per_second;
    let seconds = sub_minute_ticks as f64 / ticks_per_second as f64;
    write!(result, "{minutes}m{seconds:.6}s")
}

/// Formats the result of the times built-in.
///
/// This function takes a `Times` structure and returns a string that is to be
/// printed to the standard output. See the
/// [parent module documentation](crate::times) for the format.
pub fn format(times: &Times) -> String {
    let mut result = String::with_capacity(64);

    format_one_time(times.self_user, times.ticks_per_second, &mut result).unwrap();
    result.push(' ');
    format_one_time(times.self_system, times.ticks_per_second, &mut result).unwrap();
    result.push('\n');
    format_one_time(times.children_user, times.ticks_per_second, &mut result).unwrap();
    result.push(' ');
    format_one_time(times.children_system, times.ticks_per_second, &mut result).unwrap();
    result.push('\n');

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_one_time_zero() {
        let mut result = String::new();
        format_one_time(0, 100, &mut result).unwrap();
        assert_eq!(result, "0m0.000000s");
    }

    #[test]
    fn format_one_time_less_than_one_second() {
        let mut result = String::new();
        format_one_time(50, 100, &mut result).unwrap();
        assert_eq!(result, "0m0.500000s");
    }

    #[test]
    fn format_one_time_one_second() {
        let mut result = String::new();
        format_one_time(1000, 1000, &mut result).unwrap();
        assert_eq!(result, "0m1.000000s");
    }

    #[test]
    fn format_one_time_more_than_one_second() {
        let mut result = String::new();
        format_one_time(1225, 100, &mut result).unwrap();
        assert_eq!(result, "0m12.250000s");
    }

    #[test]
    fn format_one_time_more_than_one_minute() {
        let mut result = String::new();
        format_one_time(123450, 100, &mut result).unwrap();
        assert_eq!(result, "20m34.500000s");
    }

    #[test]
    fn format_times() {
        let times = Times {
            self_user: 1250,
            self_system: 6525,
            children_user: 2475,
            children_system: 60000,
            ticks_per_second: 100,
        };
        let result = format(&times);
        assert_eq!(
            result,
            "0m12.500000s 1m5.250000s\n0m24.750000s 10m0.000000s\n"
        );
    }
}
