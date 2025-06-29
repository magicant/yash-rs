# Shell options

**Shell options** are settings that control the behavior of the shell. They can be set (enabled) or unset (disabled) using command line arguments on startup, or using the `set` built-in during a shell session.

## Enabling and disabling options

Shell options are specified as command line arguments when starting the shell or calling the `set` built-in. In yash, all shell options have a long name, and some have a short name as well.

When options are specified on shell startup, they are set before the shell starts reading and executing commands. When options are specified to the `set` built-in, they are set in the current shell session. Some options can only be set at startup, while others can be set or unset at any time. The syntax for specifying options is the same in both cases.

### Long option names

Long options are specified as an argument starting with `--`. For example, to start yash-rs with the `allexport` option enabled:

```sh
yash3 --allexport
```

Long options can also be specified as an argument to the `-o` option:

```sh
yash3 -o allexport
```

Only alphanumeric characters are significant in long option names, and long options are case-insensitive. For example, `--all-export`, `--ALLEXPORT`, and `---All*Ex!PorT` all enable the `allexport` option.

Long option names can be abbreviated as long as they are unambiguous. For example, `--cl` is sufficient to enable the `clobber` option:

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

Note that future versions of yash-rs may support more options, so an abbreviation that is currently unambiguous may become ambiguous later. It is recommended to use the full option name for forward compatibility.

There are several ways to disable an option with a long name. One way is to prepend `no` to the option name:

```sh
yash3 --noallexport
```

Another is to start an argument with `++` instead of `--`:

```sh
yash3 ++allexport
```

You can also use the `+o` option to disable an option:

```sh
yash3 +o allexport
```

If you start an argument with `+` *and* prepend `no` to the option name, it is double negation, which is equivalent to enabling the option:

```sh
yash3 +o noallexport
```

### Short option names

Some shell options have short names, which are specified as a single character. For example, to start yash with the `allexport` option enabled using its short name, use an argument starting with `-`:

```sh
yash3 -a
```

To disable the `allexport` option using its short name, use an argument starting with `+`:

```sh
yash3 +a
```

Multiple short options can be specified in a single argument. For example, to enable the `allexport`, `errexit`, and `xtrace` options at startup:

```sh
yash3 -aex
```

Some short options correspond to negation of long options. For example, the `-C` option is equivalent to `--noclobber`, which disables the `clobber` option. To enable the `clobber` option using its short name, use `+C`.

## Viewing current options

FIXME TODO

## Option list

The following is a list of all shell options available in yash-rs, along with their long and short names, and a brief description of each option. Unless otherwise noted, all options are disabled by default.

- **`allexport`** (**`-a`**): If set, all [variables] assigned in the shell are [exported](language/parameters/variables.md#environment-variables).

- **`clobber`** (**`+C`**): If set (default), the `>` [redirection](language/redirections/README.md) operator overwrites existing files. If unset, the `>` operator fails if the target file already exists. The `>|` operator always overwrites files, regardless of the `clobber` option.

- **`cmdline`** (**`-c`**): If set, the shell executes the first operand from the command line as a command. This option is mutually exclusive with the `stdin` option, and can only be set at startup. <!-- TODO: Link to startup -->

- **`errexit`** (**`-e`**): If set, the shell exits if a command fails. This is useful for scripts where you want to ensure that any command failure causes the script to stop executing. <!-- TODO: link to termination and debugging -->

- **`exec`** (**`+n`**): If set (default), the shell actually executes commands. If unset, the shell only parses commands without executing them. This is useful for syntax checking of scripts. <!-- TODO: Link to debugging -->
    - Once this option is unset, it cannot be set again in the same shell session because you cannot execute commands anymore, including the `set` built-in to set options.
    - In interactive shells, this option is ignored and commands are always executed.

- **`glob`** (**`+f`**): If set (default), the shell performs [pathname expansion](language/words/globbing.md) on words containing wildcard characters like `*`, `?`, and `[...]`. If unset, pathname expansion is skipped.

- **`hashondefinition`** (**`-h`**): This option is deprecated and has no effect. It remains for compatibility with older versions of yash and other shells.
    - Currently, the short name `-h` is a synonym for the long name `--hashondefinition`, but this may change in the future.
    - Many shells implement the `-h` option in different ways, so you cannot expect consistent behavior across different shells.

- **`ignoreeof`**: If set, the shell ignores the end-of-file (EOF) character (usually `Ctrl+D`) and does not exit. This is useful to prevent accidental exits from the shell. <!-- TODO: link to interactive shell and termination -->
    - This option takes effect only if the shell is interactive and the input is a terminal.

- **`interactive`** (**`-i`**): If set, the shell is interactive. <!-- TODO: link to interactive -->
    - This option is enabled on startup if the `stdin` option is enabled and standard input and error are connected to a terminal.

- **`log`**: This option is deprecated and has no effect. It remains for compatibility with older versions of yash.

- **`login`** (**`-l`**): If set, the shell behaves as a login shell. This option can only be set at startup. <!-- TODO: link to startup -->
    - ⚠️ Currently, this option has no effect in yash-rs. In the future, login shells will read additional initialization files.

- **`monitor`** (**`-m`**): If set, the shell performs job control, allowing you to manage background jobs and suspended processes. <!-- TODO: link to job control -->
    - This option is enabled by default in interactive shells.

- **`notify`** (**`-b`**): If set, the shell notifies you of background job completions and suspensions as soon as they occur. If unset, job status notifications are delayed until the next command prompt. <!-- TODO: link to job control -->
    - ⚠️ This option is not yet implemented in yash-rs.
    - This option takes effect only if the `interactive` and `monitor` options are enabled.

- **`posixlycorrect`**: If set, the shell behaves in a POSIX-compliant manner to the maximum extent possible. This option is useful for scripts that need to be portable across different POSIX-compliant shells. <!-- TODO: link to POSIX compliance -->
    - This option is enabled on startup if the shell is started as `sh`, that is, the basename of the first argument to the shell is `sh`.
    - When this option is unset, yash-rs may deviate from POSIX compliance in some areas.

- **`stdin`** (**`-s`**): If set, the shell reads commands from standard input. This option is mutually exclusive with the `cmdline` option, and can only be set at startup. <!-- TODO: Link to startup -->
    - This option is enabled if the `cmdline` option is not set and the shell is started with no operands.

- **`unset`** (**`+u`**): If set (default), the shell [expands](language/words/parameters.md) unset [variables] to an empty string. If unset, the shell raises an error when trying to expand an unset variable. This is useful for catching typos or missing variable assignments in scripts.

- **`verbose`** (**`-v`**): If set, the shell prints each command before executing it. This is useful for debugging scripts and understanding what commands are being run. <!-- TODO: link to debugging -->

- **`vi`**: If set, the shell uses vi-style keybindings for command line editing. <!-- TODO: link to interactive shell and command line editing -->
    <!-- TODO: - This option is enabled on startup if ... -->
    - ⚠️ This option is not yet implemented in yash-rs.

- **`xtrace`** (**`-x`**): If set, the shell prints each field after [expansion](language/words/index.html#word-expansion), before executing it. This is useful for debugging scripts and understanding how commands are being executed. <!-- TODO: link to debugging -->

## Compatibility

FIXME TODO

[variables]: language/parameters/variables.md
