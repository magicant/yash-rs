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

Even when yash-rs conforms to POSIX, it also implements extensions that POSIX does not specify. Such extensions are convenient, but scripts that rely on them may not run on other shells. The [`portable` option](environment/options.md#portable) helps you catch this: when set, the shell rejects or ignores non-portable features so that you can verify a script uses only portable constructs.

Unlike [`posixlycorrect`](environment/options.md#posixlycorrect), which changes how the shell behaves to maximize POSIX conformance, `portable` does not alter the behavior of POSIX-conformant constructs. It only restricts the shell to features that are portable across POSIX-conforming shells, reporting an error or ignoring a feature when a non-portable construct is used. The two options are independent and can be combined.

When the `portable` option is set, the shell reacts to non-portable features in one of the ways described below. Each item links to the page that documents the feature in full, including the POSIX requirement behind the restriction and how to write the same thing portably.

<!--
Rules for writing the lists below. Ask the maintainer when a case is unclear.

- Put each item in the list that matches how the shell reacts to the construct:
  an error, silent ignoring, or a warning. An item never appears in two lists.
- State only what the option rejects, ignores, or warns about. Do not restate
  the POSIX requirement behind it; that is just the same thing said twice. Do
  not restate the reaction either; the heading above the list already gives it.
- Add an example only when the description does not already show the construct
  itself as a literal (its symbols, spelling, or syntactic form). A description
  that only states a property or condition, leaving the reader to work out what
  the construct looks like, needs one.
- When an example is needed: list every construct if they are finitely many,
  will not grow, and fit on one line; otherwise give one representative example.
  Show the syntactic form instead when that is shorter and more precise than an
  instance of it.
- At most one example per condition. An item that states two independent
  conditions may have one example for each.
- Put each example in parentheses right after the part it illustrates. How the
  parentheses read is up to the item.
- Do not explain how to write the construct portably, and do not spell out the
  options or operands of each built-in here. Those belong on the linked page.
- The two items that cover the options and the operands of several built-ins are
  exempt from the example rules above: any example there would name a specific
  option or operand, which the rule just above keeps out of this page.
- Mention a case the option does not reject only when readers would otherwise
  think it is rejected.
- Keep the `(Since x.y.z)` tag at the head of each item. For an item covering
  several built-ins, use the version in which the item itself first appeared.
- Order the items within each list by where their linked page appears in
  SUMMARY.md. The linked page is the one that documents the item in full,
  including the portable alternative.
-->

### Features that cause an error

The shell reports an error and does not run the construct:

- (Since 3.3.0) A non-portable escape sequence in a [dollar-single-quoted string](language/words/quoting.md#dollar-single-quotes) (`$'…'`): the `\E`, `\?`, `\u`, and `\U` escapes, the `\c@` control escape, and `\x` followed by more than two hexadecimal digits.
- (Since 3.3.0) A [reserved word](language/words/keywords.md#where-are-reserved-words-recognized) that immediately follows a subshell or a redirection without a separator (for example, the `}` in `{ ( foo ) }`).
- (Since 3.3.0) A command name ending with a `:` (for example, `foo:`), in a position where a [reserved word](language/words/keywords.md#where-are-reserved-words-recognized) would be recognized. The lone `:` ([colon built-in](builtins/colon.md)) is not affected.
- (Since 3.3.0) A [parameter expansion](language/words/parameters.md) that uses a length or switch modifier with special parameter `*` or `@` (for example, `${#*}`), or a trim modifier with special parameter `#`, `*`, or `@` (for example, `${*#word}`).
- (Since 3.3.0) An [assignment](language/commands/simple.md#syntax) whose [variable name](language/parameters/variables.md#variable-names) starts with a digit or contains a character other than ASCII letters, digits, and underscores (for example, `1st=foo`).
- (Since 3.3.3) An operand naming a [variable](language/parameters/variables.md#variable-names) whose name starts with a digit or contains a character other than ASCII letters, digits, and underscores (for example, `1st`), given to the [`export`](builtins/export.md), [`getopts`](builtins/getopts.md), [`read`](builtins/read.md), [`readonly`](builtins/readonly.md), or [`typeset`](builtins/typeset.md) built-in.
- (Since 3.3.1) An [array assignment](language/parameters/variables.md#arrays) (`name=(...)`).
- (Since 3.3.0) A `!` immediately followed by a `(` at the beginning of a [pipeline](language/commands/pipelines.md#negation).
- (Since 3.3.0) A `((` at the beginning of a command, where the first `(` opens a [subshell](language/commands/grouping.md#subshells).
- (Since 3.3.0) The `;;&` and `;|` terminators in [case commands](language/commands/case.md).
- (Since 3.3.0) A [`for` loop](language/commands/loops.md#for-loops) variable name that is quoted, contains an expansion, starts with a digit, or contains a character other than ASCII letters, digits, and underscores (for example, `for "i"`).
- (Since 3.3.0) A [function](language/functions.md) name that is quoted, contains an expansion, starts with a digit, or contains a character other than ASCII letters, digits, and underscores (for example, `"foo"() { :; }`).
- (Since 3.3.0) A [function](language/functions.md) name that is the same as a [special built-in](builtins/index.html#special-built-ins) utility name (for example, `export`). Other built-in names, such as `cd` and `source`, are not affected.
- (Since 3.3.1) Defining an [alias](language/aliases.md#alias-names) with a name that contains a character other than ASCII letters, digits, `!`, `%`, `,`, `-`, `@`, or `_`.
- (Since 3.3.0) The [redirection](language/redirections/index.html) operators `>>|` and `<<<`.
- (Since 3.3.0) A word that would be recognized as a file descriptor specification, used as the target of a [redirection](language/redirections/index.html) (for example, the `1` in `< 1>file`).
- (Since 3.3.5) A [shell option](environment/options.md#compatibility) given to the [`set` built-in](builtins/set.md) or on the [command line](startup.md) in a spelling POSIX does not specify (for example, `set --errexit`). The `portable` option itself is always accepted, so that it can be turned off again.
- (Since 3.3.5) An argument to `-o` or `+o` written in the same argument as the option itself on the [command line](startup.md#compatibility) (for example, `yash3 -oerrexit`).
- (Since 3.3.4) Executing an [elective or extension built-in](builtins/index.html#elective-built-ins) (for example, [`typeset`](builtins/typeset.md)).
- (Since 3.3.5) An option that POSIX does not specify, a [long option](builtins/index.html#options), an [option argument](builtins/index.html#option-arguments) written in the same argument as the option name, or a combination of options POSIX does not allow, given to the [`cd`](builtins/cd.md), [`command`](builtins/command.md), [`exit`](builtins/exit.md), [`export`](builtins/export.md), [`jobs`](builtins/jobs.md), [`pwd`](builtins/pwd.md), [`read`](builtins/read.md), [`readonly`](builtins/readonly.md), [`return`](builtins/return.md), [`trap`](builtins/trap.md), [`ulimit`](builtins/ulimit.md), or [`unset`](builtins/unset.md) built-in.
- (Since 3.3.5) A number of operands that POSIX does not allow with the accompanying options, given to the [`.`](builtins/source.md), [`command`](builtins/command.md), [`export`](builtins/export.md), [`readonly`](builtins/readonly.md), [`type`](builtins/type.md), or [`unset`](builtins/unset.md) built-in.
- (Since 3.3.5) A non-portable way of specifying a signal or listing signals with the [`kill` built-in](builtins/kill.md).
- (Since 3.3.3) Making the `PWD`, `OLDPWD`, `OPTIND`, `OPTARG`, or `LINENO` [variable](language/parameters/variables.md#reserved-variable-names) read-only with the [`readonly` built-in](builtins/readonly.md).
- (Since 3.4.1) A `-` used as an option-operand separator with the [`set` built-in](builtins/set.md) (for example, `set - foo`).
- (Since 3.3.4) Executing the [`.` built-in](builtins/source.md) under the name `source`.
- (Since 3.3.2) The increment and decrement operators (`++` and `--`) in an [arithmetic expression](arithmetic.md).

### Features that trigger a warning

The shell prints a warning to the standard error and runs the construct anyway:

- (Since 3.3.5) An argument, including a lone `--`, given to the [`false`](builtins/false.md) or [`true`](builtins/true.md) built-in.

### Features that are ignored

The shell silently proceeds as if the construct were absent:

- (Since 3.3.3) An [environment variable](language/parameters/variables.md#environment-variables) inherited at [shell startup](startup.md#compatibility) whose name starts with a digit or contains a character other than ASCII letters, digits, and underscores.

The `portable` option is still under development, so these lists will be expanded as more checks are implemented.
