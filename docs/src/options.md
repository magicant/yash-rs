# Shell options

**Shell options** control the behavior of the shell. You can enable (set) or disable (unset) them using command line arguments at startup or with the `set` built-in during a shell session.

## Enabling and disabling options

You can specify shell options as command line arguments when starting the shell, or with the `set` built-in. In yash, all options have a long name, and some also have a short name.

Options set at startup take effect before the shell reads and executes commands. Options set with `set` affect the current shell session. Some options are only available at startup; others can be changed at any time. The syntax is the same in both cases.

### Long option names

Long options start with `--`. For example, to enable the `allexport` option at startup:

```sh
yash3 --allexport
```

You can also specify long options with the `-o` option:

```sh
yash3 -o allexport
```

Only alphanumeric characters matter in long option names, and they are case-insensitive. For example, `--all-export`, `--ALLEXPORT`, and `---All*Ex!PorT` all enable `allexport`.

Long option names can be abbreviated if unambiguous. For example, `--cl` enables `clobber`:

```shell
$ set --cl
$ set --c
error: ambiguous option name "--c"
 --> <stdin>:2:5
  |
2 | set --c
  | --- ^^^ --c
  | |
  | info: executing the set built-in
  |
```

Note: Future versions may add more options, so abbreviations that work now may become ambiguous later. For forward compatibility, use full option names.

To disable a long option, prepend `no` to the name:

```sh
yash3 --noallexport
```

Or use `++` instead of `--`:

```sh
yash3 ++allexport
```

Or use `+o` instead of `-o`:

```sh
yash3 +o allexport
```

If you use both `+` and `no`, it is a double negation and enables the option:

```sh
yash3 +o noallexport
```

### Short option names

Some options have short names, specified as a single character. For example, to enable `allexport` with its short name:

```sh
yash3 -a
```

To disable it:

```sh
yash3 +a
```

You can combine multiple short options in one argument:

```sh
yash3 -aex
```

Some short options negate long options. For example, `-C` is the same as `--noclobber` (disables `clobber`). To enable `clobber` with its short name, use `+C`.

## Viewing current options

To see current shell options, use `set -o` with no arguments:

```shell
$ set -o
allexport        off
clobber          on
cmdline          off
errexit          off
exec             on
glob             on
hashondefinition off
ignoreeof        off
interactive      off
log              on
login            off
monitor          off
notify           off
posixlycorrect   off
stdin            on
unset            on
verbose          off
vi               off
xtrace           off
```

`set +o` prints options in a format that can be used to restore them:

```shell
$ set +o
set +o allexport
set -o clobber
#set +o cmdline
set +o errexit
set -o exec
set -o glob
set +o hashondefinition
set +o ignoreeof
#set +o interactive
set -o log
set +o login
set +o monitor
set +o notify
set +o posixlycorrect
#set -o stdin
set -o unset
set +o verbose
set +o vi
set +o xtrace
```

```shell
$ set +o allexport
$ savedoptions=$(set +o)
$ set -o allexport
$ eval "$savedoptions"
$ set -o | grep allexport
allexport        off
```

The `-` [special parameter](language/parameters/special.md) contains the currently set short options. For example, if `-i` and `-m` are set, the value of `-` is `im`. Options without a short name are not included. Short options that negate long options are included when the long option is unset.

```shell
$ set -a -o noclobber
$ echo "$-"
aCs
```

<!-- TODO: test built-in -->

## Option list

Below is a list of all shell options in yash-rs, with their long and short names, and a brief description. Unless noted, all options are disabled by default.

