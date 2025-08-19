# Versioning and compatibility

## POSIX conformance

**POSIX** (Portable Operating System Interface) is a set of standards specified by the IEEE to ensure compatibility among Unix-like operating systems. It defines a standard operating system interface and environment, including command-line utilities, shell scripting, and system calls.

As of 2025, the latest version of POSIX is [POSIX.1-2024](https://pubs.opengroup.org/onlinepubs/9799919799/). The requirements for shells are mainly documented in the [Shell & Utilities](https://pubs.opengroup.org/onlinepubs/9799919799/utilities/toc.html) volume. Yash-rs aims to comply with the POSIX standard, providing a consistent and portable environment for shell scripting and command execution.

The shell currently supports running shell scripts and basic interactive features in a POSIX-compliant manner. See the [homepage](index.html) for an overview of implemented features. Progress on POSIX conformance and feature implementation is tracked in [GitHub Issues](https://github.com/magicant/yash-rs/issues) and the [GitHub Project](https://github.com/users/magicant/projects/2).

Many features of yash-rs are still under development, and some may not yet be fully compliant with the POSIX standard. Any non-conforming behavior is described in the Compatibility section of each feature's documentation. These sections also clarify which aspects of the shell's behavior are POSIX requirements and which are shell-specific extensions.

## Yash and yash-rs

Yash-rs is the successor to [yash](https://github.com/magicant/yash), which is also a POSIX-compliant shell supporting both scripting and interactive use. Since yash-rs currently lacks advanced interactive features such as command history and line editing, yash is recommended for interactive use at this time.

Current releases of yash use version numbers in the form `2.x` (or sometimes `2.x.y`), where `x` increases with each release. Earlier versions used `1.x` or `0.x`. To avoid version number conflicts, the `3.x` series is reserved for yash-rs, and there are no plans for yash to use `3.x` version numbers.

In this manual, "yash" may refer to both yash and yash-rs collectively. When a distinction is needed, we refer to the former as "previous versions of yash", "older versions of yash", and so on.

## Yash-rs versioning policy

Yash-rs follows [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). Each release has a version number consisting of three parts separated by dots, such as `3.0.1`. The first part is the major version, incremented for breaking changes. The second is the minor version, incremented for new features that do not break compatibility. The third is the patch version, incremented for bug fixes that do not introduce new features.

Semantic versioning applies to the observable behavior of yash-rs as documented in this manual. This includes (but is not limited to) the syntax and semantics of the shell language, shell startup and termination behavior, and the behavior of built-in utilities. The major version will be incremented if any of these behaviors change in a way that is not backward compatible.

We strive to minimize breaking changes by carefully defining the scope of behavior covered by semantic versioning. In this manual, we may declare certain behaviors as "subject to change", "may change in the future", and similar phrases to reserve the right to make changes without incrementing the major version. Additionally, the following changes are not considered breaking:

- Changes to undocumented or internal behavior
- New features that were previously documented as "not yet implemented/supported"
- New features that allow commands which previously resulted in errors to succeed
- Changes that make non-conforming behavior compliant with POSIX

See the [changelog](https://github.com/magicant/yash-rs/blob/master/yash-cli/CHANGELOG.md) for a detailed list of changes and updates to yash-rs. Note that the documentation at <https://magicant.github.io/yash-rs/> is deployed from the master branch and may include unreleased changes.
