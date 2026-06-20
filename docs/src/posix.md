# POSIX compliance

**POSIX** (Portable Operating System Interface) is a set of standards specified by the IEEE to ensure compatibility among Unix-like operating systems. It defines a standard operating system interface and environment, including command-line utilities, shell scripting, and system calls.

As of 2025, the latest version of POSIX is [POSIX.1-2024](https://pubs.opengroup.org/onlinepubs/9799919799/). The requirements for shells are mainly documented in the [Shell & Utilities](https://pubs.opengroup.org/onlinepubs/9799919799/utilities/toc.html) volume. Yash-rs aims to comply with the POSIX standard, providing a consistent and portable environment for shell scripting and command execution.

The shell currently supports running shell scripts and basic interactive features in a POSIX-compliant manner. See the [homepage](index.html) for an overview of implemented features. Progress on POSIX conformance and feature implementation is tracked in [GitHub Issues](https://github.com/magicant/yash-rs/issues) and the [GitHub Project](https://github.com/users/magicant/projects/2).

Many features of yash-rs are still under development, and some may not yet be fully compliant with the POSIX standard. Any non-conforming behavior is described in the Compatibility section of each feature's documentation. These sections also clarify which aspects of the shell's behavior are POSIX requirements and which are shell-specific extensions.

## Maximizing POSIX compliance

Some behaviors of yash-rs prioritize convenience over POSIX compliance. The [`posixlycorrect` option](environment/options.md#posixlycorrect) disables such features. When this option is set:

- The shell exits immediately even if there are suspended jobs, when the [`exit` built-in](builtins/exit.md) is executed or end-of-file is reached in an interactive shell. (See [Suspended jobs](termination.md#suspended-jobs).)
