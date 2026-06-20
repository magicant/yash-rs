# Versioning and compatibility

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
