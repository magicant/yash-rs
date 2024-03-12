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
fn format_one_time<W>(seconds: f64, result: &mut W) -> std::fmt::Result
where
    W: std::fmt::Write,
{
    // Make sure the seconds are rounded to 6 decimal places. Without this, the
    // result may be something like "0m60.000000s" instead of "1m0.000000s".
    let seconds = (seconds * 1000000.0).round() / 1000000.0;

    let minutes = seconds.div_euclid(60.0);
    let sub_minute_seconds = seconds.rem_euclid(60.0);
    write!(result, "{minutes:.0}m{sub_minute_seconds:.6}s")
}

/// Formats the result of the times built-in.
///
/// This function takes a `Times` structure and returns a string that is to be
/// printed to the standard output. See the
/// [parent module documentation](crate::times) for the format.
pub fn format(times: &Times) -> String {
    let mut result = String::with_capacity(64);

    // The Write impl for String never returns an error, so unwrap is safe here.
    format_one_time(times.self_user, &mut result).unwrap();
    result.push(' ');
    format_one_time(times.self_system, &mut result).unwrap();
    result.push('\n');
    format_one_time(times.children_user, &mut result).unwrap();
    result.push(' ');
    format_one_time(times.children_system, &mut result).unwrap();
    result.push('\n');

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_one_time_zero() {
        let mut result = String::new();
        format_one_time(0.0, &mut result).unwrap();
        assert_eq!(result, "0m0.000000s");
    }

    #[test]
    fn format_one_time_less_than_one_second() {
        let mut result = String::new();
        format_one_time(0.5, &mut result).unwrap();
        assert_eq!(result, "0m0.500000s");
    }

    #[test]
    fn format_one_time_one_second() {
        let mut result = String::new();
        format_one_time(1.0, &mut result).unwrap();
        assert_eq!(result, "0m1.000000s");
    }

    #[test]
    fn format_one_time_more_than_one_second() {
        let mut result = String::new();
        format_one_time(12.25, &mut result).unwrap();
        assert_eq!(result, "0m12.250000s");
    }

    #[test]
    fn format_one_time_more_than_one_minute() {
        let mut result = String::new();
        format_one_time(1234.50, &mut result).unwrap();
        assert_eq!(result, "20m34.500000s");
    }

    #[test]
    fn format_one_time_almost_one_minute() {
        let mut result = String::new();
        format_one_time(59.9999990, &mut result).unwrap();
        assert_eq!(result, "0m59.999999s");

        let mut result = String::new();
        format_one_time(59.9999999, &mut result).unwrap();
        assert_eq!(result, "1m0.000000s");
    }

    #[test]
    fn format_times() {
        let times = Times {
            self_user: 12.5,
            self_system: 65.25,
            children_user: 24.75,
            children_system: 600.0,
        };
        let result = format(&times);
        assert_eq!(
            result,
            "0m12.500000s 1m5.250000s\n0m24.750000s 10m0.000000s\n"
        );
    }
}
