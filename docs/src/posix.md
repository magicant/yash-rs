# POSIX compliance

**POSIX** (Portable Operating System Interface) is a set of standards specified by the IEEE to ensure compatibility among Unix-like operating systems. It defines a standard operating system interface and environment, including command-line utilities, shell scripting, and system calls.

As of 2025, the latest version of POSIX is [POSIX.1-2024](https://pubs.opengroup.org/onlinepubs/9799919799/). The requirements for shells are mainly documented in the [Shell & Utilities](https://pubs.opengroup.org/onlinepubs/9799919799/utilities/toc.html) volume. Yash-rs aims to comply with the POSIX standard, providing a consistent and portable environment for shell scripting and command execution.

The shell currently supports running shell scripts and basic interactive features in a POSIX-compliant manner. See the [homepage](index.html) for an overview of implemented features. Progress on POSIX conformance and feature implementation is tracked in [GitHub Issues](https://github.com/magicant/yash-rs/issues) and the [GitHub Project](https://github.com/users/magicant/projects/2).

Many features of yash-rs are still under development, and some may not yet be fully compliant with the POSIX standard. Any non-conforming behavior is described in the Compatibility section of each feature's documentation. These sections also clarify which aspects of the shell's behavior are POSIX requirements and which are shell-specific extensions.

## Maximizing POSIX compliance

Some behaviors of yash-rs prioritize convenience over POSIX compliance. The [`posixlycorrect` option](environment/options.md#posixlycorrect) disables such features. When this option is set:

- The shell no longer refuses to exit because of suspended jobs when the [`exit` built-in](builtins/exit.md) is executed or end-of-file is reached in an interactive shell. (See [Suspended jobs](termination.md#suspended-jobs).)
- [Extension built-ins](builtins/index.html#extension-built-ins) are ignored (treated as non-existing), so the shell falls through to searching for an external utility with the same name.

This list may be expanded in the future as more features are added to the shell.

## Writing portable scripts

Even when yash-rs conforms to POSIX, it also implements extensions that POSIX does not specify. Such extensions are convenient, but scripts that rely on them may not run on other shells. (Since 3.3.0) The [`portable` option](environment/options.md#portable) helps you catch this: when set, the shell rejects or ignores non-portable features so that you can verify a script uses only portable constructs.

Unlike [`posixlycorrect`](environment/options.md#posixlycorrect), which changes how the shell behaves to maximize POSIX conformance, `portable` does not alter the behavior of POSIX-conformant constructs. It only restricts the shell to features that are portable across POSIX-conforming shells, reporting an error or ignoring a feature when a non-portable construct is used. The two options are independent and can be combined.

When the `portable` option is set, the shell rejects the following non-portable constructs:

- The `;;&` and `;|` terminators in [case commands](language/commands/case.md).
- The non-portable [redirection](language/redirections/index.html) operators `>>|` and `<<<`.
- A number or `{...}` token immediately followed by `<` or `>` used as a redirection operand (for example, the `1` in `< 1>file`). Separate it with a space or quote it instead.
- A reserved word that immediately follows a subshell or a redirection without a separator (see [where reserved words are recognized](language/words/keywords.md#where-are-reserved-words-recognized)). POSIX recognizes a reserved word only when it begins a command or follows another reserved word; a subshell ends with `)` and a redirection ends with a word, so a clause-delimiting reserved word right after one is not recognized. Insert `;` or a newline before it. This affects `}`, `done`, `fi`, `then`, `elif`, `else`, `esac`, and `do` (for example, write `{ ( foo ); }` instead of `{ ( foo ) }`, and `for i in 1; do ( foo ); done` instead of `for i in 1; do ( foo ) done`).
- A non-portable escape sequence in a [dollar-single-quoted string](language/words/quoting.md#dollar-single-quotes) (`$'…'`): the `\E`, `\?`, `\u`, and `\U` escapes, the `\c@` control escape, and `\x` followed by more than two hexadecimal digits.
- A `((` or `!(` at the beginning of a command. Other shells parse `((…))` as an arithmetic command and `!(…)` as an extended glob, neither of which yash-rs supports. Insert a space (`( (` to nest [subshells](language/commands/grouping.md#subshells), or `! (` to negate one).
- A command name ending with a `:` (for example, `foo:`). POSIX [reserves words](language/words/keywords.md) whose final character is a `:` for possible future use, so using one where a reserved word would be recognized produces unspecified results. The lone `:` ([colon built-in](builtins/colon.md)) is not affected.
- A [`for` loop](language/commands/loops.md#for-loops) [variable name](language/parameters/variables.md#variable-names) that is quoted, contains an expansion, or starts with a digit. POSIX requires the name to be an unquoted word consisting solely of underscores, digits, and alphabetics from the portable character set, not starting with a digit.

The `portable` option is still under development, so this list will be expanded as more checks are implemented.