- **`allexport`** (**`-a`**): If set, all [variables] assigned in the shell are [exported](language/parameters/variables.md#environment-variables).

- **`clobber`** (**`+C`**): If set (default), the `>` [redirection](language/redirections/index.html) operator overwrites existing files. If unset, `>` fails if the file exists. The `>|` operator always overwrites files.

- **`cmdline`** (**`-c`**): If set, the shell executes the first operand from the command line as a command. Mutually exclusive with `stdin`, and only settable at startup. <!-- TODO: Link to startup -->

- **`errexit`** (**`-e`**): If set, the shell [exits](termination.md) if a command fails. Useful for scripts to stop on errors. <!-- TODO: link to the debugging section -->

- **`exec`** (**`+n`**): If set (default), the shell executes commands. If unset, it only parses commands (useful for syntax checking). <!-- TODO: Link to debugging -->
    - Once unset, it cannot be set again in the same session.
    - In interactive shells, this option is ignored and commands are always executed.

- **`glob`** (**`+f`**): If set (default), the shell performs [pathname expansion](language/words/globbing.md) on words containing metacharacters. If unset, pathname expansion is skipped.

- **`hashondefinition`** (**`-h`**): Deprecated and has no effect. Remains for compatibility.
    - The short name `-h` is currently a synonym for `--hashondefinition`, but this may change.
    - Many shells implement `-h` differently, so behavior may vary.

- **`ignoreeof`**: If set, the shell ignores end-of-file (usually `Ctrl+D`) and does not exit. See [Preventing accidental exits](termination.md#preventing-accidental-exits).
    - Only takes effect if the shell is interactive and input is a terminal. <!-- TODO: link to interactive shell -->

- **`interactive`** (**`-i`**): If set, the shell is interactive. <!-- TODO: link to interactive -->
    - Enabled on startup if `stdin` is enabled and standard input and error are terminals.

- **`log`**: Deprecated and has no effect. Remains for compatibility.

- **`login`** (**`-l`**): If set, the shell behaves as a login shell. Only settable at startup. <!-- TODO: link to startup -->
    - ⚠️ Currently has no effect in yash-rs. In the future, login shells will read extra initialization files.

- **`monitor`** (**`-m`**): If set, the shell performs job control for background jobs and suspended processes. <!-- TODO: link to job control -->
    - Enabled by default in interactive shells.

- **`notify`** (**`-b`**): If set, the shell notifies you of background job completions and suspensions as soon as they occur. If unset, notifications are delayed until the next prompt. <!-- TODO: link to job control -->
    - ⚠️ Currently has no effect in yash-rs. In the future, it will enable immediate notifications for background jobs.
    - Only takes effect if `interactive` and `monitor` are enabled.

- **`pipefail`**: If set, the shell returns the [exit status](language/commands/exit_status.md) of the last command in a [pipeline](language/commands/pipelines.md) that failed, instead of the last command's exit status.
    - ⚠️ Not yet implemented in yash-rs.

- **`posixlycorrect`**: If set, the shell behaves as POSIX-compliant as possible. Useful for portable scripts. <!-- TODO: link to POSIX compliance -->
    - Enabled on startup if the shell is started as `sh`.
    - When unset, yash-rs may deviate from POSIX in some areas.

- **`stdin`** (**`-s`**): If set, the shell reads commands from standard input. Mutually exclusive with `cmdline`, and only settable at startup. <!-- TODO: Link to startup -->
    - Enabled if `cmdline` is not set and the shell is started with no operands.

- **`unset`** (**`+u`**): If set (default), the shell [expands](language/words/parameters.md) unset [variables] to an empty string. If unset, expanding an unset variable raises an error.

- **`verbose`** (**`-v`**): If set, the shell prints each command before executing it. Useful for debugging. <!-- TODO: link to debugging -->

- **`vi`**: If set, the shell uses vi-style keybindings for command line editing. <!-- TODO: link to interactive shell and command line editing -->
    - ⚠️ Currently has no effect in yash-rs. In the future, it will enable vi-style editing in interactive shells.

- **`xtrace`** (**`-x`**): If set, the shell prints each field after [expansion](language/words/index.html#word-expansion), before executing it. Useful for debugging. <!-- TODO: link to debugging -->

## Compatibility

The syntax and options specified in POSIX.1-2024 are much more limited than those in yash-rs. For portable scripts, use only POSIX-specified syntax and options.

POSIX.1-2024 syntax:

- Enable a long option: `set -o optionname` (no `--` prefix).
- Disable a long option: `set +o optionname` (no `++` prefix).
- Long options are case-sensitive, must be spelled out in full, and cannot contain extra symbols.
- No support for `no`-prefix inversion of long options.
- Enable a short option: `-` followed by the option character.
- Disable a short option: `+` followed by the option character.
- Short options can be combined after the `-` or `+` prefix.
- View current options: `set -o` or `set +o`.

POSIX.1-2024 options:

- `-a`, `-o allexport`
- `-b`, `-o notify`
- `-C`, `-o noclobber`
- `-c`
- `-e`, `-o errexit`
- `-f`, `-o noglob`
- `-h`
- `-i`
- `-m`, `-o monitor`
- `-n`, `-o noexec`
- `-s`
- `-u`, `-o nounset`
- `-v`, `-o verbose`
- `-x`, `-o xtrace`
- `-o ignoreeof`
- `-o nolog`
- `-o pipefail`
- `-o vi`

[variables]: language/parameters/variables.md
